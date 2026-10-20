use axum::Json;
use axum::extract::{Path, State};
use paginator_axum::PaginationQuery;
use tracing::instrument;

use crate::application::guild::{
    GuildDetailUseCase, GuildDirectoryUseCase, ListGuildsUseCase, UpdateGuildCommand,
    UpdateGuildUseCase,
};
use crate::domain::user::UserRepository;
use crate::presentation::dto::PageResponse;
use crate::presentation::dto::auth::{GuildResponse, UpdateGuildRequest};
use crate::presentation::dto::guild::GuildDirectoryResponse;
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::authorize;
use crate::presentation::middleware::{
    AuthenticatedUser, ensure_any_permission, load_guild_context,
};
use crate::presentation::state::AppState;
use crate::presentation::validation::guild_id_from_path;

/// Every guild the bot is in. Platform-admin only.
#[instrument(skip_all, fields(actor = %actor.id))]
pub async fn list(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    PaginationQuery(query): PaginationQuery,
) -> Result<Json<PageResponse<GuildResponse>>, AppError> {
    // Guilds the caller is not a member of are visible only to a platform admin;
    // for everyone else the guild picker at `/me` is the right endpoint.
    let user = state
        .users
        .find_by_id(actor.id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("your account no longer exists".into()))?;

    if !user.is_platform_admin() {
        return Err(AppError::Forbidden(
            "listing every guild is restricted to platform administrators".into(),
        ));
    }

    let use_case = ListGuildsUseCase::new(state.guilds.clone());
    let params = crate::presentation::handlers::to_pagination(&query);
    let page = use_case.execute(&params).await?;

    Ok(Json(page.map(GuildResponse::from).into()))
}

/// One guild's detail.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn detail(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<GuildResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "members:read").await?;

    let use_case = GuildDetailUseCase::new(state.guilds.clone());
    let guild = use_case.execute(guild_id).await?;

    Ok(Json(guild.into()))
}

/// Renames a guild.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn update(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<UpdateGuildRequest>,
) -> Result<Json<GuildResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "dashboard:manage").await?;

    let use_case = UpdateGuildUseCase::new(state.guilds.clone());
    let guild = use_case
        .execute(UpdateGuildCommand {
            guild_id,
            name: request.name,
        })
        .await?;

    Ok(Json(guild.into()))
}

/// Every role and channel in the guild, with names.
///
/// Gated on holding *any* feature-read permission rather than one: this is shared
/// reference data, and every configuration screen needs it to render a picker.
/// Requiring `leveling:read` here would break the welcome page for a role that
/// only holds `welcome:read`, and nothing about the directory is more sensitive
/// than the ids the caller can already read from the config it is decorating.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn directory(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<GuildDirectoryResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;

    let context = load_guild_context(&state, actor.id, guild_id).await?;
    ensure_any_permission(
        &context,
        &[
            "leveling:read",
            "economy:read",
            "welcome:read",
            "autorole:read",
            "roles:read",
            "members:read",
        ],
    )?;

    let use_case = GuildDirectoryUseCase::new(state.guild_directory.clone());
    let directory = use_case.execute(guild_id).await?;

    Ok(Json(directory.into()))
}
