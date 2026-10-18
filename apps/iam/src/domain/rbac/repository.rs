use std::future::Future;
use uuid::Uuid;

use super::entity::{GuildRole, Permission, PermissionSet};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

/// Persistence contract for the permission catalog.
pub trait PermissionRepository: Send + Sync {
    fn find_by_name(
        &self,
        name: &str,
    ) -> impl Future<Output = Result<Option<Permission>, RepositoryError>> + Send;

    fn list(&self) -> impl Future<Output = Result<Vec<Permission>, RepositoryError>> + Send;

    /// Inserts the permission, or updates its description when it already exists.
    ///
    /// Bootstrap calls this on every start, so it must be safe to repeat.
    fn upsert(
        &self,
        name: &str,
        group: &str,
        description: &str,
    ) -> impl Future<Output = Result<Permission, RepositoryError>> + Send;
}

/// Persistence contract for per-guild roles and their matrices.
pub trait GuildRoleRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<GuildRole>, RepositoryError>> + Send;

    fn find_by_slug(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> impl Future<Output = Result<Option<GuildRole>, RepositoryError>> + Send;

    fn list_for_guild(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<Vec<GuildRole>, RepositoryError>> + Send;

    fn list(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<GuildRole>, RepositoryError>> + Send;

    fn create(
        &self,
        guild_id: u64,
        slug: &str,
        name: &str,
        description: Option<&str>,
        is_system: bool,
        position: i32,
    ) -> impl Future<Output = Result<GuildRole, RepositoryError>> + Send;

    fn update(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        position: Option<i32>,
    ) -> impl Future<Output = Result<GuildRole, RepositoryError>> + Send;

    fn delete(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Permission names granted to a role.
    fn permissions_for_role(
        &self,
        role_id: Uuid,
    ) -> impl Future<Output = Result<Vec<String>, RepositoryError>> + Send;

    /// Replaces a role's entire matrix in one transaction.
    ///
    /// Replace rather than add/remove: the dashboard submits the matrix as a
    /// whole, and diffing client-side state against server state invites races.
    fn replace_role_permissions(
        &self,
        role_id: Uuid,
        permission_names: &[String],
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The resolved permission set for one user in one guild.
    ///
    /// The single query the authorization path depends on; returns an empty set
    /// when the user is not a member rather than an error, because "not a member"
    /// and "member with no permissions" are the same answer to "may they?".
    fn permissions_for_member(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> impl Future<Output = Result<PermissionSet, RepositoryError>> + Send;
}
