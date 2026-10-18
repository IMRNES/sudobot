use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum RbacError {
    #[error("role not found")]
    RoleNotFound,

    #[error("permission `{0}` does not exist")]
    UnknownPermission(String),

    #[error("role `{0}` is a system role and cannot be deleted")]
    SystemRoleImmutable(String),

    #[error("role `{0}` still has members assigned")]
    RoleInUse(String),

    #[error("a guild must keep at least one role granting `{0}`")]
    WouldRemoveLastAdmin(String),

    #[error("you do not have permission `{0}` in this guild")]
    PermissionDenied(String),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl RbacError {
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::RoleNotFound
                | Self::UnknownPermission(_)
                | Self::SystemRoleImmutable(_)
                | Self::RoleInUse(_)
                | Self::WouldRemoveLastAdmin(_)
                | Self::PermissionDenied(_)
        )
    }

    pub fn permission_denied(permission: &str) -> Self {
        tracing::warn!(permission, "permission denied");
        Self::PermissionDenied(permission.to_string())
    }
}

/// Rejects a role slug that is not usable as an identifier.
///
/// Slugs appear in URLs and in the permission check path, so they stay lowercase,
/// short and punctuation-free.
pub fn validate_slug(slug: &str) -> Result<String, RbacError> {
    let normalized = slug.trim().to_ascii_lowercase();

    let valid = !normalized.is_empty()
        && normalized.len() <= 32
        && normalized.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        });

    if !valid {
        return Err(RbacError::UnknownPermission(format!(
            "invalid slug `{slug}`: use 1-32 characters of a-z, 0-9 or -"
        )));
    }

    Ok(normalized)
}

/// Guards the matrix edit that would lock everyone out of a guild.
///
/// Called with the permissions the role would hold *after* the change: if no role
/// in the guild keeps `dashboard:manage`, the edit is refused. Without this, an
/// admin can uncheck their own last capability and lose the guild permanently.
pub fn ensure_guild_stays_administrable(
    remaining_grants: usize,
    role: Uuid,
) -> Result<(), RbacError> {
    if remaining_grants == 0 {
        tracing::warn!(%role, "refused a matrix edit that would leave no role with dashboard:manage");
        return Err(RbacError::WouldRemoveLastAdmin(
            super::entity::names::DASHBOARD_MANAGE.to_string(),
        ));
    }
    Ok(())
}
