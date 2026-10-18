use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::domain::rbac::{GuildRole, PermissionSet};

/// A dashboard user's relationship to one guild.
///
/// This row is what makes the application multi-tenant: authorization questions
/// are always "this user, in this guild", never "this user" alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildMember {
    pub id: Uuid,
    pub guild_id: u64,
    pub user_id: Uuid,
    pub guild_role_id: Uuid,
    /// Discord's permission bitfield for this member, captured at login.
    pub discord_permissions: u64,
    pub discord_nick: Option<String>,
    pub synced_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A member together with the role it points at.
///
/// The dashboard almost always wants both — the role's name for display and its
/// slug for the check — and fetching them separately is the classic N+1.
#[derive(Debug, Clone)]
pub struct GuildMemberWithRole {
    pub member: GuildMember,
    pub role: GuildRole,
}

impl GuildMemberWithRole {
    /// Whether this member's Discord permissions alone would let them administer
    /// the guild, independent of the dashboard's own matrix.
    pub fn is_discord_admin(&self) -> bool {
        crate::domain::guild::has_manage_guild(self.member.discord_permissions)
    }
}

/// A new membership row, created on first login for a guild.
#[derive(Debug, Clone)]
pub struct NewGuildMember {
    pub id: Uuid,
    pub guild_id: u64,
    pub user_id: Uuid,
    pub guild_role_id: Uuid,
    pub discord_permissions: u64,
    pub discord_nick: Option<String>,
}

/// The permissions a member holds in a guild, ready for a handler to check.
///
/// Bundles the resolved [`PermissionSet`] with the context needed to log a denial
/// meaningfully.
#[derive(Debug, Clone)]
pub struct GuildAccess {
    pub guild_id: u64,
    pub user_id: Uuid,
    pub role_slug: String,
    pub is_super_admin: bool,
    pub permissions: PermissionSet,
}

impl GuildAccess {
    /// A platform super-admin bypasses the guild matrix entirely.
    ///
    /// This is the escape hatch that keeps a broken matrix from bricking a guild:
    /// whoever operates the installation can always get back in.
    pub fn allows(&self, permission: &str) -> bool {
        self.is_super_admin || self.permissions.allows(permission)
    }

    pub fn allows_all(&self, required: &[&str]) -> bool {
        self.is_super_admin || self.permissions.allows_all(required)
    }
}
