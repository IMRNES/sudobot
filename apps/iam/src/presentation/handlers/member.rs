use axum::Json;
use axum::extract::{Path, State};
use paginator_axum::PaginationQuery;
use tracing::instrument;
use uuid::Uuid;

use crate::application::guild_member::{
    ListGuildMembersUseCase, RemoveGuildMemberUseCase, UpdateGuildMemberRoleCommand,
    UpdateGuildMemberRoleUseCase,
};
use crate::domain::guild::GuildRepository;
use crate::domain::rbac::GuildRoleRepository;
use crate::domain::user::UserRepository;
use crate::presentation::dto::PageResponse;
use crate::presentation::dto::member::{MemberResponse, UpdateMemberRoleRequest, member_response};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::{authorize, parse_uuid, to_pagination};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::guild_id_from_path;

/// Members of a guild, with the dashboard role each holds.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    PaginationQuery(query): PaginationQuery,
) -> Result<Json<PageResponse<MemberResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "members:read").await?;

    let use_case = ListGuildMembersUseCase::new(state.guild_members.clone());
    let page = use_case.execute(guild_id, &to_pagination(&query)).await?;

    let owner_discord_id = state
        .guilds
        .find_by_id(guild_id)
        .await?
        .and_then(|guild| guild.owner_discord_id);

    // The account fields are not part of the membership join, so they are fetched
    // per row here. A page is at most 100 rows and this is a dashboard list, not a
    // hot path; the comment exists so the next reader does not assume it is free.
    let mut data = Vec::with_capacity(page.data.len());
    for member in &page.data {
        let account = state.users.find_by_id(member.member.user_id).await?;
        data.push(member_response(member, account.as_ref(), owner_discord_id));
    }

    Ok(Json(PageResponse {
        data,
        meta: crate::presentation::dto::PageMeta::new(page.page, page.per_page, page.total),
    }))
}

/// Changes which dashboard role a member holds.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, target = %user_id))]
pub async fn update_role(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, user_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<UpdateMemberRoleRequest>,
) -> Result<Json<MemberResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let target_user_id = parse_uuid(&user_id, "userId")?;
    authorize(&state, actor.id, guild_id, "members:write").await?;

    let use_case = UpdateGuildMemberRoleUseCase::new(
        state.guild_members.clone(),
        state.guild_roles.clone(),
        state.guilds.clone(),
    );

    let member = use_case
        .execute(UpdateGuildMemberRoleCommand {
            guild_id,
            actor_user_id: actor.id,
            target_user_id,
            guild_role_id: parse_uuid(&request.guild_role_id, "guild_role_id")?,
        })
        .await?;

    let role = state
        .guild_roles
        .find_by_id(member.guild_role_id)
        .await?
        .ok_or_else(|| AppError::NotFound("the assigned role no longer exists".into()))?;

    let account = state.users.find_by_id(member.user_id).await?;
    let owner_discord_id = state
        .guilds
        .find_by_id(guild_id)
        .await?
        .and_then(|guild| guild.owner_discord_id);

    let joined = crate::domain::guild_member::GuildMemberWithRole { member, role };
    Ok(Json(member_response(
        &joined,
        account.as_ref(),
        owner_discord_id,
    )))
}

/// Removes a member's dashboard access.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, target = %user_id))]
pub async fn remove(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, user_id)): Path<(String, String)>,
) -> Result<axum::http::StatusCode, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let target_user_id: Uuid = parse_uuid(&user_id, "userId")?;
    authorize(&state, actor.id, guild_id, "members:write").await?;

    let use_case =
        RemoveGuildMemberUseCase::new(state.guild_members.clone(), state.guild_roles.clone());
    use_case.execute(guild_id, actor.id, target_user_id).await?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}
