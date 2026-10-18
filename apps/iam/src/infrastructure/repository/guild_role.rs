//! Guild roles and their permission matrices, backed by SeaORM.

use chrono::Utc;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    TransactionTrait,
};
use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::rbac::{GuildRole, GuildRoleRepository, PermissionSet};

use super::entities::{guild_role_permissions, guild_roles, permissions};
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake};
use super::pagination::paginate_models;

const ROLE: &str = "guild role";
const ROLE_CONFLICT: &str = "a role with that slug already exists in this guild";
const MATRIX_CONFLICT: &str = "that permission is already granted to this role";

/// Guild roles and their permission matrices, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmGuildRoleRepository {
    db: DatabaseConnection,
}

impl SeaOrmGuildRoleRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Re-reads a role after a write that may have updated instead of inserted.
    async fn read(&self, id: Uuid) -> Result<GuildRole, RepositoryError> {
        guild_roles::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?
            .map(GuildRole::from)
            .ok_or_else(|| not_found(ROLE))
    }
}

impl GuildRoleRepository for SeaOrmGuildRoleRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<GuildRole>, RepositoryError> {
        let model = guild_roles::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        Ok(model.map(GuildRole::from))
    }

    async fn find_by_slug(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<Option<GuildRole>, RepositoryError> {
        let model = guild_roles::Entity::find()
            .filter(guild_roles::Column::GuildId.eq(snowflake(guild_id)))
            .filter(guild_roles::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        Ok(model.map(GuildRole::from))
    }

    async fn list_for_guild(&self, guild_id: u64) -> Result<Vec<GuildRole>, RepositoryError> {
        let models = guild_roles::Entity::find()
            .filter(guild_roles::Column::GuildId.eq(snowflake(guild_id)))
            // Position first because that is the order a guild owner expects to see
            // roles in; the slug breaks ties so the list does not shuffle between calls.
            .order_by_desc(guild_roles::Column::Position)
            .order_by_asc(guild_roles::Column::Slug)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        Ok(models.into_iter().map(GuildRole::from).collect())
    }

    async fn list(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<GuildRole>, RepositoryError> {
        // Position first because that is the order a guild owner expects to see
        // roles in; the slug breaks ties so the list does not shuffle between calls.
        let query = guild_roles::Entity::find()
            .filter(guild_roles::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_desc(guild_roles::Column::Position)
            .order_by_asc(guild_roles::Column::Slug);

        paginate_models(
            query,
            &self.db,
            params,
            ROLE,
            ROLE_CONFLICT,
            GuildRole::from,
        )
        .await
    }

    async fn create(
        &self,
        guild_id: u64,
        slug: &str,
        name: &str,
        description: Option<&str>,
        is_system: bool,
        position: i32,
    ) -> Result<GuildRole, RepositoryError> {
        let now = from_utc(Utc::now());
        let id = Uuid::new_v4();

        let model = guild_roles::ActiveModel {
            id: sea_orm::ActiveValue::Set(id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(guild_id)),
            slug: sea_orm::ActiveValue::Set(slug.to_string()),
            name: sea_orm::ActiveValue::Set(name.to_string()),
            description: sea_orm::ActiveValue::Set(description.map(str::to_string)),
            is_system: sea_orm::ActiveValue::Set(is_system),
            position: sea_orm::ActiveValue::Set(position),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        let created = guild_roles::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        let _ = id;
        Ok(created.into())
    }

    async fn update(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        position: Option<i32>,
    ) -> Result<GuildRole, RepositoryError> {
        if name.is_none() && description.is_none() && position.is_none() {
            return self.read(id).await;
        }

        let mut active = guild_roles::ActiveModel {
            id: sea_orm::ActiveValue::Set(id),
            ..Default::default()
        };

        if let Some(name) = name {
            active.name = sea_orm::ActiveValue::Set(name.to_string());
        }
        if let Some(description) = description {
            active.description = sea_orm::ActiveValue::Set(Some(description.to_string()));
        }
        if let Some(position) = position {
            active.position = sea_orm::ActiveValue::Set(position);
        }
        active.updated_at = sea_orm::ActiveValue::Set(from_utc(Utc::now()));

        let updated = guild_roles::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| match error {
                sea_orm::DbErr::RecordNotUpdated => not_found(ROLE),
                other => map_db_err(ROLE, ROLE_CONFLICT, other),
            })?;

        Ok(updated.into())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        // `guild_members.guild_role_id` is `ON DELETE RESTRICT`, so a role people
        // still hold fails here with a foreign-key violation rather than silently
        // demoting them — the migration comment asks for exactly that, and
        // `map_db_err` turns it into a `Conflict` a handler can explain.
        let result = guild_roles::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, "this role still has members assigned", error))?;

        if result.rows_affected == 0 {
            return Err(not_found(ROLE));
        }

        Ok(())
    }

    async fn permissions_for_role(&self, role_id: Uuid) -> Result<Vec<String>, RepositoryError> {
        let names = guild_role_permissions::Entity::find()
            .select_only()
            .column(permissions::Column::Name)
            .inner_join(permissions::Entity)
            .filter(guild_role_permissions::Column::GuildRoleId.eq(role_id))
            .order_by_asc(permissions::Column::Name)
            .into_tuple::<String>()
            .all(&self.db)
            .await
            .map_err(|error| map_db_err("permission", MATRIX_CONFLICT, error))?;

        Ok(names)
    }

    async fn replace_role_permissions(
        &self,
        role_id: Uuid,
        permission_names: &[String],
    ) -> Result<(), RepositoryError> {
        // Replace, not diff: the dashboard submits the whole matrix, and deleting
        // then inserting inside one transaction means a failure halfway leaves the
        // previous matrix intact rather than a partially edited one.
        let transaction = self
            .db
            .begin()
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        guild_role_permissions::Entity::delete_many()
            .filter(guild_role_permissions::Column::GuildRoleId.eq(role_id))
            .exec(&transaction)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        if !permission_names.is_empty() {
            // Resolved through the catalog rather than trusting the caller: a name
            // that is not in `permissions` has no id to grant, and silently skipping
            // it would make a typo look like a successful save.
            let ids = permissions::Entity::find()
                .select_only()
                .column(permissions::Column::Id)
                .filter(permissions::Column::Name.is_in(permission_names.iter().cloned()))
                .into_tuple::<Uuid>()
                .all(&transaction)
                .await
                .map_err(|error| map_db_err("permission", MATRIX_CONFLICT, error))?;

            if ids.len() != permission_names.len() {
                let known: Vec<String> = permissions::Entity::find()
                    .select_only()
                    .column(permissions::Column::Name)
                    .filter(permissions::Column::Id.is_in(ids.iter().copied()))
                    .into_tuple::<String>()
                    .all(&transaction)
                    .await
                    .map_err(|error| map_db_err("permission", MATRIX_CONFLICT, error))?;

                let unknown: Vec<&str> = permission_names
                    .iter()
                    .map(String::as_str)
                    .filter(|name| !known.iter().any(|held| held == name))
                    .collect();

                tracing::warn!(role_id = %role_id, unknown = ?unknown, "matrix edit named unknown permissions");
                return Err(RepositoryError::conflict(format!(
                    "unknown permission `{}`",
                    unknown.first().copied().unwrap_or("")
                )));
            }

            let rows: Vec<guild_role_permissions::ActiveModel> = ids
                .into_iter()
                .map(|permission_id| guild_role_permissions::ActiveModel {
                    guild_role_id: sea_orm::ActiveValue::Set(role_id),
                    permission_id: sea_orm::ActiveValue::Set(permission_id),
                })
                .collect();

            guild_role_permissions::Entity::insert_many(rows)
                .exec_without_returning(&transaction)
                .await
                .map_err(|error| map_db_err(ROLE, MATRIX_CONFLICT, error))?;
        }

        transaction
            .commit()
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))
    }

    async fn permissions_for_member(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> Result<PermissionSet, RepositoryError> {
        // One query: member → role → matrix → catalog. This is the authorization hot
        // path, and a per-role round trip here would be one query per request per
        // role the member holds.
        let names: Vec<String> = guild_role_permissions::Entity::find()
            .select_only()
            .column(permissions::Column::Name)
            .inner_join(super::entities::guild_roles::Entity)
            .inner_join(permissions::Entity)
            .filter(super::entities::guild_members::Column::GuildId.eq(snowflake(guild_id)))
            .filter(super::entities::guild_members::Column::UserId.eq(user_id))
            .into_tuple::<String>()
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(ROLE, ROLE_CONFLICT, error))?;

        // No rows means "not a member" or "member whose role grants nothing", and
        // both are the same answer to "may they?" — an empty set, not an error.
        Ok(PermissionSet::new(guild_id, names))
    }
}

impl From<guild_roles::Model> for GuildRole {
    fn from(model: guild_roles::Model) -> Self {
        Self {
            id: model.id,
            guild_id: super::mapping::snowflake_from_db(model.guild_id),
            slug: model.slug,
            name: model.name,
            description: model.description,
            is_system: model.is_system,
            position: model.position,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_role_model_converts_without_losing_its_guild() {
        let now = chrono::Utc::now().fixed_offset();
        let model = guild_roles::Model {
            id: Uuid::new_v4(),
            guild_id: -1,
            slug: "admin".to_owned(),
            name: "Admin".to_owned(),
            description: None,
            is_system: true,
            position: 10,
            created_at: now,
            updated_at: now,
        };

        // `-1` as `i64` is `u64::MAX`: the conversion must not narrow.
        assert_eq!(GuildRole::from(model).guild_id, u64::MAX);
    }
}
