use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use paginator_axum::{PaginatedJson, PaginationQuery};
use tracing::instrument;

use crate::application::leveling::{
    CreateLevelRewardCommand, CreateLevelRewardUseCase, DeleteLevelRewardUseCase,
    GetLevelingConfigUseCase, LevelLeaderboardUseCase, ListLevelRewardsUseCase,
    UpdateLevelingConfigCommand, UpdateLevelingConfigUseCase,
};
use crate::presentation::dto::leveling::{
    CreateLevelRewardRequest, LeaderboardRowResponse, LevelRewardResponse, LevelingConfigResponse,
    UpdateLevelingConfigRequest,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::{authorize, parse_uuid, to_pagination};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::{guild_id_from_path, parse_snowflake};

/// The guild's XP configuration.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn get_config(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<LevelingConfigResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "leveling:read").await?;

    let use_case = GetLevelingConfigUseCase::new(state.leveling.clone());
    let config = use_case.execute(guild_id).await?;

    Ok(Json(config.into()))
}

/// Changes the XP configuration.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn update_config(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<UpdateLevelingConfigRequest>,
) -> Result<Json<LevelingConfigResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "leveling:write").await?;

    // The channel id arrives as a string because it is a snowflake; the double
    // option is preserved so "clear the announcement channel" stays expressible.
    let announce_channel_id = match request.announce_channel_id {
        Some(Some(raw)) => Some(Some(parse_snowflake(&raw, "announce_channel_id")?)),
        Some(None) => Some(None),
        None => None,
    };

    // Same shape as the announcement channel: absent leaves it alone, null clears it.
    let ladder_base_role_id = match request.ladder_base_role_id {
        None => None,
        Some(None) => Some(None),
        Some(Some(raw)) => Some(Some(parse_snowflake(&raw, "ladder_base_role_id")?)),
    };

    let ignored_channel_ids = request
        .ignored_channel_ids
        .map(|ids| {
            ids.iter()
                .map(|raw| parse_snowflake(raw, "ignored_channel_ids"))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    let ignored_role_ids = request
        .ignored_role_ids
        .map(|ids| {
            ids.iter()
                .map(|raw| parse_snowflake(raw, "ignored_role_ids"))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;

    let use_case = UpdateLevelingConfigUseCase::new(state.leveling.clone());
    let config = use_case
        .execute(UpdateLevelingConfigCommand {
            guild_id,
            enabled: request.enabled,
            xp_min: request.xp_min,
            xp_max: request.xp_max,
            cooldown_secs: request.cooldown_secs,
            voice_xp_multiplier: request.voice_xp_multiplier,
            curve: request.curve,
            announce_channel_id,
            stack_roles: request.stack_roles,
            ladder_base_role_id,
            ignored_channel_ids,
            ignored_role_ids,
        })
        .await?;

    Ok(Json(config.into()))
}

/// The guild's XP leaderboard.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn leaderboard(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    PaginationQuery(query): PaginationQuery,
) -> Result<PaginatedJson<LeaderboardRowResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "leveling:read").await?;

    let use_case = LevelLeaderboardUseCase::new(state.leveling.clone());
    let page = use_case.execute(guild_id, &to_pagination(&query)).await?;

    // Built by hand rather than through the paginator's SeaORM adapter: the ranks
    // are computed from the page offset, so the rows are already domain values by
    // the time they reach here.
    let total = u32::try_from(page.total).unwrap_or(u32::MAX);
    Ok(PaginatedJson::new(
        page.data
            .into_iter()
            .map(LeaderboardRowResponse::from)
            .collect(),
        &paginator_rs::PaginationParams {
            page: query.page,
            per_page: query.per_page,
            ..Default::default()
        },
        total,
    ))
}

/// Reward roles configured for this guild.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list_rewards(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<LevelRewardResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "leveling:read").await?;

    let use_case = ListLevelRewardsUseCase::new(state.leveling.clone());
    let rewards = use_case.execute(guild_id).await?;

    Ok(Json(rewards.into_iter().map(Into::into).collect()))
}

/// Adds a reward role.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn create_reward(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<CreateLevelRewardRequest>,
) -> Result<(StatusCode, Json<LevelRewardResponse>), AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "leveling:write").await?;

    let use_case = CreateLevelRewardUseCase::new(state.leveling.clone());
    let reward = use_case
        .execute(CreateLevelRewardCommand {
            guild_id,
            level: request.level,
            role_id: parse_snowflake(&request.role_id, "role_id")?,
            keep_on_demote: request.keep_on_demote,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(reward.into())))
}

/// Removes a reward role.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, reward = %reward_id))]
pub async fn delete_reward(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, reward_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let reward_id = parse_uuid(&reward_id, "rewardId")?;
    authorize(&state, actor.id, guild_id, "leveling:write").await?;

    let use_case = DeleteLevelRewardUseCase::new(state.leveling.clone());
    use_case.execute(guild_id, reward_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
