use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use axum::Json;
use axum::extract::{RawQuery, State};
use axum::response::Redirect;
use chrono::{DateTime, Utc};
use tracing::{debug, info, instrument, warn};

use crate::application::auth::{
    DashboardMembershipUseCase, DiscordLoginCommand, DiscordLoginUseCase, LogoutCommand,
    LogoutUseCase, RefreshCommand, RefreshUseCase,
};
use crate::application::ports::Clock;
use crate::domain::auth::{AuthError, OAUTH_STATE_TTL_SECS, generate_state};
use crate::domain::user::UserRepository;
use crate::presentation::dto::auth::{
    AuthorizeResponse, CallbackRequest, LoginResponse, LogoutRequest, LogoutResponse, MeResponse,
    RefreshRequest, RefreshResponse, UserResponse,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
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

/// Discord's redirect, when the registered URI points at the API instead of the
/// dashboard.
///
/// The exchange itself is `POST /auth/discord/callback`, which the dashboard calls
/// with the `code` it read from its own URL. That works only if Discord sends the
/// browser to the dashboard page — but the URI is registered in the Developer
/// Portal, which only a human can change, and Discord matches it literally. A
/// deployment whose registration still names this path would otherwise answer the
/// redirect with `405 Method Not Allowed` *after* the user approved the app, with
/// the authorization code already spent and no way to retry it.
///
/// So this forwards: same query string, one relative hop to the dashboard's own
/// callback route, which then does the exchange. Relative because the dashboard and
/// the API share an origin in this deployment, and a relative `Location` cannot be
/// wrong about the host the browser used.
#[instrument(skip_all)]
pub async fn callback_redirect(RawQuery(query): RawQuery) -> Result<Redirect, AppError> {
    // The query is forwarded verbatim, not rebuilt: `state` is compared byte for
    // byte against the value the server minted, so re-encoding it here could
    // change it. `RawQuery` keeps the original bytes.
    let target = match query {
        Some(query) if !query.is_empty() => format!("/auth/callback?{query}"),
        _ => "/auth/callback".to_string(),
    };

    Ok(Redirect::to(&target))
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

    // Checked before the exchange: without a configured guild there is no way to
    // decide who is staff, so every login would fail at the whitelist with a
    // denial that looks like a missing role. Naming the real problem up front
    // turns an afternoon of Discord-permission debugging into one log line.
    let Some(access) = state.access.clone() else {
        return Err(AppError::from(AuthError::LoginNotConfigured));
    };

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

    // The gate has already passed — `execute` refuses otherwise — so this records
    // the membership row the guild-scoped routes authorize against.
    let membership = DashboardMembershipUseCase::new(
        state.guilds.clone(),
        state.guild_members.clone(),
        state.guild_roles.clone(),
        state.clock,
    );

    let guild = membership.execute(result.user.id, access.guild_id).await?;

    // The verdict is logged rather than returned: the client only ever sees an
    // admitted login, so this line is the one place that records *why* someone got
    // in, which is what an operator needs when a role is later stripped.
    info!(
        user_id = %result.user.id,
        discord_id = result.user.discord_id,
        guild_id = access.guild_id,
        "login admitted by the dashboard whitelist"
    );
    debug!(verdict = ?result.membership, "whitelist verdict");

    let expires_in = (result.tokens.access_expires_at - state.clock.now()).num_seconds();

    Ok(Json(LoginResponse {
        user: result.user.into(),
        access_token: result.tokens.access_token,
        refresh_token: result.tokens.refresh_token,
        expires_in,
        guilds: vec![guild.into()],
    }))
}

/// The signed-in account and the guild this dashboard serves.
///
/// The guild list is derived, not stored per request: `DASHBOARD_GUILD_ID` is the
/// only guild, and a membership row proves the caller was admitted. A signed-in
/// caller with no membership row is one whose login predates the whitelist — they
/// are sent through the gate again rather than shown an empty dashboard.
#[instrument(skip_all, fields(user = %actor.id))]
pub async fn me(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
) -> Result<Json<MeResponse>, AppError> {
    let Some(access) = state.access.clone() else {
        return Err(AppError::from(AuthError::LoginNotConfigured));
    };

    let use_case = DashboardMembershipUseCase::new(
        state.guilds.clone(),
        state.guild_members.clone(),
        state.guild_roles.clone(),
        state.clock,
    );

    // Recorded idempotently on every `/me`: a session that outlives a role change
    // is re-checked here, and the upsert is a single statement.
    let guild = use_case.execute(actor.id, access.guild_id).await?;

    let user = state
        .users
        .find_by_id(actor.id)
        .await?
        .ok_or_else(|| AppError::from(AuthError::UserNotFound))?;

    info!(user_id = %actor.id, guild_id = access.guild_id, "session resolved");

    Ok(Json(MeResponse {
        user: UserResponse::from(user),
        guilds: vec![guild.into()],
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
