//! Guild memberships and the authorization lookup, backed by SeaORM.

use chrono::Utc;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, JoinType, QueryFilter, QueryOrder, QuerySelect,
    RelationTrait,
};
use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::guild_member::{
    GuildMember, GuildMemberRepository, GuildMemberWithRole, NewGuildMember,
};
use crate::domain::rbac::GuildRole;

use super::entities::{guild_members, guild_role_permissions, guild_roles, permissions};
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const ENTITY: &str = "guild member";
const CONFLICT: &str = "this user is already a member of that guild";

/// Guild memberships, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmGuildMemberRepository {
    db: DatabaseConnection,
}

impl SeaOrmGuildMemberRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// The member joined to the role it points at.
    ///
    /// One query rather than a member read followed by a role read: the dashboard's
    /// member list wants both, and the second read would be an N+1 across a page.
    async fn with_role(
        &self,
        query: sea_orm::Select<guild_members::Entity>,
    ) -> Result<Vec<GuildMemberWithRole>, RepositoryError> {
        let rows: Vec<(guild_members::Model, Option<guild_roles::Model>)> = query
            .find_also_related(guild_roles::Entity)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(rows
            .into_iter()
            .filter_map(|(member, role)| {
                // A left join because a membership whose role vanished would still be
                // a row; the foreign key is `RESTRICT`, so this cannot happen in
                // practice, and skipping it here is better than panicking on it.
                role.map(|role| GuildMemberWithRole {
                    member: GuildMember::from(member),
                    role: GuildRole::from(role),
                })
            })
            .collect())
    }
}

