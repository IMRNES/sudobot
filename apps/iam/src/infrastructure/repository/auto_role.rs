//! Auto roles and their pending queue, backed by SeaORM.

use chrono::{DateTime, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use uuid::Uuid;

use crate::domain::autorole::{AutoRole, AutoRoleRepository, AutoRoleTrigger, PendingGrant};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

use super::entities::{auto_role_pending, auto_roles};
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const RULE: &str = "auto role";
const PENDING: &str = "pending grant";
const RULE_CONFLICT: &str = "this guild already grants that role automatically";
const PENDING_CONFLICT: &str = "that grant is already queued";

/// Auto roles and their pending queue, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmAutoRoleRepository {
    db: DatabaseConnection,
}

impl SeaOrmAutoRoleRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl AutoRoleRepository for SeaOrmAutoRoleRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<AutoRole>, RepositoryError> {
        let model = auto_roles::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(RULE, RULE_CONFLICT, error))?;

        Ok(model.map(AutoRole::from))
    }

    async fn list_for_guild(&self, guild_id: u64) -> Result<Vec<AutoRole>, RepositoryError> {
        let models = auto_roles::Entity::find()
            .filter(auto_roles::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_asc(auto_roles::Column::RoleId)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(RULE, RULE_CONFLICT, error))?;

        Ok(models.into_iter().map(AutoRole::from).collect())
    }

    async fn list(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<AutoRole>, RepositoryError> {
        let query = auto_roles::Entity::find()
            .filter(auto_roles::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_asc(auto_roles::Column::RoleId);

        paginate_models(query, &self.db, params, RULE, RULE_CONFLICT, AutoRole::from).await
    }

    async fn create(&self, rule: AutoRole) -> Result<AutoRole, RepositoryError> {
        let now = from_utc(Utc::now());

        let model = auto_roles::ActiveModel {
            id: sea_orm::ActiveValue::Set(rule.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(rule.guild_id)),
            role_id: sea_orm::ActiveValue::Set(snowflake(rule.role_id)),
            trigger: sea_orm::ActiveValue::Set(rule.trigger.as_str().to_string()),
            delay_seconds: sea_orm::ActiveValue::Set(rule.delay_seconds),
            applies_to_bots: sea_orm::ActiveValue::Set(rule.applies_to_bots),
            is_active: sea_orm::ActiveValue::Set(rule.is_active),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        // The (guild, role) unique index is the rule the domain states: two rules
        // granting the same role would race each other on every join, so the second
        // one is refused rather than silently accepted.
        let created = auto_roles::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(RULE, RULE_CONFLICT, error))?;

        Ok(created.into())
    }

    async fn update(
        &self,
        id: Uuid,
        trigger: Option<AutoRoleTrigger>,
        delay_seconds: Option<i32>,
        applies_to_bots: Option<bool>,
        is_active: Option<bool>,
    ) -> Result<AutoRole, RepositoryError> {
        let mut active = auto_roles::ActiveModel {
            id: sea_orm::ActiveValue::Set(id),
            ..Default::default()
        };

        if let Some(trigger) = trigger {
            active.trigger = sea_orm::ActiveValue::Set(trigger.as_str().to_string());
        }
        if let Some(delay_seconds) = delay_seconds {
            active.delay_seconds = sea_orm::ActiveValue::Set(delay_seconds);
        }
        if let Some(applies_to_bots) = applies_to_bots {
            active.applies_to_bots = sea_orm::ActiveValue::Set(applies_to_bots);
        }
        if let Some(is_active) = is_active {
            active.is_active = sea_orm::ActiveValue::Set(is_active);
        }
        active.updated_at = sea_orm::ActiveValue::Set(from_utc(Utc::now()));

        let updated = auto_roles::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| match error {
                sea_orm::DbErr::RecordNotUpdated => not_found(RULE),
                other => map_db_err(RULE, RULE_CONFLICT, other),
            })?;

        Ok(updated.into())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // Queued grants for this rule cascade with it, which is the right answer: a
        // rule that no longer exists must not keep granting its role on a sweep.
        let result = auto_roles::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(RULE, RULE_CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(RULE));
        }

        Ok(())
    }

    async fn enqueue_pending(
        &self,
        grant: PendingGrant,
    ) -> Result<Option<PendingGrant>, RepositoryError> {
        let model = auto_role_pending::ActiveModel {
            id: sea_orm::ActiveValue::Set(grant.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(grant.guild_id)),
            discord_user_id: sea_orm::ActiveValue::Set(snowflake(grant.discord_user_id)),
            auto_role_id: sea_orm::ActiveValue::Set(grant.auto_role_id),
            role_id: sea_orm::ActiveValue::Set(snowflake(grant.role_id)),
            due_at: sea_orm::ActiveValue::Set(from_utc(grant.due_at)),
            attempts: sea_orm::ActiveValue::Set(grant.attempts),
            last_error: sea_orm::ActiveValue::Set(grant.last_error.clone()),
            created_at: sea_orm::ActiveValue::Set(from_utc(grant.created_at)),
        };

        // `ON CONFLICT DO NOTHING` on the (rule, member) unique index: a rejoin must
        // not stack a second grant on top of one already waiting, and it must not be
        // an error either — the member ends up with the role exactly once.
        let result = auto_role_pending::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    auto_role_pending::Column::AutoRoleId,
                    auto_role_pending::Column::DiscordUserId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(PENDING, PENDING_CONFLICT, error))?;

        // `rows_affected == 0` is the conflict path; anything else inserted the row.
        if result == 0 {
            return Ok(None);
        }

        Ok(Some(grant))
    }

    async fn due_pending(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<PendingGrant>, RepositoryError> {
        let models = auto_role_pending::Entity::find()
            .filter(auto_role_pending::Column::DueAt.lte(from_utc(now)))
            // Oldest first, and capped: one sweep must not try to drain a backlog
            // that accumulated while the bot was down, or it would hammer Discord.
            .order_by_asc(auto_role_pending::Column::DueAt)
            .order_by_asc(auto_role_pending::Column::Id)
            .limit(u64::from(limit))
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(PENDING, PENDING_CONFLICT, error))?;

        Ok(models.into_iter().map(PendingGrant::from).collect())
    }

    async fn record_attempt(
        &self,
        id: Uuid,
        error: Option<&str>,
        next_due_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError> {
        // `attempts = attempts + 1` in SQL rather than from a number read here: two
        // sweeps racing on the same failing grant must count two attempts, not one.
        let mut update = auto_role_pending::Entity::update_many()
            .col_expr(auto_role_pending::Column::Attempts, increment_attempts());

        if let Some(error) = error {
            update = update.col_expr(
                auto_role_pending::Column::LastError,
                sea_orm::sea_query::Expr::value(error.to_string()),
            );
        }

        if let Some(next_due_at) = next_due_at {
            update = update.col_expr(
                auto_role_pending::Column::DueAt,
                sea_orm::sea_query::Expr::value(from_utc(next_due_at)),
            );
        }

        let result = update
            .filter(auto_role_pending::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(PENDING, PENDING_CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(PENDING));
        }

        Ok(())
    }

    async fn delete_pending(&self, id: Uuid) -> Result<(), RepositoryError> {
        let result = auto_role_pending::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(PENDING, PENDING_CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(PENDING));
        }

        Ok(())
    }

    async fn count_pending_for_user(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<u64, RepositoryError> {
        auto_role_pending::Entity::find()
            .filter(auto_role_pending::Column::GuildId.eq(snowflake(guild_id)))
            .filter(auto_role_pending::Column::DiscordUserId.eq(snowflake(discord_user_id)))
            .count(&self.db)
            .await
            .map_err(|error| map_db_err(PENDING, PENDING_CONFLICT, error))
    }
}

