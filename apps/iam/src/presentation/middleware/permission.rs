use uuid::Uuid;

use crate::domain::guild::GuildRepository;
use crate::domain::guild_member::{GuildAccess, GuildMemberRepository};
use crate::domain::rbac::GuildRoleRepository;
use crate::domain::rbac::PermissionSet;
use crate::domain::user::UserRepository;
use crate::presentation::error::{AppError, PERMISSION_DENIED};
use crate::presentation::state::AppState;

/// What the caller may do in the guild the request names.
///
/// Resolved once per request by [`load_guild_context`] and then handed to handlers,
/// so a handler that checks two permissions does not query the matrix twice — and,
/// more importantly, so every authorization question about one request is answered
/// from one snapshot.
#[derive(Debug, Clone)]
pub struct GuildContext {
    pub guild_id: u64,
    pub access: GuildAccess,
}

impl GuildContext {
    pub fn allows(&self, permission: &str) -> bool {
        self.access.allows(permission)
    }
}

/// Resolves the caller's authority in one guild.
///
/// Deliberately a function handlers call rather than a middleware: it needs the
/// guild id from the path, which a layer would have to re-parse, and it needs to be
/// skippable for routes that are not guild-scoped. Handlers call it first, which
/// also puts the check at the top of the body where a reader will see it.
pub async fn load_guild_context(
    state: &AppState,
    user_id: Uuid,
    guild_id: u64,
) -> Result<GuildContext, AppError> {
    // The guild must exist and the bot must be in it: operating on a guild the bot
    // has left would produce settings nobody can act on.
    let guild = state
        .guilds
        .find_by_id(guild_id)
        .await?
        .ok_or_else(|| AppError::NotFound("guild not found".into()))?;

    if !guild.bot_present {
        return Err(AppError::Conflict(
            "the bot is not in this server; invite it again to manage it".into(),
        ));
    }

    // Membership is the multi-tenant boundary: not a member means no access at all,
    // regardless of the platform flag.
    let member = state
        .guild_members
        .find_with_role(guild_id, user_id)
        .await?
        .ok_or_else(|| AppError::Forbidden("you are not a member of this server".into()))?;

    let is_super_admin = state
        .users
        .find_by_id(user_id)
        .await?
        .map(|user| user.is_platform_admin())
        .unwrap_or(false);

    let permissions = state
        .guild_roles
        .permissions_for_member(guild_id, user_id)
        .await?;

    Ok(GuildContext {
        guild_id,
        access: GuildAccess {
            guild_id,
            user_id,
            role_slug: member.role.slug,
            is_super_admin,
            permissions,
        },
    })
}

/// Refuses the request unless the caller holds `permission`.
///
/// The refusal names the permission in the response body's `code`, because a
/// dashboard that cannot say *which* capability is missing can only show "denied".
pub fn ensure_permission(context: &GuildContext, permission: &str) -> Result<(), AppError> {
    if context.allows(permission) {
        return Ok(());
    }

    tracing::warn!(
        guild_id = context.guild_id,
        user_id = %context.access.user_id,
        permission,
        "permission denied"
    );

    Err(AppError::Forbidden(PERMISSION_DENIED.to_string()))
}

/// Refuses the request unless the caller holds every named permission.
pub fn ensure_permissions(context: &GuildContext, required: &[&str]) -> Result<(), AppError> {
    for permission in required {
        ensure_permission(context, permission)?;
    }
    Ok(())
}

/// The permissions a caller holds, for the `/me` payload.
///
/// Exposed so the dashboard can hide or disable controls without a second request
/// per guild, and so a read-only user still sees what exists.
pub fn permission_names(set: &PermissionSet) -> Vec<String> {
    set.iter().map(str::to_string).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn context_with(permissions: &[&str], is_super_admin: bool) -> GuildContext {
        GuildContext {
            guild_id: 1,
            access: GuildAccess {
                guild_id: 1,
                user_id: Uuid::nil(),
                role_slug: "viewer".into(),
                is_super_admin,
                permissions: PermissionSet::new(
                    1,
                    permissions.iter().map(|name| name.to_string()).collect(),
                ),
            },
        }
    }

    #[test]
    fn allows_a_held_permission() {
        let context = context_with(&["economy:read"], false);
        assert!(ensure_permission(&context, "economy:read").is_ok());
    }

    #[test]
    fn refuses_a_missing_permission() {
        let context = context_with(&["economy:read"], false);
        assert!(ensure_permission(&context, "economy:write").is_err());
    }

    #[test]
    fn a_super_admin_bypasses_the_matrix() {
        // The escape hatch that keeps a broken matrix from bricking a guild.
        let context = context_with(&[], true);
        assert!(ensure_permission(&context, "dashboard:manage").is_ok());
    }
}
