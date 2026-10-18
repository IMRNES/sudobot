//! Guilds, backed by SeaORM.

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::guild::{Guild, GuildPatch, GuildRepository, NewGuild};
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder,
};

use super::entities::guilds;
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const ENTITY: &str = "guild";
const CONFLICT: &str = "a guild with that id already exists";
const NAME_TAKEN: &str = "a guild with that name already exists";

/// Guilds, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmGuildRepository {
    db: DatabaseConnection,
}

impl SeaOrmGuildRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Re-reads a row written by an upsert.
    ///
    /// `ON CONFLICT DO UPDATE` returns the row when it inserts and nothing when it
    /// updates, so the returned model is only trustworthy for the insert path; the
    /// read makes both paths agree.
    async fn read(&self, id: i64) -> Result<Guild, RepositoryError> {
        guilds::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?
            .map(Guild::from)
            .ok_or_else(|| not_found(ENTITY))
    }
}

impl GuildRepository for SeaOrmGuildRepository {
    async fn find_by_id(&self, id: u64) -> Result<Option<Guild>, RepositoryError> {
        let model = guilds::Entity::find_by_id(snowflake(id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(model.map(Guild::from))
    }

    async fn list_active(&self) -> Result<Vec<Guild>, RepositoryError> {
        let models = guilds::Entity::find()
            .filter(guilds::Column::BotPresent.eq(true))
            .order_by_asc(guilds::Column::Id)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(models.into_iter().map(Guild::from).collect())
    }

    async fn list(&self, params: &PaginationParams) -> Result<Page<Guild>, RepositoryError> {
        // Guild id is Discord's snowflake: unique, immutable and already indexed,
        // which is what a paging query needs to stay stable between calls.
        let query = guilds::Entity::find().order_by_asc(guilds::Column::Id);

        paginate_models(query, &self.db, params, ENTITY, CONFLICT, Guild::from).await
    }

    async fn upsert(&self, guild: NewGuild) -> Result<Guild, RepositoryError> {
        let id = snowflake(guild.id);
        let now = from_utc(Utc::now());

        // `ON CONFLICT ... DO UPDATE` rather than select-then-insert: a rejoin is
        // the normal path, and two joins racing (the bot's join handler and a login
        // sync) must produce one row, not a unique violation in one of them.
        let model = guilds::ActiveModel {
            id: Set(id),
            name: Set(guild.name),
            icon_url: Set(guild.icon_url),
            owner_discord_id: Set(guild.owner_discord_id.map(snowflake)),
            member_count: Set(guild.member_count),
            // A rejoin must flip this back: the dashboard hides guilds whose bot
            // is gone, and a stale `false` would hide every guild after an outage.
            bot_present: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
        };

        guilds::Entity::insert(model)
            .on_conflict(
                OnConflict::column(guilds::Column::Id)
                    .update_columns([
                        guilds::Column::Name,
                        guilds::Column::IconUrl,
                        guilds::Column::OwnerDiscordId,
                        guilds::Column::MemberCount,
                        guilds::Column::BotPresent,
                        guilds::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        self.read(id).await
    }

    async fn update(&self, id: u64, patch: GuildPatch) -> Result<Guild, RepositoryError> {
        if patch.is_empty() {
            return self.find_by_id(id).await?.ok_or_else(|| not_found(ENTITY));
        }

        let mut active = guilds::ActiveModel {
            id: Set(snowflake(id)),
            ..Default::default()
        };

        if let Some(name) = patch.name {
            active.name = Set(name);
        }
        if let Some(icon_url) = patch.icon_url {
            active.icon_url = Set(Some(icon_url));
        }
        if let Some(owner_discord_id) = patch.owner_discord_id {
            active.owner_discord_id = Set(Some(snowflake(owner_discord_id)));
        }
        if let Some(member_count) = patch.member_count {
            active.member_count = Set(member_count);
        }
        if let Some(bot_present) = patch.bot_present {
            active.bot_present = Set(bot_present);
        }
        active.updated_at = Set(from_utc(Utc::now()));

        let updated = guilds::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, NAME_TAKEN, error))?;

        Ok(updated.into())
    }

    async fn delete(&self, id: u64) -> Result<(), RepositoryError> {
        let result = guilds::Entity::delete_by_id(snowflake(id))
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(ENTITY));
        }

        Ok(())
    }

    async fn count(&self) -> Result<u64, RepositoryError> {
        guilds::Entity::find()
            .count(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))
    }
}

impl From<guilds::Model> for Guild {
    fn from(model: guilds::Model) -> Self {
        Self {
            id: snowflake_from_db(model.id),
            name: model.name,
            icon_url: model.icon_url,
            owner_discord_id: model.owner_discord_id.map(snowflake_from_db),
            member_count: model.member_count,
            bot_present: model.bot_present,
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}
