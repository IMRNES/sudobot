//! Wallets, the ledger and the shop, backed by SeaORM.

use chrono::Utc;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
};
use uuid::Uuid;

use crate::domain::economy::{
    Account, EconomyRepository, NewTransaction, ShopItem, Transaction, TransactionReason,
};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

use super::entities::{economy_accounts, economy_shop_items, economy_transactions};
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const ACCOUNT: &str = "economy account";
const LEDGER: &str = "transaction";
const ITEM: &str = "shop item";
const IDEMPOTENT: &str = "this transaction has already been applied";
const ITEM_TAKEN: &str = "a shop item with that slug already exists";

/// Wallets, the ledger and the shop, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmEconomyRepository {
    db: DatabaseConnection,
}

impl SeaOrmEconomyRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// The account row, or a freshly built one that has not been written yet.
    fn new_account(guild_id: u64, discord_user_id: u64) -> economy_accounts::ActiveModel {
        let now = from_utc(Utc::now());

        economy_accounts::ActiveModel {
            id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
            guild_id: sea_orm::ActiveValue::Set(snowflake(guild_id)),
            discord_user_id: sea_orm::ActiveValue::Set(snowflake(discord_user_id)),
            balance: sea_orm::ActiveValue::Set(Account::STARTING_BALANCE),
            total_earned: sea_orm::ActiveValue::Set(0),
            total_spent: sea_orm::ActiveValue::Set(0),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
            last_daily_at: sea_orm::ActiveValue::NotSet,
        }
    }
}

impl EconomyRepository for SeaOrmEconomyRepository {
    async fn find_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Option<Account>, RepositoryError> {
        let model = economy_accounts::Entity::find()
            .filter(economy_accounts::Column::GuildId.eq(snowflake(guild_id)))
            .filter(economy_accounts::Column::DiscordUserId.eq(snowflake(discord_user_id)))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?;

        Ok(model.map(Account::from))
    }

