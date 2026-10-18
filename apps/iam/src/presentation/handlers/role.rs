use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use tracing::instrument;

use crate::application::rbac::{
    CreateGuildRoleCommand, CreateGuildRoleUseCase, DeleteGuildRoleUseCase, GetGuildRoleUseCase,
    ListGuildRolesUseCase, ListPermissionsUseCase, ReplaceRolePermissionsCommand,
    ReplaceRolePermissionsUseCase, RoleView, UpdateGuildRoleCommand, UpdateGuildRoleUseCase,
};
use crate::presentation::dto::role::{
    CreateRoleRequest, PermissionResponse, ReplacePermissionsRequest, RoleResponse,
    UpdateRoleRequest,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::{authorize, parse_uuid};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::guild_id_from_path;

/// The permission catalog, for the matrix editor's rows.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list_permissions(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<PermissionResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "roles:read").await?;

    let use_case = ListPermissionsUseCase::new(state.permissions.clone());
    let permissions = use_case.execute().await?;

    Ok(Json(permissions.into_iter().map(Into::into).collect()))
}

/// The guild's roles and their permission matrices.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list_roles(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<RoleResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "roles:read").await?;

    let use_case = ListGuildRolesUseCase::new(state.guild_roles.clone());
    let roles = use_case.execute(guild_id).await?;

    Ok(Json(roles.into_iter().map(role_response).collect()))
}

/// One role.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, role = %role_id))]
pub async fn get_role(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, role_id)): Path<(String, String)>,
) -> Result<Json<RoleResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let role_id = parse_uuid(&role_id, "roleId")?;
    authorize(&state, actor.id, guild_id, "roles:read").await?;

    let use_case = GetGuildRoleUseCase::new(state.guild_roles.clone());
    let role = use_case.execute(guild_id, role_id).await?;

    Ok(Json(role_response(role)))
}

/// Creates a role.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn create_role(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<CreateRoleRequest>,
) -> Result<(StatusCode, Json<RoleResponse>), AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "roles:write").await?;

    let use_case = CreateGuildRoleUseCase::new(state.guild_roles.clone());
    let role = use_case
        .execute(CreateGuildRoleCommand {
            guild_id,
            slug: request.slug,
            name: request.name,
            description: request.description,
            position: request.position.unwrap_or(0),
        })
        .await?;

    // A role is created with no permissions: granting them is the matrix editor's
    // job, and defaulting to anything would be a silent privilege grant.
    Ok((
        StatusCode::CREATED,
        Json(RoleResponse::new(role, Vec::new(), 0)),
    ))
}

/// Renames a role or moves it in the ordering.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, role = %role_id))]
pub async fn update_role(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, role_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<UpdateRoleRequest>,
) -> Result<Json<RoleResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let role_id = parse_uuid(&role_id, "roleId")?;
    authorize(&state, actor.id, guild_id, "roles:write").await?;

    let use_case = UpdateGuildRoleUseCase::new(state.guild_roles.clone());
    use_case
        .execute(UpdateGuildRoleCommand {
            guild_id,
            role_id,
            name: request.name,
            description: request.description,
            position: request.position,
        })
        .await?;

    // Re-read through the detail use case so the response carries the matrix and
    // the member count, exactly like the GET.
    let detail = GetGuildRoleUseCase::new(state.guild_roles.clone());
    Ok(Json(role_response(
        detail.execute(guild_id, role_id).await?,
    )))
}

/// Deletes a role.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, role = %role_id))]
pub async fn delete_role(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, role_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let role_id = parse_uuid(&role_id, "roleId")?;
    authorize(&state, actor.id, guild_id, "roles:write").await?;

    let use_case =
        DeleteGuildRoleUseCase::new(state.guild_roles.clone(), state.guild_members.clone());
    use_case.execute(guild_id, role_id).await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Replaces a role's permission matrix.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, role = %role_id))]
pub async fn replace_permissions(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, role_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<ReplacePermissionsRequest>,
) -> Result<Json<RoleResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let role_id = parse_uuid(&role_id, "roleId")?;
    authorize(&state, actor.id, guild_id, "roles:write").await?;

    let use_case =
        ReplaceRolePermissionsUseCase::new(state.guild_roles.clone(), state.guild_members.clone());

    let role = use_case
        .execute(ReplaceRolePermissionsCommand {
            guild_id,
            role_id,
            permissions: request.permissions,
        })
        .await?;

    Ok(Json(role_response(role)))
}

/// Flattens the use case's view into the wire shape.
fn role_response(view: RoleView) -> RoleResponse {
    RoleResponse::new(view.role, view.permissions, view.member_count)
}