impl GuildMemberRepository for SeaOrmGuildMemberRepository {
    async fn find(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> Result<Option<GuildMember>, RepositoryError> {
        let model = guild_members::Entity::find()
            .filter(guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .filter(guild_members::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(model.map(GuildMember::from))
    }

    async fn find_with_role(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> Result<Option<GuildMemberWithRole>, RepositoryError> {
        let query = guild_members::Entity::find()
            .filter(guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .filter(guild_members::Column::UserId.eq(user_id));

        Ok(self.with_role(query).await?.into_iter().next())
    }

    async fn list_for_guild(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<GuildMemberWithRole>, RepositoryError> {
        // A membership has no natural display order; the creation time is at least
        // stable, which is what keeps a page from repeating rows.
        let query = guild_members::Entity::find()
            .filter(guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_asc(guild_members::Column::CreatedAt)
            .order_by_asc(guild_members::Column::Id);

        let page =
            paginate_models(query, &self.db, params, ENTITY, CONFLICT, |model| model).await?;

        // The roles for the page, in one query keyed by the ids the page returned —
        // not one query per row, which is the N+1 this joined read exists to avoid.
        let role_ids: Vec<Uuid> = page
            .data
            .iter()
            .map(|member| member.guild_role_id)
            .collect();

        let roles = guild_roles::Entity::find()
            .filter(guild_roles::Column::Id.is_in(role_ids))
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        let by_id: std::collections::HashMap<Uuid, guild_roles::Model> =
            roles.into_iter().map(|role| (role.id, role)).collect();

        let members: Option<Vec<GuildMemberWithRole>> = page
            .data
            .into_iter()
            .map(|member| {
                by_id
                    .get(&member.guild_role_id)
                    .cloned()
                    .map(|role| GuildMemberWithRole {
                        member: GuildMember::from(member),
                        role: GuildRole::from(role),
                    })
            })
            .collect();

        Ok(Page::new(
            members.ok_or_else(|| {
                RepositoryError::database("a guild member row points at a role that does not exist")
            })?,
            params,
            page.total,
        ))
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<GuildMemberWithRole>, RepositoryError> {
        let query = guild_members::Entity::find()
            .filter(guild_members::Column::UserId.eq(user_id))
            .order_by_asc(guild_members::Column::GuildId);

        self.with_role(query).await
    }

    async fn create(&self, member: NewGuildMember) -> Result<GuildMember, RepositoryError> {
        let now = from_utc(Utc::now());

        let model = guild_members::ActiveModel {
            id: sea_orm::ActiveValue::Set(member.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(member.guild_id)),
            user_id: sea_orm::ActiveValue::Set(member.user_id),
            guild_role_id: sea_orm::ActiveValue::Set(member.guild_role_id),
            discord_permissions: sea_orm::ActiveValue::Set(snowflake(member.discord_permissions)),
            discord_nick: sea_orm::ActiveValue::Set(member.discord_nick),
            synced_at: sea_orm::ActiveValue::Set(now),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        let created = guild_members::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(created.into())
    }

    async fn upsert_from_discord(
        &self,
        member: NewGuildMember,
    ) -> Result<GuildMember, RepositoryError> {
        let now = from_utc(Utc::now());

        let model = guild_members::ActiveModel {
            id: sea_orm::ActiveValue::Set(member.id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(member.guild_id)),
            user_id: sea_orm::ActiveValue::Set(member.user_id),
            guild_role_id: sea_orm::ActiveValue::Set(member.guild_role_id),
            discord_permissions: sea_orm::ActiveValue::Set(snowflake(member.discord_permissions)),
            discord_nick: sea_orm::ActiveValue::Set(member.discord_nick),
            synced_at: sea_orm::ActiveValue::Set(now),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        // `ON CONFLICT (guild_id, user_id) DO UPDATE`: logging in again after a
        // permission change is the normal path, and a select-then-insert would race
        // the two guilds a login syncs in parallel.
        //
        // `created_at` is deliberately not updated — it is when the membership
        // started, not when it was last seen.
        guild_members::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([
                    guild_members::Column::GuildId,
                    guild_members::Column::UserId,
                ])
                .update_columns([
                    guild_members::Column::GuildRoleId,
                    guild_members::Column::DiscordPermissions,
                    guild_members::Column::DiscordNick,
                    guild_members::Column::SyncedAt,
                    guild_members::Column::UpdatedAt,
                ])
                .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        self.find(member.guild_id, member.user_id)
            .await?
            .ok_or_else(|| not_found(ENTITY))
    }

    async fn update_role(
        &self,
        guild_id: u64,
        user_id: Uuid,
        guild_role_id: Uuid,
    ) -> Result<GuildMember, RepositoryError> {
        let active = guild_members::ActiveModel {
            guild_role_id: sea_orm::ActiveValue::Set(guild_role_id),
            updated_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
            ..Default::default()
        };

        let result = guild_members::Entity::update_many()
            .set(active)
            .filter(guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .filter(guild_members::Column::UserId.eq(user_id))
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        result
            .into_iter()
            .next()
            .map(GuildMember::from)
            .ok_or_else(|| not_found(ENTITY))
    }

    async fn delete(&self, guild_id: u64, user_id: Uuid) -> Result<(), RepositoryError> {
        let result = guild_members::Entity::delete_many()
            .filter(guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .filter(guild_members::Column::UserId.eq(user_id))
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(ENTITY));
        }

        Ok(())
    }

    async fn count_holders_of(
        &self,
        guild_id: u64,
        permission: &str,
    ) -> Result<u64, RepositoryError> {
        // Distinct members, not rows: a member holds one role, but the join fans out
        // across that role's matrix, and counting rows would report the size of the
        // matrix instead of the number of people.
        let total = guild_members::Entity::find()
            .select_only()
            .column_as(
                sea_orm::sea_query::ExprTrait::count_distinct(Expr::col((
                    guild_members::Entity,
                    guild_members::Column::Id,
                ))),
                "holders",
            )
            .join_join(
                JoinType::InnerJoin,
                guild_members::Relation::GuildRole.def(),
                None,
            )
            .join_join(
                JoinType::InnerJoin,
                guild_roles::Relation::GuildRolePermission.def(),
                None,
            )
            .join_join(
                JoinType::InnerJoin,
                guild_role_permissions::Relation::Permission.def(),
                None,
            )
            .filter(guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .filter(permissions::Column::Name.eq(permission))
            .into_tuple::<i64>()
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(u64::try_from(total.unwrap_or(0)).unwrap_or(0))
    }
}

impl From<guild_members::Model> for GuildMember {
    fn from(model: guild_members::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            user_id: model.user_id,
            guild_role_id: model.guild_role_id,
            discord_permissions: snowflake_from_db(model.discord_permissions),
            discord_nick: model.discord_nick,
            synced_at: to_utc(model.synced_at),
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}
