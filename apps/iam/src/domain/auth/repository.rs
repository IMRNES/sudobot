use std::future::Future;
use uuid::Uuid;

use super::entity::{NewRefreshToken, RefreshToken};
use crate::domain::errors::RepositoryError;

pub trait RefreshTokenRepository: Send + Sync {
    fn create(
        &self,
        token: NewRefreshToken,
    ) -> impl Future<Output = Result<RefreshToken, RepositoryError>> + Send;

    fn find_by_hash(
        &self,
        token_hash: &str,
    ) -> impl Future<Output = Result<Option<RefreshToken>, RepositoryError>> + Send;

    /// Marks one token as used.
    ///
    /// Refresh rotation is the reason this exists: each refresh invalidates the
    /// token it consumed, so a replay of a stolen token fails.
    fn revoke(
        &self,
        id: Uuid,
        revoked_at: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Revokes every live token for a user — the "log out everywhere" path.
    fn revoke_all_for_user(
        &self,
        user_id: Uuid,
        revoked_at: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;

    /// Deletes tokens that expired before `before`, keeping the table small.
    fn delete_expired_before(
        &self,
        before: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}
