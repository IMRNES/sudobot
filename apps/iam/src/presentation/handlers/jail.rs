//! The jail's HTTP handlers.
//!
//! Two responsibilities that could not be further apart in cost: reading and writing
//! a small row, and writing a permission overwrite into every channel in a guild. The
//! second is why every write path here seals *after* the database agrees — a row that
//! claims a jail exists while Discord says otherwise is the worst of both states,
//! because the dashboard would show a working jail that lets everyone talk.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::{info, instrument};

use crate::application::jail::{
    GetJailConfigUseCase, JailMemberCommand, JailMemberUseCase, ListSentencesUseCase,
    ReleaseMemberCommand, ReleaseMemberUseCase, UpdateJailConfigCommand, UpdateJailConfigUseCase,
};
use crate::application::ports::{Clock as _, JailEnforcer};
use crate::domain::jail::JailRepository;
use crate::domain::user::UserRepository as _;
use crate::presentation::dto::jail::{
    JailConfigResponse, JailMemberRequest, ReleaseResponse, SentenceResponse,
    UpdateJailConfigRequest,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::authorize;
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::guild_id_from_path;

/// The guild's jail.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn get_config(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<JailConfigResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "members:read").await?;

    let use_case = GetJailConfigUseCase::new(state.jail.clone());
    let config = use_case.execute(guild_id).await?;

    Ok(Json(config.into()))
}

/// Changes the jail and re-seals Discord to match.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn update_config(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<UpdateJailConfigRequest>,
) -> Result<Json<JailConfigResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "members:write").await?;

    // The outgoing role, before the write, so a change of role can unseal the old one.
    let previous = state.jail.find_config(guild_id).await?.jail_role_id;

    let use_case = UpdateJailConfigUseCase::new(state.jail.clone());
    let config = use_case
        .execute(UpdateJailConfigCommand {
            guild_id,
            jail_role_id: request.jail_role_id,
            jail_channel_id: request.jail_channel_id,
            default_sentence_seconds: request.default_sentence_seconds,
            bail_amount: request.bail_amount,
        })
        .await?;

    let response: JailConfigResponse = config.clone().into();

    // Only seal when there is a role to seal with. Clearing the jail must not fail on
    // a Discord call — the admin's intent is "no jail", and the row already says so.
    if let Some(role_id) = config.jail_role_id {
        if let Some(previous) = previous.filter(|previous| *previous != role_id) {
            // Best effort: an old role left behind is a stale permission grant, not a
            // reason to reject a correctly stored config. Reported, not fatal.
            if let Err(error) = state.jail_enforcer.unseal(guild_id, previous).await {
                tracing::warn!(guild_id, role_id = previous, %error, "could not unseal the previous jail role");
            }
        }

        let report = state
            .jail_enforcer
            .seal(guild_id, role_id, config.jail_channel_id)
            .await
            .map_err(AppError::from)?;

        // A partial seal is reported rather than hidden: the config is stored, and the
        // admin needs to know the bot's role sits too low for some channels.
        if !report.is_clean() {
            tracing::warn!(
                guild_id,
                written = report.written,
                failed = report.failed,
                "the jail was stored but some channels refused the overwrite"
            );
        }

        info!(
            guild_id,
            role_id,
            written = report.written,
            failed = report.failed,
            "jail configuration applied"
        );
    }

    Ok(Json(response))
}

/// Everyone currently serving a sentence.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list_sentences(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<SentenceResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "members:read").await?;

    let now = state.clock.now();
    let use_case = ListSentencesUseCase::new(state.jail.clone());
    let sentences = use_case.execute(guild_id).await?;

    Ok(Json(
        sentences
            .into_iter()
            .map(|sentence| SentenceResponse::new(sentence, now))
            .collect(),
    ))
}

/// Jails a member: the row first, then Discord.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn jail_member(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<JailMemberRequest>,
) -> Result<(StatusCode, Json<SentenceResponse>), AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "members:write").await?;

    // `jailed_by` is a Discord id — it is shown to the prisoner and written to the
    // audit trail — but the session only carries the user row's UUID.
    let jailed_by = state
        .users
        .find_by_id(actor.id)
        .await?
        .map(|user| user.discord_id)
        .ok_or_else(|| AppError::Unauthorized("your account is not linked to discord".into()))?;

    let use_case = JailMemberUseCase::new(state.jail.clone(), state.clock);
    let outcome = use_case
        .execute(JailMemberCommand {
            guild_id,
            discord_user_id: request.discord_user_id,
            jailed_by,
            reason: request.reason,
            duration_seconds: request.duration_seconds,
        })
        .await?;

    let now = state.clock.now();
    let response = SentenceResponse::new(outcome.imprisonment.clone(), now);

    // The row is written; the role is best effort and reported. A Discord failure here
    // leaves a sentence the sweeper will reconcile on its next pass — which is the
    // reason the sweeper re-applies roles rather than only releasing them.
    if let Some(role_id) = outcome.config.jail_role_id
        && let Err(error) = state
            .jail_enforcer
            .put_in_cell(guild_id, request.discord_user_id, role_id)
            .await
    {
        tracing::warn!(
            guild_id,
            user_id = request.discord_user_id,
            role_id,
            %error,
            "sentence stored but the jail role could not be granted; the sweeper will retry"
        );
    }

    info!(
        guild_id,
        user_id = request.discord_user_id,
        "member jailed from the dashboard"
    );

    Ok((StatusCode::CREATED, Json(response)))
}

/// Releases a member.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn release_member(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, discord_user_id)): Path<(String, String)>,
) -> Result<Json<ReleaseResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let discord_user_id =
        crate::presentation::validation::parse_snowflake(&discord_user_id, "discordUserId")?;
    authorize(&state, actor.id, guild_id, "members:write").await?;

    let released_by = state
        .users
        .find_by_id(actor.id)
        .await?
        .map(|user| user.discord_id);

    let use_case = ReleaseMemberUseCase::new(state.jail.clone());
    let outcome = use_case
        .execute(ReleaseMemberCommand {
            guild_id,
            discord_user_id,
            cause: crate::application::jail::ReleaseCause::Pardoned,
            released_by,
        })
        .await?;
    let was_imprisoned = outcome.imprisonment.is_some();

    // The role comes off even when there was no row: the moderator's intent is that
    // this member is not in jail, and a stray role from a hand-applied grant is
    // exactly what they are trying to clear.
    if let Some(role_id) = state.jail.find_config(guild_id).await?.jail_role_id
        && let Err(error) = state
            .jail_enforcer
            .let_out(guild_id, discord_user_id, role_id)
            .await
    {
        tracing::warn!(
            guild_id,
            user_id = discord_user_id,
            role_id,
            %error,
            "could not remove the jail role; the sweeper will reconcile"
        );
    }

    info!(
        guild_id,
        user_id = discord_user_id,
        "member released from the dashboard"
    );

    Ok(Json(ReleaseResponse { was_imprisoned }))
}
