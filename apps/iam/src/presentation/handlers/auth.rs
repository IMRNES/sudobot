use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use axum::Json;
use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use tracing::{instrument, warn};

use crate::application::auth::{
    DiscordLoginCommand, DiscordLoginUseCase, LogoutCommand, LogoutUseCase, RefreshCommand,
    RefreshUseCase, SyncGuildsCommand, SyncGuildsUseCase,
};
use crate::application::ports::Clock;
use crate::application::user::GetMeUseCase;
use crate::domain::auth::{AuthError, OAUTH_STATE_TTL_SECS, generate_state};
use crate::presentation::dto::auth::{
    AuthorizeResponse, CallbackRequest, LoginResponse, LogoutRequest, LogoutResponse, MeResponse,
    RefreshRequest, RefreshResponse, UserResponse,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::{ValidatedJson, ValidatedRequest};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;

/// Server-minted OAuth `state` values, waiting for their callback.
///
/// The state is generated here, not by the browser, and never leaves the server
/// until the callback presents it back — which is what makes it a defence against
/// a forged callback rather than decoration. In-process and short-lived by design:
/// a value that only has to survive one redirect does not deserve a table, and a
/// stolen database dump then cannot be replayed as a login. A multi-replica
/// deployment moves this to Redis; that is a deployment decision, not an oversight.
fn pending_states() -> &'static Mutex<HashMap<String, DateTime<Utc>>> {
    static STATES: OnceLock<Mutex<HashMap<String, DateTime<Utc>>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Hands the browser a Discord consent URL.
///
/// Takes no body: the server mints the `state` and returns it alongside the URL, so
/// the value never round-trips through the browser before it is signed. A client
/// that sends `{}` is not punished for it — axum ignores a body a handler does not
/// declare — but nothing in it is read.
#[instrument(skip_all)]
pub async fn authorize(State(state): State<AppState>) -> Result<Json<AuthorizeResponse>, AppError> {
    // Checked before minting a state: sending the user to Discord's consent screen
    // when the exchange is guaranteed to fail strands them on an error page with
    // no explanation. An immediate refusal names the actual problem.
    if !state.oauth_client.is_configured() {
        return Err(AppError::from(AuthError::LoginNotConfigured));
    }

    let now = state.clock.now();
    let oauth_state = generate_state();

    // A poisoned lock here would mean the map is corrupt, which cannot happen:
    // nothing inside the critical section can panic.
    let mut states = pending_states()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Sweep on every issue rather than running a background task: the map holds one
    // entry per login in flight, so it is tiny and a timer would be more moving
    // parts than the problem deserves.
    states.retain(|_, issued_at| {
        now.signed_duration_since(*issued_at).num_seconds() <= OAUTH_STATE_TTL_SECS
    });
    states.insert(oauth_state.clone(), now);
    drop(states);

    Ok(Json(AuthorizeResponse {
        authorize_url: state.oauth_client.authorize_url(&oauth_state),
        state: oauth_state,
    }))
}

/// Completes a login.
#[axum::debug_handler]
#[instrument(skip_all)]
pub async fn callback(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<CallbackRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    consume_state(&state, &request.state)?;

    let use_case = DiscordLoginUseCase::new(
        state.oauth_client.clone(),
        state.users.clone(),
        state.refresh_tokens.clone(),
        state.token_service.clone(),
        state.clock,
    );

    let result = use_case
        .execute(DiscordLoginCommand { code: request.code })
        .await?;

    let expires_in = (result.tokens.access_expires_at - state.clock.now()).num_seconds();

    // The Discord token is returned so the client can immediately import its
    // guilds with `POST /auth/sync-guilds`. It is short-lived at Discord's end and
    // never touches our database.
    Ok(Json(LoginResponse {
        user: result.user.into(),
        access_token: result.tokens.access_token,
        refresh_token: result.tokens.refresh_token,
        expires_in,
        discord_access_token: result.discord_access_token,
        guilds: Vec::new(),
    }))
}

/// Imports the caller's guilds from Discord, then returns their session view.
///
/// A separate step from the callback so a Discord hiccup while listing guilds
/// cannot fail a sign-in that has already succeeded.
#[instrument(skip_all, fields(user = %actor.id))]
pub async fn sync_guilds(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    ValidatedJson(request): ValidatedJson<SyncGuildsRequest>,
) -> Result<Json<MeResponse>, AppError> {
    let sync = SyncGuildsUseCase::new(
        state.oauth_client.clone(),
        state.guilds.clone(),
        state.guild_members.clone(),
        state.guild_roles.clone(),
        state.clock,
    );

    let outcome = sync
        .execute(SyncGuildsCommand {
            user_id: actor.id,
            discord_access_token: request.discord_access_token.clone(),
        })
        .await?;

    // The token has now done its job. Revoking is best-effort: a failure leaves it
    // valid at Discord until it expires, which is worth a log line and not an error
    // in the caller's face.
    let login = DiscordLoginUseCase::new(
        state.oauth_client.clone(),
        state.users.clone(),
        state.refresh_tokens.clone(),
        state.token_service.clone(),
        state.clock,
    );
    if let Err(error) = login
        .revoke_discord_token(&request.discord_access_token)
        .await
    {
        warn!(%error, "guild sync finished but the discord token could not be revoked");
    }

    tracing::debug!(
        manageable = outcome.manageable.len(),
        ignored = outcome.ignored.len(),
        "guilds imported"
    );

    me(State(state), actor).await
}

/// The signed-in account and the guilds they administer.
#[instrument(skip_all, fields(user = %actor.id))]
pub async fn me(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
) -> Result<Json<MeResponse>, AppError> {
    let use_case = GetMeUseCase::new(
        state.users.clone(),
        state.guilds.clone(),
        state.guild_members.clone(),
        state.guild_roles.clone(),
    );

    let view = use_case.execute(actor.id).await?;

    Ok(Json(MeResponse {
        user: UserResponse::from(view.user),
        guilds: view.guilds.into_iter().map(Into::into).collect(),
    }))
}

/// Exchanges a refresh token for a new pair.
#[instrument(skip_all)]
pub async fn refresh(
    State(state): State<AppState>,
    ValidatedJson(request): ValidatedJson<RefreshRequest>,
) -> Result<Json<RefreshResponse>, AppError> {
    let use_case = RefreshUseCase::new(
        state.refresh_tokens.clone(),
        state.token_service.clone(),
        state.clock,
    );

    let pair = use_case
        .execute(RefreshCommand {
            refresh_token: request.refresh_token,
        })
        .await?;

    Ok(Json(RefreshResponse {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        expires_in: (pair.access_expires_at - state.clock.now()).num_seconds(),
    }))
}

/// Ends a session.
#[instrument(skip_all, fields(user = %actor.id))]
pub async fn logout(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    ValidatedJson(request): ValidatedJson<LogoutRequest>,
) -> Result<Json<LogoutResponse>, AppError> {
    let use_case = LogoutUseCase::new(state.refresh_tokens.clone(), state.clock);

    let revoked = use_case
        .execute(LogoutCommand {
            refresh_token: request.refresh_token,
            user_id: actor.id,
            everywhere: request.everywhere,
        })
        .await?;

    Ok(Json(LogoutResponse { revoked }))
}

/// Consumes a pending state, refusing anything this server did not issue.
///
/// `remove` rather than `get`: a state is single-use, so a replayed callback finds
/// nothing and is rejected even within the validity window.
fn consume_state(state: &AppState, received: &str) -> Result<(), AppError> {
    let now = state.clock.now();

    let issued_at = pending_states()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(received);

    let Some(issued_at) = issued_at else {
        warn!("callback presented a state this server never issued");
        return Err(AppError::from(AuthError::StateMismatch));
    };

    if now.signed_duration_since(issued_at).num_seconds() > OAUTH_STATE_TTL_SECS {
        return Err(AppError::from(AuthError::StateExpired));
    }

    Ok(())
}

/// Body for the guild import.
#[derive(Debug, Deserialize)]
pub struct SyncGuildsRequest {
    pub discord_access_token: String,
}

impl ValidatedRequest for SyncGuildsRequest {
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
        match value
            .get("discord_access_token")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
        {
            Some(token) if !token.is_empty() => Ok(()),
            _ => Err(vec![("discord_access_token".into(), "is required".into())]),
        }
    }
}
