use std::future::Future;
use uuid::Uuid;

use super::entity::{NewUser, User, UserPatch};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

/// Persistence contract for accounts.
///
/// Uses `impl Future` in the signatures (Rust 2024) rather than `#[async_trait]`:
/// the trait stays object-safe-free and the compiler can inline the returned
/// future, at the cost of the trait not being usable as `dyn UserRepository`.
/// Nothing in this codebase needs a trait object here — `AppState` holds the
/// concrete SeaORM implementation and use cases are generic over the trait.
pub trait UserRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<User>, RepositoryError>> + Send;

    fn find_by_discord_id(
        &self,
        discord_id: u64,
    ) -> impl Future<Output = Result<Option<User>, RepositoryError>> + Send;

    fn create(&self, user: NewUser) -> impl Future<Output = Result<User, RepositoryError>> + Send;

    fn update(
        &self,
        id: Uuid,
        patch: UserPatch,
    ) -> impl Future<Output = Result<User, RepositoryError>> + Send;

    fn delete(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn list(
        &self,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<User>, RepositoryError>> + Send;

    /// Promotes or demotes an account's platform-wide flag.
    fn set_super_admin(
        &self,
        id: Uuid,
        is_super_admin: bool,
    ) -> impl Future<Output = Result<User, RepositoryError>> + Send;
}