    async fn ensure_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Account, RepositoryError> {
        // `ON CONFLICT DO NOTHING` on the (guild, user) unique index, then read:
        // two concurrent claims on a first-ever daily would otherwise both see
        // "no account" and both insert.
        economy_accounts::Entity::insert(Self::new_account(guild_id, discord_user_id))
            .on_conflict(
                OnConflict::columns([
                    economy_accounts::Column::GuildId,
                    economy_accounts::Column::DiscordUserId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?;

        self.find_account(guild_id, discord_user_id)
            .await?
            .ok_or_else(|| not_found(ACCOUNT))
    }

    async fn apply_transaction(
        &self,
        transaction: NewTransaction,
    ) -> Result<(Account, Transaction), RepositoryError> {
        // Everything below happens inside one transaction, and the balance is read
        // *inside* it. Computing the new balance from a value read outside would let
        // two concurrent purchases both start from the same number and mint currency.
        let tx = self
            .db
            .begin()
            .await
            .map_err(|error| map_db_err(LEDGER, IDEMPOTENT, error))?;

        let account_id = {
            let existing = economy_accounts::Entity::find()
                .filter(economy_accounts::Column::GuildId.eq(snowflake(transaction.guild_id)))
                .filter(
                    economy_accounts::Column::DiscordUserId
                        .eq(snowflake(transaction.discord_user_id)),
                )
                .one(&tx)
                .await
                .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?;

            match existing {
                Some(model) => model.id,
                None => {
                    // `ensure_account` promises the row exists; a caller that skipped
                    // it still gets a ledger entry rather than a foreign-key failure,
                    // because the ledger is the record that matters.
                    economy_accounts::Entity::insert(Self::new_account(
                        transaction.guild_id,
                        transaction.discord_user_id,
                    ))
                    .on_conflict(
                        OnConflict::columns([
                            economy_accounts::Column::GuildId,
                            economy_accounts::Column::DiscordUserId,
                        ])
                        .do_nothing()
                        .to_owned(),
                    )
                    .exec_without_returning(&tx)
                    .await
                    .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?;

                    economy_accounts::Entity::find()
                        .filter(
                            economy_accounts::Column::GuildId.eq(snowflake(transaction.guild_id)),
                        )
                        .filter(
                            economy_accounts::Column::DiscordUserId
                                .eq(snowflake(transaction.discord_user_id)),
                        )
                        .one(&tx)
                        .await
                        .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?
                        .ok_or_else(|| not_found(ACCOUNT))?
                        .id
                }
            }
        };

        let current = economy_accounts::Entity::find_by_id(account_id)
            .one(&tx)
            .await
            .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?
            .ok_or_else(|| not_found(ACCOUNT))?;

        let balance_after = current.balance + transaction.amount;
        let now = from_utc(Utc::now());

        let earned = current.total_earned + i64::max(transaction.amount, 0);
        let spent = current.total_spent + i64::max(-transaction.amount, 0);
        let last_daily_at = match transaction.reason {
            // The daily claim's timestamp belongs on the account, not in a separate
            // read: it is what makes the next claim's cooldown check correct under
            // concurrency.
            TransactionReason::Daily => Some(now),
            _ => current.last_daily_at,
        };

        economy_accounts::Entity::update_many()
            .col_expr(
                economy_accounts::Column::Balance,
                Expr::value(balance_after),
            )
            .col_expr(economy_accounts::Column::TotalEarned, Expr::value(earned))
            .col_expr(economy_accounts::Column::TotalSpent, Expr::value(spent))
            .col_expr(economy_accounts::Column::UpdatedAt, Expr::value(now))
            .col_expr(
                economy_accounts::Column::LastDailyAt,
                Expr::value(last_daily_at),
            )
            .filter(economy_accounts::Column::Id.eq(account_id))
            .exec(&tx)
            .await
            .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?;

        let entry = economy_transactions::ActiveModel {
            id: sea_orm::ActiveValue::Set(transaction.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(transaction.guild_id)),
            discord_user_id: sea_orm::ActiveValue::Set(snowflake(transaction.discord_user_id)),
            amount: sea_orm::ActiveValue::Set(transaction.amount),
            balance_after: sea_orm::ActiveValue::Set(balance_after),
            reason: sea_orm::ActiveValue::Set(transaction.reason.as_str().to_string()),
            actor_discord_id: sea_orm::ActiveValue::Set(
                transaction.actor_discord_id.map(snowflake),
            ),
            note: sea_orm::ActiveValue::Set(transaction.note),
            idempotency_key: sea_orm::ActiveValue::Set(transaction.idempotency_key),
            created_at: sea_orm::ActiveValue::Set(now),
        };

        // The unique index on `idempotency_key` is what stops a replayed interaction
        // from paying twice; `map_db_err` turns that violation into a `Conflict` the
        // caller can report as "already applied" rather than a 500.
        economy_transactions::Entity::insert(entry)
            .exec_with_returning(&tx)
            .await
            .map_err(|error| map_db_err(LEDGER, IDEMPOTENT, error))?;

        let account = economy_accounts::Entity::find_by_id(account_id)
            .one(&tx)
            .await
            .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?
            .ok_or_else(|| not_found(ACCOUNT))?;

        let entry = economy_transactions::Entity::find_by_id(transaction.id)
            .one(&tx)
            .await
            .map_err(|error| map_db_err(LEDGER, IDEMPOTENT, error))?
            .ok_or_else(|| not_found(LEDGER))?;

        tx.commit()
            .await
            .map_err(|error| map_db_err(LEDGER, IDEMPOTENT, error))?;

        Ok((Account::from(account), Transaction::from(entry)))
    }

    async fn list_transactions(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<Transaction>, RepositoryError> {
        // Newest first — a ledger is read from the most recent entry — and the id
        // breaks ties so a page boundary inside one timestamp stays stable.
        let query = economy_transactions::Entity::find()
            .filter(economy_transactions::Column::GuildId.eq(snowflake(guild_id)))
            .filter(economy_transactions::Column::DiscordUserId.eq(snowflake(discord_user_id)))
            .order_by_desc(economy_transactions::Column::CreatedAt)
            .order_by_desc(economy_transactions::Column::Id);

        paginate_models(
            query,
            &self.db,
            params,
            LEDGER,
            IDEMPOTENT,
            Transaction::from,
        )
        .await
    }

    async fn leaderboard(
        &self,
        guild_id: u64,
        limit: u32,
    ) -> Result<Vec<Account>, RepositoryError> {
        let models = economy_accounts::Entity::find()
            .filter(economy_accounts::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_desc(economy_accounts::Column::Balance)
            // Ties resolve by member id so the same call twice returns the same order.
            .order_by_asc(economy_accounts::Column::DiscordUserId)
            .limit(u64::from(limit))
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ACCOUNT, IDEMPOTENT, error))?;

        Ok(models.into_iter().map(Account::from).collect())
    }

    async fn find_item_by_slug(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<Option<ShopItem>, RepositoryError> {
        let model = economy_shop_items::Entity::find()
            .filter(economy_shop_items::Column::GuildId.eq(snowflake(guild_id)))
            .filter(economy_shop_items::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ITEM, ITEM_TAKEN, error))?;

        Ok(model.map(ShopItem::from))
    }

    async fn find_item_by_id(&self, id: Uuid) -> Result<Option<ShopItem>, RepositoryError> {
        let model = economy_shop_items::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ITEM, ITEM_TAKEN, error))?;

        Ok(model.map(ShopItem::from))
    }

    async fn list_items(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<ShopItem>, RepositoryError> {
        let query = economy_shop_items::Entity::find()
            .filter(economy_shop_items::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_asc(economy_shop_items::Column::Slug);

        paginate_models(query, &self.db, params, ITEM, ITEM_TAKEN, ShopItem::from).await
    }

    async fn create_item(&self, item: ShopItem) -> Result<ShopItem, RepositoryError> {
        let now = from_utc(Utc::now());

        let model = economy_shop_items::ActiveModel {
            id: sea_orm::ActiveValue::Set(item.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(item.guild_id)),
            slug: sea_orm::ActiveValue::Set(item.slug),
            name: sea_orm::ActiveValue::Set(item.name),
            description: sea_orm::ActiveValue::Set(item.description),
            price: sea_orm::ActiveValue::Set(item.price),
            role_id: sea_orm::ActiveValue::Set(item.role_id.map(snowflake)),
            stock: sea_orm::ActiveValue::Set(item.stock),
            is_active: sea_orm::ActiveValue::Set(item.is_active),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        let created = economy_shop_items::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ITEM, ITEM_TAKEN, error))?;

        Ok(created.into())
    }

    async fn update_item(
        &self,
        id: Uuid,
        name: Option<String>,
        description: Option<String>,
        price: Option<i64>,
        is_active: Option<bool>,
        stock: Option<Option<i32>>,
    ) -> Result<ShopItem, RepositoryError> {
        let mut active = economy_shop_items::ActiveModel {
            id: sea_orm::ActiveValue::Set(id),
            ..Default::default()
        };

        if let Some(name) = name {
            active.name = sea_orm::ActiveValue::Set(name);
        }
        if let Some(description) = description {
            active.description = sea_orm::ActiveValue::Set(Some(description));
        }
        if let Some(price) = price {
            active.price = sea_orm::ActiveValue::Set(price);
        }
        if let Some(is_active) = is_active {
            active.is_active = sea_orm::ActiveValue::Set(is_active);
        }
        if let Some(stock) = stock {
            // `Some(None)` clears the stock to "unlimited", which is distinct from
            // "leave it alone" — hence the nested `Option` in the trait.
            active.stock = sea_orm::ActiveValue::Set(stock);
        }
        active.updated_at = sea_orm::ActiveValue::Set(from_utc(Utc::now()));

        let updated = economy_shop_items::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| match error {
                sea_orm::DbErr::RecordNotUpdated => not_found(ITEM),
                other => map_db_err(ITEM, ITEM_TAKEN, other),
            })?;

        Ok(updated.into())
    }

    async fn decrement_stock_if_available(&self, item_id: Uuid) -> Result<u64, RepositoryError> {
        // The arithmetic and null tests below come from `ExprTrait`; it is imported
        // here rather than at module scope because its `max`/`min` would otherwise
        // shadow the integer ones this file uses on plain `i64`s.
        use sea_orm::sea_query::ExprTrait;

        // One guarded statement: the `WHERE stock > 0` and the decrement happen in the
        // same row lock, so two buyers cannot both see the last unit. Reading the
        // stock first and then writing would be the race this exists to avoid.
        //
        // `NULL` means unlimited and always "succeeds"; the caller only calls this for
        // an item whose stock it has already seen as `Some`.
        let result = economy_shop_items::Entity::update_many()
            .col_expr(
                economy_shop_items::Column::Stock,
                Expr::col(economy_shop_items::Column::Stock).sub(1),
            )
            .col_expr(
                economy_shop_items::Column::UpdatedAt,
                Expr::value(from_utc(Utc::now())),
            )
            .filter(economy_shop_items::Column::Id.eq(item_id))
            .filter(
                Expr::col(economy_shop_items::Column::Stock)
                    .is_null()
                    .or(Expr::col(economy_shop_items::Column::Stock).gt(0)),
            )
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ITEM, ITEM_TAKEN, error))?;

        Ok(result.rows_affected)
    }
}

impl From<economy_accounts::Model> for Account {
    fn from(model: economy_accounts::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            discord_user_id: snowflake_from_db(model.discord_user_id),
            balance: model.balance,
            total_earned: model.total_earned,
            total_spent: model.total_spent,
            last_daily_at: model.last_daily_at.map(to_utc),
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}

impl From<economy_transactions::Model> for Transaction {
    fn from(model: economy_transactions::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            discord_user_id: snowflake_from_db(model.discord_user_id),
            amount: model.amount,
            balance_after: model.balance_after,
            reason: model.reason,
            actor_discord_id: model.actor_discord_id.map(snowflake_from_db),
            note: model.note,
            created_at: to_utc(model.created_at),
        }
    }
}

impl From<economy_shop_items::Model> for ShopItem {
    fn from(model: economy_shop_items::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            slug: model.slug,
            name: model.name,
            description: model.description,
            price: model.price,
            role_id: model.role_id.map(snowflake_from_db),
            stock: model.stock,
            is_active: model.is_active,
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}
