use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::instrument;

use crate::application::autorole::{
    CreateAutoRoleCommand, CreateAutoRoleUseCase, DeleteAutoRoleUseCase, ListAutoRolesUseCase,
    UpdateAutoRoleCommand, UpdateAutoRoleUseCase,
};
use crate::presentation::dto::autorole::{
    AutoRoleResponse, CreateAutoRoleRequest, UpdateAutoRoleRequest,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::{authorize, parse_uuid};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::{guild_id_from_path, parse_snowflake};

/// The guild's automatic roles.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<AutoRoleResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "autorole:read").await?;

    let use_case = ListAutoRolesUseCase::new(state.auto_roles.clone());
    let rules = use_case.execute(guild_id).await?;

    Ok(Json(rules.into_iter().map(Into::into).collect()))
}

/// Adds an automatic role.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn create(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<CreateAutoRoleRequest>,
) -> Result<(StatusCode, Json<AutoRoleResponse>), AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let use_case = CreateAutoRoleUseCase::new(state.auto_roles.clone());
    let rule = use_case
        .execute(CreateAutoRoleCommand {
            guild_id,
            role_id: parse_snowflake(&request.role_id, "role_id")?,
            trigger: request.trigger,
            delay_seconds: request.delay_seconds,
            applies_to_bots: request.applies_to_bots,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(rule.into())))
}

/// Changes a rule.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, rule = %rule_id))]
pub async fn update(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, rule_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<UpdateAutoRoleRequest>,
) -> Result<Json<AutoRoleResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let rule_id = parse_uuid(&rule_id, "id")?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let use_case = UpdateAutoRoleUseCase::new(state.auto_roles.clone());
    let rule = use_case
        .execute(UpdateAutoRoleCommand {
            id: rule_id,
            trigger: request.trigger,
            delay_seconds: request.delay_seconds,
            applies_to_bots: request.applies_to_bots,
            is_active: request.is_active,
        })
        .await?;

    Ok(Json(rule.into()))
}

/// Removes a rule.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, rule = %rule_id))]
pub async fn delete(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, rule_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let rule_id = parse_uuid(&rule_id, "id")?;
    authorize(&state, actor.id, guild_id, "autorole:write").await?;

    let use_case = DeleteAutoRoleUseCase::new(state.auto_roles.clone());
    use_case.execute(guild_id, rule_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
