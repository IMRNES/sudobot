//! Refresh tokens, backed by SeaORM.

use chrono::{DateTime, Utc};
use sea_orm::{ActiveValue::Set, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::domain::auth::{NewRefreshToken, RefreshToken, RefreshTokenRepository};
use crate::domain::errors::RepositoryError;

use super::entities::refresh_tokens;
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, to_utc};

const ENTITY: &str = "refresh token";
const CONFLICT: &str = "that refresh token has already been issued";

/// Refresh tokens, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmRefreshTokenRepository {
    db: DatabaseConnection,
}

impl SeaOrmRefreshTokenRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

impl RefreshTokenRepository for SeaOrmRefreshTokenRepository {
    async fn create(&self, token: NewRefreshToken) -> Result<RefreshToken, RepositoryError> {
        let model = refresh_tokens::ActiveModel {
            id: Set(token.id),
            user_id: Set(token.user_id),
            token_hash: Set(token.token_hash),
            expires_at: Set(from_utc(token.expires_at)),
            ..Default::default()
        };

        let created = refresh_tokens::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(created.into())
    }

    async fn find_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RepositoryError> {
        let model = refresh_tokens::Entity::find()
            .filter(refresh_tokens::Column::TokenHash.eq(token_hash))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(model.map(RefreshToken::from))
    }

    async fn revoke(&self, id: Uuid, revoked_at: DateTime<Utc>) -> Result<(), RepositoryError> {
        let active = refresh_tokens::ActiveModel {
            id: Set(id),
            revoked_at: Set(Some(from_utc(revoked_at))),
            ..Default::default()
        };

        refresh_tokens::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| match error {
                sea_orm::DbErr::RecordNotUpdated => not_found(ENTITY),
                other => map_db_err(ENTITY, CONFLICT, other),
            })?;

        Ok(())
    }

    async fn revoke_all_for_user(
        &self,
        user_id: Uuid,
        revoked_at: DateTime<Utc>,
    ) -> Result<u64, RepositoryError> {
        let result = refresh_tokens::Entity::update_many()
            .col_expr(
                refresh_tokens::Column::RevokedAt,
                sea_orm::sea_query::Expr::value(from_utc(revoked_at)),
            )
            // Already-revoked rows are left alone so the returned count is "sessions
            // this call ended", not "rows that happen to be dead".
            .filter(refresh_tokens::Column::UserId.eq(user_id))
            .filter(refresh_tokens::Column::RevokedAt.is_null())
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(result.rows_affected)
    }

    async fn delete_expired_before(&self, before: DateTime<Utc>) -> Result<u64, RepositoryError> {
        let result = refresh_tokens::Entity::delete_many()
            .filter(refresh_tokens::Column::ExpiresAt.lt(from_utc(before)))
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(ENTITY, CONFLICT, error))?;

        Ok(result.rows_affected)
    }
}

impl From<refresh_tokens::Model> for RefreshToken {
    fn from(model: refresh_tokens::Model) -> Self {
        Self {
            id: model.id,
            user_id: model.user_id,
            token_hash: model.token_hash,
            expires_at: to_utc(model.expires_at),
            revoked_at: model.revoked_at.map(to_utc),
            created_at: to_utc(model.created_at),
        }
    }
}
