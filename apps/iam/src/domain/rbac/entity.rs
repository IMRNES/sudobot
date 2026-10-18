use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A capability, named `group:action`.
///
/// The catalog is global: the same names exist for every guild, seeded once by
/// bootstrap. Storing permissions as rows rather than an enum is what lets the
/// dashboard edit a role's matrix without a deploy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Permission {
    pub id: Uuid,
    pub name: String,
    pub group: String,
    pub description: String,
}

/// The well-known permission names.
///
/// Referenced by bootstrap when seeding and by handlers when checking, so a typo
/// is a compile error rather than a check that silently never passes.
pub mod names {
    pub const DASHBOARD_MANAGE: &str = "dashboard:manage";
    pub const MEMBERS_READ: &str = "members:read";
    pub const MEMBERS_WRITE: &str = "members:write";

    pub const ECONOMY_READ: &str = "economy:read";
    pub const ECONOMY_WRITE: &str = "economy:write";

    pub const LEVELING_READ: &str = "leveling:read";
    pub const LEVELING_WRITE: &str = "leveling:write";

    pub const WELCOME_READ: &str = "welcome:read";
    pub const WELCOME_WRITE: &str = "welcome:write";

    pub const AUTOROLE_READ: &str = "autorole:read";
    pub const AUTOROLE_WRITE: &str = "autorole:write";

    pub const ROLES_READ: &str = "roles:read";
    pub const ROLES_WRITE: &str = "roles:write";

    /// Every permission, with the group it belongs to and what it grants.
    pub const CATALOG: &[(&str, &str, &str)] = &[
        (
            DASHBOARD_MANAGE,
            "dashboard",
            "Change guild settings and manage the dashboard",
        ),
        (
            MEMBERS_READ,
            "members",
            "View guild members and their roles",
        ),
        (
            MEMBERS_WRITE,
            "members",
            "Change member roles and blacklist members",
        ),
        (
            ECONOMY_READ,
            "economy",
            "View balances, the ledger and the shop",
        ),
        (
            ECONOMY_WRITE,
            "economy",
            "Grant or remove currency and edit shop items",
        ),
        (
            LEVELING_READ,
            "leveling",
            "View XP settings, rewards and leaderboards",
        ),
        (
            LEVELING_WRITE,
            "leveling",
            "Change XP settings and reward roles",
        ),
        (
            WELCOME_READ,
            "welcome",
            "View welcome and goodbye configuration",
        ),
        (
            WELCOME_WRITE,
            "welcome",
            "Change welcome and goodbye messages",
        ),
        (
            AUTOROLE_READ,
            "autorole",
            "View role panels and automatic roles",
        ),
        (
            AUTOROLE_WRITE,
            "autorole",
            "Change role panels and automatic roles",
        ),
        (
            ROLES_READ,
            "roles",
            "View guild roles and their permissions",
        ),
        (
            ROLES_WRITE,
            "roles",
            "Create roles and edit their permission matrix",
        ),
    ];
}

/// A named bundle of permissions inside one guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildRole {
    pub id: Uuid,
    pub guild_id: u64,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    /// Seeded with the guild; cannot be deleted from the dashboard.
    pub is_system: bool,
    pub position: i32,
}

/// The three roles every guild starts with.
///
/// The slug is the stable identifier — a guild may rename "Admin" freely without
/// breaking the checks that reference it.
pub mod slugs {
    /// Full authority inside the guild.
    pub const ADMIN: &str = "admin";
    /// Day-to-day moderation and content management.
    pub const MODERATOR: &str = "moderator";
    /// Read-only access.
    pub const VIEWER: &str = "viewer";

    pub const SYSTEM_SLUGS: &[&str] = &[ADMIN, MODERATOR, VIEWER];
}

impl GuildRole {
    pub fn is_system_role(&self) -> bool {
        self.is_system
    }

    /// Whether this role is the guild's full-authority role.
    pub fn is_admin(&self) -> bool {
        self.slug == slugs::ADMIN
    }
}

/// What bootstrap seeds for each system role: the permission names it grants.
///
/// Expressed as data rather than code so the matrix can be reviewed at a glance,
/// and so a test can assert every name exists in the catalog.
pub fn default_role_permissions(slug: &str) -> &'static [&'static str] {
    use names::*;

    match slug {
        slugs::ADMIN => &[
            DASHBOARD_MANAGE,
            MEMBERS_READ,
            MEMBERS_WRITE,
            ECONOMY_READ,
            ECONOMY_WRITE,
            LEVELING_READ,
            LEVELING_WRITE,
            WELCOME_READ,
            WELCOME_WRITE,
            AUTOROLE_READ,
            AUTOROLE_WRITE,
            ROLES_READ,
            ROLES_WRITE,
        ],
        slugs::MODERATOR => &[
            MEMBERS_READ,
            MEMBERS_WRITE,
            ECONOMY_READ,
            LEVELING_READ,
            LEVELING_WRITE,
            WELCOME_READ,
            AUTOROLE_READ,
            ROLES_READ,
        ],
        slugs::VIEWER => &[
            MEMBERS_READ,
            ECONOMY_READ,
            LEVELING_READ,
            WELCOME_READ,
            AUTOROLE_READ,
            ROLES_READ,
        ],
        _ => &[],
    }
}

/// What a guild member is allowed to do, resolved once per request.
///
/// Handlers ask this, never the database: the answer is computed by a use case
/// and carried in the request extensions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionSet {
    pub guild_id: u64,
    permissions: Vec<String>,
}

impl PermissionSet {
    pub fn new(guild_id: u64, mut permissions: Vec<String>) -> Self {
        permissions.sort();
        permissions.dedup();
        Self {
            guild_id,
            permissions,
        }
    }

    pub fn allows(&self, permission: &str) -> bool {
        self.permissions.iter().any(|held| held == permission)
    }

    pub fn allows_all(&self, required: &[&str]) -> bool {
        required.iter().all(|permission| self.allows(permission))
    }

    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.permissions.iter().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.permissions.is_empty()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_seeded_permission_exists_in_the_catalog() {
        let catalog: Vec<&str> = names::CATALOG.iter().map(|(name, _, _)| *name).collect();

        for slug in slugs::SYSTEM_SLUGS {
            for permission in default_role_permissions(slug) {
                assert!(
                    catalog.contains(permission),
                    "role `{slug}` grants `{permission}`, which is not in the catalog"
                );
            }
        }
    }

    #[test]
    fn admin_holds_every_catalogued_permission() {
        let granted = default_role_permissions(slugs::ADMIN);
        for (name, _, _) in names::CATALOG {
            assert!(
                granted.contains(name),
                "admin is missing `{name}`; an admin that cannot do something is a support ticket"
            );
        }
    }

    #[test]
    fn permission_set_matches_exactly() {
        let set = PermissionSet::new(1, vec!["economy:read".into(), "economy:read".into()]);
        assert!(set.allows("economy:read"));
        assert!(!set.allows("economy:write"));
        // Duplicates are collapsed rather than counted twice.
        assert_eq!(set.iter().count(), 1);
    }
}
