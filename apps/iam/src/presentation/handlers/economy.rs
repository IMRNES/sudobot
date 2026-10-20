use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use paginator_axum::PaginationQuery;
use serde::Deserialize;
use tracing::instrument;

use crate::application::economy::{
    AdjustBalanceCommand, AdjustBalanceUseCase, CreateShopItemCommand, CreateShopItemUseCase,
    DeleteShopItemUseCase, GetAccountUseCase, LeaderboardEntry, ListTransactionsUseCase,
    ShopItemView, ShopListingUseCase, UpdateShopItemCommand, UpdateShopItemUseCase,
};
// `find_by_id` is a trait method, not an inherent one — without this the call site
// fails with "no method named find_by_id", which points at the wrong place.
use crate::domain::user::UserRepository;
use crate::presentation::dto::PageResponse;
use crate::presentation::dto::economy::{
    AccountResponse, AdjustBalanceRequest, CreateShopItemRequest, LeaderboardEntryResponse,
    ShopItemResponse, TransactionResponse, UpdateShopItemRequest,
};
use crate::presentation::error::AppError;
use crate::presentation::extract::ValidatedJson;
use crate::presentation::handlers::{authorize, parse_uuid, to_pagination};
use crate::presentation::middleware::AuthenticatedUser;
use crate::presentation::state::AppState;
use crate::presentation::validation::{guild_id_from_path, parse_snowflake, validate_limit};

/// Query for the leaderboard, which does not paginate.
#[derive(Debug, Deserialize)]
pub struct LimitQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    10
}

/// One member's wallet.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, subject = %discord_id))]
pub async fn get_account(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, discord_id)): Path<(String, String)>,
) -> Result<Json<AccountResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let discord_id = parse_snowflake(&discord_id, "discordId")?;
    authorize(&state, actor.id, guild_id, "economy:read").await?;

    let use_case = GetAccountUseCase::new(state.economy.clone());
    let account = use_case.execute(guild_id, discord_id).await?;

    Ok(Json(account.into()))
}

/// A member's ledger history.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, subject = %discord_id))]
pub async fn list_transactions(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, discord_id)): Path<(String, String)>,
    PaginationQuery(query): PaginationQuery,
) -> Result<Json<PageResponse<TransactionResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let discord_id = parse_snowflake(&discord_id, "discordId")?;
    authorize(&state, actor.id, guild_id, "economy:read").await?;

    let use_case = ListTransactionsUseCase::new(state.economy.clone());
    let page = use_case
        .execute(guild_id, discord_id, &to_pagination(&query))
        .await?;

    Ok(Json(page.map(TransactionResponse::from).into()))
}

/// Grants or deducts currency.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, subject = %discord_id))]
pub async fn adjust_balance(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, discord_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<AdjustBalanceRequest>,
) -> Result<Json<AccountResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let discord_id = parse_snowflake(&discord_id, "discordId")?;
    authorize(&state, actor.id, guild_id, "economy:write").await?;

    let use_case = AdjustBalanceUseCase::new(state.economy.clone());

    // The ledger records the actor as a Discord id, and the session only knows the
    // user row's UUID, so the id is resolved here. A missing row leaves it null
    // rather than failing the adjustment: the write is what the caller asked for.
    let actor_discord_id = state
        .users
        .find_by_id(actor.id)
        .await?
        .map(|user| user.discord_id);

    let account = use_case
        .execute(AdjustBalanceCommand {
            guild_id,
            discord_user_id: discord_id,
            amount: request.amount,
            actor_user_id: Some(actor.id),
            actor_discord_id,
            reason: request.reason,
        })
        .await?;

    Ok(Json(account.into()))
}

/// The richest members of the guild.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn leaderboard(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    Query(query): Query<LimitQuery>,
) -> Result<Json<Vec<LeaderboardEntryResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let limit = validate_limit(query.limit, 100, "limit")?;
    authorize(&state, actor.id, guild_id, "economy:read").await?;

    let use_case =
        ShopListingUseCase::with_member_names(state.economy.clone(), state.leveling.clone());
    let entries: Vec<LeaderboardEntry> = use_case.leaderboard_with_names(guild_id, limit).await?;

    Ok(Json(LeaderboardEntryResponse::list(&entries)))
}

/// The guild's shop.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn list_items(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
) -> Result<Json<Vec<ShopItemResponse>>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "economy:read").await?;

    let use_case = ShopListingUseCase::new(state.economy.clone());
    let items: Vec<ShopItemView> = use_case.execute(guild_id).await?;

    Ok(Json(items.into_iter().map(Into::into).collect()))
}

/// Adds an item to the shop.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id))]
pub async fn create_item(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path(guild_id): Path<String>,
    ValidatedJson(request): ValidatedJson<CreateShopItemRequest>,
) -> Result<(StatusCode, Json<ShopItemResponse>), AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    authorize(&state, actor.id, guild_id, "economy:write").await?;

    let role_id = match request.role_id.as_deref() {
        Some(raw) => Some(parse_snowflake(raw, "role_id")?),
        None => None,
    };

    let use_case = CreateShopItemUseCase::new(state.economy.clone());
    let item = use_case
        .execute(CreateShopItemCommand {
            guild_id,
            slug: request.slug,
            name: request.name,
            description: request.description,
            price: request.price,
            role_id,
            stock: request.stock,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(ShopItemView::from(item).into())))
}

/// Changes a shop item.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, item = %item_id))]
pub async fn update_item(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, item_id)): Path<(String, String)>,
    ValidatedJson(request): ValidatedJson<UpdateShopItemRequest>,
) -> Result<Json<ShopItemResponse>, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let item_id = parse_uuid(&item_id, "itemId")?;
    authorize(&state, actor.id, guild_id, "economy:write").await?;

    let use_case = UpdateShopItemUseCase::new(state.economy.clone());
    let item = use_case
        .execute(UpdateShopItemCommand {
            guild_id,
            id: item_id,
            name: request.name,
            description: request.description,
            price: request.price,
            is_active: request.is_active,
            stock: request.stock,
        })
        .await?;

    Ok(Json(ShopItemView::from(item).into()))
}

/// Removes a shop item.
#[instrument(skip_all, fields(actor = %actor.id, guild = %guild_id, item = %item_id))]
pub async fn delete_item(
    State(state): State<AppState>,
    actor: AuthenticatedUser,
    Path((guild_id, item_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let guild_id = guild_id_from_path(&guild_id)?;
    let item_id = parse_uuid(&item_id, "itemId")?;
    authorize(&state, actor.id, guild_id, "economy:write").await?;

    let use_case = DeleteShopItemUseCase::new(state.economy.clone());
    use_case.execute(guild_id, item_id).await?;

    Ok(StatusCode::NO_CONTENT)
}
