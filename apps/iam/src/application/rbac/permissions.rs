use uuid::Uuid;

use crate::domain::errors::RepositoryError;
use crate::domain::rbac::{
    GuildRole, GuildRoleRepository, Permission, PermissionRepository, RbacError,
};

/// The permission catalog the matrix editor renders its rows from.
///
/// Not guild-scoped: the names are global, seeded once by bootstrap, and the same
/// checkboxes appear in every guild.
pub struct ListPermissionsUseCase<P> {
    permissions: P,
}

impl<P: PermissionRepository> ListPermissionsUseCase<P> {
    pub fn new(permissions: P) -> Self {
        Self { permissions }
    }

    pub async fn execute(&self) -> Result<Vec<Permission>, RbacError> {
        Ok(self.permissions.list().await?)
    }
}

/// Turns a repository failure into the error the caller sees.
///
/// The domain enum is frozen and carries no "the row is gone" variant of its own,
/// so a missing role is reported as `RbacError::RoleNotFound`; every other
/// repository failure travels through unchanged.
pub(super) fn map_role_error(error: RepositoryError) -> RbacError {
    match error {
        RepositoryError::NotFound { .. } => RbacError::RoleNotFound,
        other => RbacError::from(other),
    }
}

/// Loads a role and confirms it belongs to the guild in the path.
///
/// Without the guild comparison a caller could edit a role in a guild they
/// administer by quoting another guild's role id — the classic cross-tenant hole
/// in a `/{guildId}/roles/{roleId}` route. Every role-scoped use case goes through
/// here so the check cannot be forgotten in one of them.
pub(super) async fn load_role<R: GuildRoleRepository>(
    roles: &R,
    guild_id: u64,
    role_id: Uuid,
) -> Result<GuildRole, RbacError> {
    roles
        .find_by_id(role_id)
        .await
        .map_err(map_role_error)?
        .filter(|role| role.guild_id == guild_id)
        .ok_or(RbacError::RoleNotFound)
}
