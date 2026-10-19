//! XP configuration, profiles and reward roles, backed by SeaORM.

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    TransactionTrait,
};
use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::leveling::{
    LevelCurve, LevelProfile, LevelReward, LevelingConfig, LevelingRepository,
};

use super::entities::{
    guild_member_profiles, leveling_configs, leveling_profiles, leveling_rewards,
};
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const CONFIG: &str = "leveling config";
const PROFILE: &str = "leveling profile";
const REWARD: &str = "level reward";
const PROFILE_CONFLICT: &str = "that member already has a leveling profile in this guild";
const MEMBER: &str = "guild member profile";
const MEMBER_CONFLICT: &str = "that member already has a profile in this guild";
const REWARD_CONFLICT: &str = "a reward already exists for that level";

/// XP configuration, profiles and reward roles, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmLevelingRepository {
    db: DatabaseConnection,
}

impl SeaOrmLevelingRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// The profile row, inserted on first sight.
    ///
    /// `ON CONFLICT DO NOTHING` on the (guild, member) unique index: two messages
    /// arriving at once for a member who has never spoken must produce one row.
    async fn ensure_profile_row(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<leveling_profiles::Model, RepositoryError> {
        let now = from_utc(Utc::now());

        leveling_profiles::Entity::insert(leveling_profiles::ActiveModel {
            id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
            guild_id: sea_orm::ActiveValue::Set(snowflake(guild_id)),
            discord_user_id: sea_orm::ActiveValue::Set(snowflake(discord_user_id)),
            xp: sea_orm::ActiveValue::Set(0),
            level: sea_orm::ActiveValue::Set(0),
            total_xp: sea_orm::ActiveValue::Set(0),
            message_count: sea_orm::ActiveValue::Set(0),
            voice_seconds: sea_orm::ActiveValue::Set(0),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
            last_xp_at: sea_orm::ActiveValue::NotSet,
        })
        .on_conflict(
            OnConflict::columns([
                leveling_profiles::Column::GuildId,
                leveling_profiles::Column::DiscordUserId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec_without_returning(&self.db)
        .await
        .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        leveling_profiles::Entity::find()
            .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
            .filter(leveling_profiles::Column::DiscordUserId.eq(snowflake(discord_user_id)))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?
            .ok_or_else(|| not_found(PROFILE))
    }

    /// The curve to keep `total_xp` consistent with.
    ///
    /// `total_xp` means "XP ever earned" and does not fall when a penalty is
    /// applied, so a gain is the difference between the caller's new position and
    /// the position the row held before — which the repository can only compute if
    /// it knows the curve.
    async fn curve_for(&self, guild_id: u64) -> Result<LevelCurve, RepositoryError> {
        let model = leveling_configs::Entity::find_by_id(snowflake(guild_id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(CONFIG, PROFILE_CONFLICT, error))?;

        Ok(model
            .and_then(|model| LevelCurve::parse(&model.curve))
            .unwrap_or(LevelCurve::Mrn))
    }
}

impl LevelingRepository for SeaOrmLevelingRepository {
    async fn find_config(&self, guild_id: u64) -> Result<LevelingConfig, RepositoryError> {
        let model = leveling_configs::Entity::find_by_id(snowflake(guild_id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(CONFIG, PROFILE_CONFLICT, error))?;

        // A guild that has never opened the leveling page has no row, and that is
        // not an error: the domain's defaults are what it runs on.
        Ok(model
            .map(LevelingConfig::from)
            .unwrap_or_else(|| LevelingConfig::default_for(guild_id)))
    }

    async fn upsert_config(
        &self,
        config: LevelingConfig,
    ) -> Result<LevelingConfig, RepositoryError> {
        let now = from_utc(Utc::now());
        let guild_id = snowflake(config.guild_id);

        let model = leveling_configs::ActiveModel {
            guild_id: sea_orm::ActiveValue::Set(guild_id),
            enabled: sea_orm::ActiveValue::Set(config.enabled),
            xp_min: sea_orm::ActiveValue::Set(config.xp_min),
            xp_max: sea_orm::ActiveValue::Set(config.xp_max),
            cooldown_secs: sea_orm::ActiveValue::Set(config.cooldown_secs),
            voice_xp_multiplier: sea_orm::ActiveValue::Set(config.voice_xp_multiplier),
            curve: sea_orm::ActiveValue::Set(config.curve.as_str().to_string()),
            announce_channel_id: sea_orm::ActiveValue::Set(
                config.announce_channel_id.map(snowflake),
            ),
            stack_roles: sea_orm::ActiveValue::Set(config.stack_roles),
            ignored_channel_ids: sea_orm::ActiveValue::Set(Some(
                config
                    .ignored_channel_ids
                    .iter()
                    .copied()
                    .map(snowflake)
                    .collect(),
            )),
            ignored_role_ids: sea_orm::ActiveValue::Set(Some(
                config
                    .ignored_role_ids
                    .iter()
                    .copied()
                    .map(snowflake)
                    .collect(),
            )),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        // The guild id is the primary key, so the upsert target is unambiguous and
        // saving the leveling page twice cannot produce two configurations.
        leveling_configs::Entity::insert(model)
            .on_conflict(
                OnConflict::column(leveling_configs::Column::GuildId)
                    .update_columns([
                        leveling_configs::Column::Enabled,
                        leveling_configs::Column::XpMin,
                        leveling_configs::Column::XpMax,
                        leveling_configs::Column::CooldownSecs,
                        leveling_configs::Column::VoiceXpMultiplier,
                        leveling_configs::Column::Curve,
                        leveling_configs::Column::AnnounceChannelId,
                        leveling_configs::Column::StackRoles,
                        leveling_configs::Column::IgnoredChannelIds,
                        leveling_configs::Column::IgnoredRoleIds,
                        leveling_configs::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(CONFIG, PROFILE_CONFLICT, error))?;

        self.find_config(config.guild_id).await
    }

    async fn find_profile(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Option<LevelProfile>, RepositoryError> {
        let model = leveling_profiles::Entity::find()
            .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
            .filter(leveling_profiles::Column::DiscordUserId.eq(snowflake(discord_user_id)))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        Ok(model.map(LevelProfile::from))
    }

    async fn ensure_profile(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<LevelProfile, RepositoryError> {
        Ok(self
            .ensure_profile_row(guild_id, discord_user_id)
            .await?
            .into())
    }

    async fn add_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        xp: i64,
        level: i32,
        counted_message: bool,
    ) -> Result<LevelProfile, RepositoryError> {
        let curve = self.curve_for(guild_id).await?;
        let now = from_utc(Utc::now());

        // One transaction: the row is created if it is missing, the new position is
        // read back, and `total_xp` is advanced by the difference between the two
        // positions — all against a value read inside the same transaction.
        let tx = self
            .db
            .begin()
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        let profile_id = {
            leveling_profiles::Entity::insert(leveling_profiles::ActiveModel {
                id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
                guild_id: sea_orm::ActiveValue::Set(snowflake(guild_id)),
                discord_user_id: sea_orm::ActiveValue::Set(snowflake(discord_user_id)),
                xp: sea_orm::ActiveValue::Set(0),
                level: sea_orm::ActiveValue::Set(0),
                total_xp: sea_orm::ActiveValue::Set(0),
                message_count: sea_orm::ActiveValue::Set(0),
                voice_seconds: sea_orm::ActiveValue::Set(0),
                created_at: sea_orm::ActiveValue::Set(now),
                updated_at: sea_orm::ActiveValue::Set(now),
                last_xp_at: sea_orm::ActiveValue::NotSet,
            })
            .on_conflict(
                OnConflict::columns([
                    leveling_profiles::Column::GuildId,
                    leveling_profiles::Column::DiscordUserId,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec_without_returning(&tx)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

            leveling_profiles::Entity::find()
                .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
                .filter(leveling_profiles::Column::DiscordUserId.eq(snowflake(discord_user_id)))
                .one(&tx)
                .await
                .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?
                .ok_or_else(|| not_found(PROFILE))?
                .id
        };

        let current = leveling_profiles::Entity::find_by_id(profile_id)
            .one(&tx)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?
            .ok_or_else(|| not_found(PROFILE))?;

        let before = curve.total_xp_for_level(current.level) + current.xp;
        let after = curve.total_xp_for_level(level) + xp;
        // A negative delta is a penalty: `total_xp` is "ever earned" and must not
        // fall, so it only ever moves up.
        let total_xp = current.total_xp + i64::max(after - before, 0);
        let message_count = current.message_count + i64::from(counted_message);

        let updated = leveling_profiles::Entity::update_many()
            .set(leveling_profiles::ActiveModel {
                xp: sea_orm::ActiveValue::Set(xp),
                level: sea_orm::ActiveValue::Set(level),
                total_xp: sea_orm::ActiveValue::Set(total_xp),
                message_count: sea_orm::ActiveValue::Set(message_count),
                last_xp_at: sea_orm::ActiveValue::Set(Some(now)),
                updated_at: sea_orm::ActiveValue::Set(now),
                ..Default::default()
            })
            .filter(leveling_profiles::Column::Id.eq(profile_id))
            .exec_with_returning(&tx)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        let profile = updated
            .into_iter()
            .next()
            .ok_or_else(|| not_found(PROFILE))?;

        tx.commit()
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        Ok(profile.into())
    }

    async fn add_voice_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        seconds: i64,
        xp: i64,
        level: i32,
    ) -> Result<LevelProfile, RepositoryError> {
        let curve = self.curve_for(guild_id).await?;
        let current = self.ensure_profile_row(guild_id, discord_user_id).await?;

        let before = curve.total_xp_for_level(current.level) + current.xp;
        let after = curve.total_xp_for_level(level) + xp;
        let total_xp = current.total_xp + i64::max(after - before, 0);
        let now = from_utc(Utc::now());

        // Voice time is not a message, so `message_count` is untouched; the column
        // it does move is `voice_seconds`, incremented in the database rather than
        // from a number read here.

        let updated = leveling_profiles::Entity::update_many()
            .col_expr(
                leveling_profiles::Column::VoiceSeconds,
                add_to_voice_seconds(seconds),
            )
            .set(leveling_profiles::ActiveModel {
                xp: sea_orm::ActiveValue::Set(xp),
                level: sea_orm::ActiveValue::Set(level),
                total_xp: sea_orm::ActiveValue::Set(total_xp),
                updated_at: sea_orm::ActiveValue::Set(now),
                ..Default::default()
            })
            .filter(leveling_profiles::Column::Id.eq(current.id))
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        updated
            .into_iter()
            .next()
            .map(LevelProfile::from)
            .ok_or_else(|| not_found(PROFILE))
    }

    async fn set_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        xp: i64,
        level: i32,
    ) -> Result<LevelProfile, RepositoryError> {
        // An administrative set is an absolute position: it creates the row when the
        // member has none, and it leaves `total_xp`, `message_count` and
        // `voice_seconds` alone — those describe what happened, not where the member
        // stands now.
        let current = self.ensure_profile_row(guild_id, discord_user_id).await?;
        let now = from_utc(Utc::now());

        let updated = leveling_profiles::Entity::update_many()
            .set(leveling_profiles::ActiveModel {
                xp: sea_orm::ActiveValue::Set(xp),
                level: sea_orm::ActiveValue::Set(level),
                updated_at: sea_orm::ActiveValue::Set(now),
                ..Default::default()
            })
            .filter(leveling_profiles::Column::Id.eq(current.id))
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        updated
            .into_iter()
            .next()
            .map(LevelProfile::from)
            .ok_or_else(|| not_found(PROFILE))
    }

    async fn list_leaderboard(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<LevelProfile>, RepositoryError> {
        // `total_xp` is what a rank means, and it is what `rank_of` counts against —
        // the two must agree or a member's page position and their reported rank
        // would disagree.
        let query = leveling_profiles::Entity::find()
            .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_desc(leveling_profiles::Column::TotalXp)
            .order_by_asc(leveling_profiles::Column::DiscordUserId);

        paginate_models(
            query,
            &self.db,
            params,
            PROFILE,
            PROFILE_CONFLICT,
            LevelProfile::from,
        )
        .await
    }

    async fn list_profiles(&self, guild_id: u64) -> Result<Vec<LevelProfile>, RepositoryError> {
        // Ordered by the same key the leaderboard uses, so a reconciliation pass
        // and a leaderboard page see guild members in a consistent order.
        let models = leveling_profiles::Entity::find()
            .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_desc(leveling_profiles::Column::TotalXp)
            .order_by_asc(leveling_profiles::Column::DiscordUserId)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?;

        Ok(models.into_iter().map(LevelProfile::from).collect())
    }

    async fn record_member_name(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        display_name: &str,
        is_bot: bool,
    ) -> Result<(), RepositoryError> {
        let now = from_utc(Utc::now());

        // `ON CONFLICT ... DO UPDATE` rather than a read-then-write: the bot sees the
        // same member on every message, and the cache must not grow a row per sighting.
        // Only the name and the sighting time move; `joined_at` belongs to the join
        // event and must not be overwritten by a chat message.
        guild_member_profiles::Entity::insert(guild_member_profiles::ActiveModel {
            id: sea_orm::ActiveValue::Set(Uuid::new_v4()),
            guild_id: sea_orm::ActiveValue::Set(snowflake(guild_id)),
            discord_user_id: sea_orm::ActiveValue::Set(snowflake(discord_user_id)),
            display_name: sea_orm::ActiveValue::Set(Some(display_name.to_string())),
            is_bot: sea_orm::ActiveValue::Set(is_bot),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
            last_seen_at: sea_orm::ActiveValue::Set(Some(now)),
            joined_at: sea_orm::ActiveValue::NotSet,
            is_blacklisted: sea_orm::ActiveValue::NotSet,
        })
        .on_conflict(
            OnConflict::columns([
                guild_member_profiles::Column::GuildId,
                guild_member_profiles::Column::DiscordUserId,
            ])
            .update_columns([
                guild_member_profiles::Column::DisplayName,
                guild_member_profiles::Column::IsBot,
                guild_member_profiles::Column::LastSeenAt,
                guild_member_profiles::Column::UpdatedAt,
            ])
            .to_owned(),
        )
        .exec_without_returning(&self.db)
        .await
        .map_err(|error| map_db_err(MEMBER, MEMBER_CONFLICT, error))?;

        Ok(())
    }

    async fn member_names(
        &self,
        guild_id: u64,
        discord_user_ids: &[u64],
    ) -> Result<std::collections::HashMap<u64, String>, RepositoryError> {
        if discord_user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        // One query for the whole page rather than one per row: this runs on the
        // leaderboard path, where a per-row lookup is the classic N+1.
        let ids: Vec<i64> = discord_user_ids.iter().copied().map(snowflake).collect();

        let rows = guild_member_profiles::Entity::find()
            .filter(guild_member_profiles::Column::GuildId.eq(snowflake(guild_id)))
            .filter(guild_member_profiles::Column::DiscordUserId.is_in(ids))
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(MEMBER, MEMBER_CONFLICT, error))?;

        Ok(rows
            .into_iter()
            .filter_map(|row| {
                row.display_name
                    .map(|name| (snowflake_from_db(row.discord_user_id), name))
            })
            .collect())
    }

    async fn rank_of(&self, guild_id: u64, discord_user_id: u64) -> Result<u64, RepositoryError> {
        // One `COUNT(*)` of the profiles strictly ahead, plus one for the member's own
        // position. Reading the leaderboard to find a position would cost a page per
        // rank and break down past the end of the first page.
        let mine = self
            .find_profile(guild_id, discord_user_id)
            .await?
            .map(|profile| profile.total_xp);

        // A member with no profile is at the bottom, and everyone else is ahead.
        let ahead = match mine {
            Some(total_xp) => leveling_profiles::Entity::find()
                .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
                .filter(leveling_profiles::Column::TotalXp.gt(total_xp))
                .count(&self.db)
                .await
                .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?,
            None => leveling_profiles::Entity::find()
                .filter(leveling_profiles::Column::GuildId.eq(snowflake(guild_id)))
                .count(&self.db)
                .await
                .map_err(|error| map_db_err(PROFILE, PROFILE_CONFLICT, error))?,
        };

        Ok(ahead + 1)
    }

    async fn list_rewards(&self, guild_id: u64) -> Result<Vec<LevelReward>, RepositoryError> {
        let models = leveling_rewards::Entity::find()
            .filter(leveling_rewards::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_asc(leveling_rewards::Column::Level)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(REWARD, REWARD_CONFLICT, error))?;

        Ok(models.into_iter().map(LevelReward::from).collect())
    }

    async fn create_reward(&self, reward: LevelReward) -> Result<LevelReward, RepositoryError> {
        let model = leveling_rewards::ActiveModel {
            id: sea_orm::ActiveValue::Set(reward.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(reward.guild_id)),
            level: sea_orm::ActiveValue::Set(reward.level),
            role_id: sea_orm::ActiveValue::Set(snowflake(reward.role_id)),
            keep_on_demote: sea_orm::ActiveValue::Set(reward.keep_on_demote),
            created_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
        };

        let created = leveling_rewards::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(REWARD, REWARD_CONFLICT, error))?;

        Ok(created.into())
    }

    async fn delete_reward(&self, id: Uuid) -> Result<(), RepositoryError> {
        let result = leveling_rewards::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(REWARD, REWARD_CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(REWARD));
        }

        Ok(())
    }

    async fn rewards_reached(
        &self,
        guild_id: u64,
        level: i32,
    ) -> Result<Vec<LevelReward>, RepositoryError> {
        let models = leveling_rewards::Entity::find()
            .filter(leveling_rewards::Column::GuildId.eq(snowflake(guild_id)))
            .filter(leveling_rewards::Column::Level.lte(level))
            .order_by_asc(leveling_rewards::Column::Level)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(REWARD, REWARD_CONFLICT, error))?;

        Ok(models.into_iter().map(LevelReward::from).collect())
    }
}

impl From<leveling_configs::Model> for LevelingConfig {
    fn from(model: leveling_configs::Model) -> Self {
        Self {
            guild_id: snowflake_from_db(model.guild_id),
            enabled: model.enabled,
            xp_min: model.xp_min,
            xp_max: model.xp_max,
            cooldown_secs: model.cooldown_secs,
            voice_xp_multiplier: model.voice_xp_multiplier,
            // A curve name this build does not know is data written by a newer
            // version; falling back to the default keeps the guild earning XP
            // instead of panicking on a value the enum has no variant for.
            curve: LevelCurve::parse(&model.curve).unwrap_or(LevelCurve::Mrn),
            announce_channel_id: model.announce_channel_id.map(snowflake_from_db),
            stack_roles: model.stack_roles,
            ignored_channel_ids: ids(model.ignored_channel_ids),
            ignored_role_ids: ids(model.ignored_role_ids),
        }
    }
}

impl From<leveling_profiles::Model> for LevelProfile {
    fn from(model: leveling_profiles::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            discord_user_id: snowflake_from_db(model.discord_user_id),
            xp: model.xp,
            level: model.level,
            total_xp: model.total_xp,
            message_count: model.message_count,
            voice_seconds: model.voice_seconds,
            last_xp_at: model.last_xp_at.map(to_utc),
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}

impl From<leveling_rewards::Model> for LevelReward {
    fn from(model: leveling_rewards::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            level: model.level,
            role_id: snowflake_from_db(model.role_id),
            keep_on_demote: model.keep_on_demote,
        }
    }
}

/// `voice_seconds = voice_seconds + seconds`, built as SQL.
///
/// `ExprTrait` is imported inside this function rather than at module scope: its
/// `add`/`max` would otherwise shadow the integer methods the rest of the file
/// uses on plain numbers.
fn add_to_voice_seconds(seconds: i64) -> sea_orm::sea_query::SimpleExpr {
    use sea_orm::sea_query::{Expr, ExprTrait};

    Expr::col(leveling_profiles::Column::VoiceSeconds).add(seconds)
}

/// `bigint[]` is nullable in the schema; the domain's field is not.
///
/// A `NULL` and an empty array mean the same thing to every caller — "nothing is
/// ignored" — so the `NULL` case collapses to an empty vector here rather than
/// leaking an `Option` the domain would have to unwrap everywhere.
fn ids(values: Option<Vec<i64>>) -> Vec<u64> {
    values
        .unwrap_or_default()
        .into_iter()
        .map(snowflake_from_db)
        .collect()
}
