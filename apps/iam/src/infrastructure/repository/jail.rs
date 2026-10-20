//! Jail configuration and sentences, backed by SeaORM.

use crate::domain::errors::RepositoryError;
use crate::domain::jail::{Imprisonment, JailConfig, JailRepository};
use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};

use super::entities::{jail_configs, jail_sentences};
use super::error::map_db_err;
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};

const CONFIG: &str = "jail config";
const SENTENCE: &str = "jail sentence";
const CONFIG_CONFLICT: &str = "that guild already has jail settings";
const SENTENCE_CONFLICT: &str = "that member already has a sentence in this guild";

/// Jail configuration and sentences, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmJailRepository {
    db: DatabaseConnection,
}

impl SeaOrmJailRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl From<jail_configs::Model> for JailConfig {
    fn from(model: jail_configs::Model) -> Self {
        Self {
            guild_id: snowflake_from_db(model.guild_id),
            jail_role_id: model.jail_role_id.map(snowflake_from_db),
            jail_channel_id: model.jail_channel_id.map(snowflake_from_db),
            default_sentence_seconds: model.default_sentence_seconds,
            bail_amount: model.bail_amount,
            updated_at: to_utc(model.updated_at),
        }
    }
}

impl From<jail_sentences::Model> for Imprisonment {
    fn from(model: jail_sentences::Model) -> Self {
        Self {
            guild_id: snowflake_from_db(model.guild_id),
            discord_user_id: snowflake_from_db(model.discord_user_id),
            jailed_by: snowflake_from_db(model.jailed_by),
            reason: model.reason,
            expires_at: model.expires_at.map(to_utc),
            created_at: to_utc(model.created_at),
        }
    }
}

impl JailRepository for SeaOrmJailRepository {
    async fn find_config(&self, guild_id: u64) -> Result<JailConfig, RepositoryError> {
        let model = jail_configs::Entity::find_by_id(snowflake(guild_id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(CONFIG, CONFIG_CONFLICT, error))?;

        // No row means the guild has never configured a jail, which is the
        // defaults — not a missing record the caller has to handle.
        Ok(model
            .map(JailConfig::from)
            .unwrap_or_else(|| JailConfig::default_for(guild_id)))
    }

    async fn upsert_config(&self, config: JailConfig) -> Result<JailConfig, RepositoryError> {
        let model = jail_configs::ActiveModel {
            guild_id: sea_orm::ActiveValue::Set(snowflake(config.guild_id)),
            jail_role_id: sea_orm::ActiveValue::Set(config.jail_role_id.map(snowflake)),
            jail_channel_id: sea_orm::ActiveValue::Set(config.jail_channel_id.map(snowflake)),
            default_sentence_seconds: sea_orm::ActiveValue::Set(config.default_sentence_seconds),
            bail_amount: sea_orm::ActiveValue::Set(config.bail_amount),
            updated_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
        };

        // `update_columns` decides what an EXISTING row gets; the `ActiveModel` only
        // decides what an INSERT carries. Every mutable column must be listed here
        // or a save to an existing config answers 200 and stores nothing.
        jail_configs::Entity::insert(model)
            .on_conflict(
                OnConflict::column(jail_configs::Column::GuildId)
                    .update_columns([
                        jail_configs::Column::JailRoleId,
                        jail_configs::Column::JailChannelId,
                        jail_configs::Column::DefaultSentenceSeconds,
                        jail_configs::Column::BailAmount,
                        jail_configs::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_with_returning(&self.db)
            .await
            .map(JailConfig::from)
            .map_err(|error| map_db_err(CONFIG, CONFIG_CONFLICT, error))
    }

    async fn find(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Option<Imprisonment>, RepositoryError> {
        let model =
            jail_sentences::Entity::find_by_id((snowflake(guild_id), snowflake(discord_user_id)))
                .one(&self.db)
                .await
                .map_err(|error| map_db_err(SENTENCE, SENTENCE_CONFLICT, error))?;

        Ok(model.map(Imprisonment::from))
    }

    async fn upsert(&self, imprisonment: Imprisonment) -> Result<Imprisonment, RepositoryError> {
        let model = jail_sentences::ActiveModel {
            guild_id: sea_orm::ActiveValue::Set(snowflake(imprisonment.guild_id)),
            discord_user_id: sea_orm::ActiveValue::Set(snowflake(imprisonment.discord_user_id)),
            jailed_by: sea_orm::ActiveValue::Set(snowflake(imprisonment.jailed_by)),
            reason: sea_orm::ActiveValue::Set(imprisonment.reason.clone()),
            expires_at: sea_orm::ActiveValue::Set(imprisonment.expires_at.map(from_utc)),
            created_at: sea_orm::ActiveValue::Set(from_utc(imprisonment.created_at)),
        };

        jail_sentences::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    jail_sentences::Column::GuildId,
                    jail_sentences::Column::DiscordUserId,
                ])
                .update_columns([
                    jail_sentences::Column::JailedBy,
                    jail_sentences::Column::Reason,
                    jail_sentences::Column::ExpiresAt,
                    jail_sentences::Column::CreatedAt,
                ])
                .to_owned(),
            )
            .exec_with_returning(&self.db)
            .await
            .map(Imprisonment::from)
            .map_err(|error| map_db_err(SENTENCE, SENTENCE_CONFLICT, error))
    }

    async fn delete(&self, guild_id: u64, discord_user_id: u64) -> Result<(), RepositoryError> {
        // Deleting a sentence that is not there is a success, not a 404. Two ordinary
        // situations produce it: a moderator releasing somebody the sweeper has
        // already freed, and a sweep completing a release whose row a dashboard PATCH
        // removed first. Turning either into an error would make the caller log a
        // failure for something that already has the desired end state.
        jail_sentences::Entity::delete_by_id((snowflake(guild_id), snowflake(discord_user_id)))
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(SENTENCE, SENTENCE_CONFLICT, error))?;

        Ok(())
    }

    async fn expired(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> Result<Vec<Imprisonment>, RepositoryError> {
        // `is_not_null` is redundant given the `lte` comparison — SQL already
        // excludes NULL from an inequality — but it is spelled out because
        // "an indefinite sentence must never be picked up here" is the rule, and a
        // future edit to a `COALESCE` or a `>=` would quietly break it.
        let rows = jail_sentences::Entity::find()
            .filter(jail_sentences::Column::ExpiresAt.is_not_null())
            .filter(jail_sentences::Column::ExpiresAt.lte(from_utc(now)))
            .order_by_asc(jail_sentences::Column::ExpiresAt)
            .limit(u64::from(limit))
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(SENTENCE, SENTENCE_CONFLICT, error))?;

        Ok(rows.into_iter().map(Imprisonment::from).collect())
    }

    async fn list_for_guild(&self, guild_id: u64) -> Result<Vec<Imprisonment>, RepositoryError> {
        let rows = jail_sentences::Entity::find()
            .filter(jail_sentences::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_desc(jail_sentences::Column::CreatedAt)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(SENTENCE, SENTENCE_CONFLICT, error))?;

        Ok(rows.into_iter().map(Imprisonment::from).collect())
    }
}