/// `attempts = attempts + 1`, built as SQL.
///
/// `ExprTrait` is imported inside this function rather than at module scope so its
/// `add` does not shadow anything else in the file.
fn increment_attempts() -> sea_orm::sea_query::SimpleExpr {
    use sea_orm::sea_query::{Expr, ExprTrait};

    Expr::col(auto_role_pending::Column::Attempts).add(1)
}

impl From<auto_roles::Model> for AutoRole {
    fn from(model: auto_roles::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            role_id: snowflake_from_db(model.role_id),
            // The column defaults to `join`, so an unrecognised value means a newer
            // version wrote it; treating it as an immediate grant is the same
            // behaviour the database's own default asks for.
            trigger: AutoRoleTrigger::parse(&model.trigger).unwrap_or(AutoRoleTrigger::Join),
            delay_seconds: model.delay_seconds,
            applies_to_bots: model.applies_to_bots,
            is_active: model.is_active,
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}

impl From<auto_role_pending::Model> for PendingGrant {
    fn from(model: auto_role_pending::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            discord_user_id: snowflake_from_db(model.discord_user_id),
            auto_role_id: model.auto_role_id,
            role_id: snowflake_from_db(model.role_id),
            due_at: to_utc(model.due_at),
            attempts: model.attempts,
            last_error: model.last_error,
            created_at: to_utc(model.created_at),
        }
    }
}
