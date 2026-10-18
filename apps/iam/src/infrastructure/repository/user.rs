//! Accounts, backed by SeaORM.

use chrono::Utc;
use sea_orm::{
    ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::user::{NewUser, User, UserPatch, UserRepository};

use super::entities::users;
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const ENTITY: &str = "user";
const CONFLICT: &str = "an account with that Discord id already exists";

/// Accounts, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmUserRepository {
    db: DatabaseConnection,
}

impl SeaOrmUserRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl UserRepository for SeaOrmUserRepository {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepositoryError> {
        let model = users::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(model.map(User::from))
    }

    async fn find_by_discord_id(&self, discord_id: u64) -> Result<Option<User>, RepositoryError> {
        let model = users::Entity::find()
            .filter(users::Column::DiscordId.eq(snowflake(discord_id)))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(model.map(User::from))
    }

    async fn create(&self, user: NewUser) -> Result<User, RepositoryError> {
        let model = users::ActiveModel {
            id: Set(user.id),
            discord_id: Set(snowflake(user.discord_id)),
            username: Set(user.username),
            global_name: Set(user.global_name),
            avatar_url: Set(user.avatar_url),
            email: Set(user.email),
            is_super_admin: Set(false),
            ..Default::default()
        };

        let created = users::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(created.into())
    }

    async fn update(&self, id: Uuid, patch: UserPatch) -> Result<User, RepositoryError> {
        // A patch that writes nothing must not touch `updated_at` either, or every
        // no-op login would look like a profile change to anything watching the row.
        if patch.is_empty() {
            return self.find_by_id(id).await?.ok_or_else(|| not_found(ENTITY));
        }

        let mut active = users::ActiveModel {
            id: Set(id),
            ..Default::default()
        };

        if let Some(username) = patch.username {
            active.username = Set(username);
        }
        if let Some(global_name) = patch.global_name {
            active.global_name = Set(Some(global_name));
        }
        if let Some(avatar_url) = patch.avatar_url {
            active.avatar_url = Set(Some(avatar_url));
        }
        if let Some(email) = patch.email {
            active.email = Set(Some(email));
        }
        active.updated_at = Set(from_utc(Utc::now()));

        let updated = users::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(updated.into())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        let result = users::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(ENTITY));
        }

        Ok(())
    }

    async fn list(&self, params: &PaginationParams) -> Result<Page<User>, RepositoryError> {
        // A stable order, or page 2 can repeat a row that page 1 already showed.
        let query = users::Entity::find()
            .order_by_asc(users::Column::CreatedAt)
            .order_by_asc(users::Column::Id);

        paginate_models(query, &self.db, params, ENTITY, CONFLICT, User::from).await
    }

    async fn set_super_admin(
        &self,
        id: Uuid,
        is_super_admin: bool,
    ) -> Result<User, RepositoryError> {
        let active = users::ActiveModel {
            id: Set(id),
            is_super_admin: Set(is_super_admin),
            updated_at: Set(from_utc(Utc::now())),
            ..Default::default()
        };

        let updated = users::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(updated.into())
    }
}

impl From<users::Model> for User {
    fn from(model: users::Model) -> Self {
        Self {
            id: model.id,
            discord_id: snowflake_from_db(model.discord_id),
            username: model.username,
            global_name: model.global_name,
            avatar_url: model.avatar_url,
            email: model.email,
            is_super_admin: model.is_super_admin,
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}
