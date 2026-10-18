use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A dashboard account, created on first successful Discord login.
///
/// Plain struct: no ORM attributes, no serde. The shape is the business's, and
/// the adapters translate to and from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: Uuid,
    pub discord_id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub is_super_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    /// The name to show in the dashboard: Discord's display name when set,
    /// otherwise the unique username.
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }

    /// Whether this account may operate outside any single guild.
    pub fn is_platform_admin(&self) -> bool {
        self.is_super_admin
    }
}

/// The data needed to create an account.
///
/// Separate from [`User`] because the id and timestamps are decisions the use
/// case makes, not facts about a new user.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub id: Uuid,
    pub discord_id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
}

/// A partial update. `None` means "leave this field alone".
///
/// The distinction matters for `avatar_url`: `None` is "no change", and clearing
/// an avatar is expressed by the Discord payload itself, which always carries the
/// current hash. Modelling it as `Option<Option<String>>` here would add a layer
/// of nesting for a case the sync never produces.
#[derive(Debug, Clone, Default)]
pub struct UserPatch {
    pub username: Option<String>,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
}

impl UserPatch {
    /// True when the patch would write nothing.
    ///
    /// Repositories use this to skip a pointless UPDATE, and use cases use it to
    /// avoid publishing an event for a no-op.
    pub fn is_empty(&self) -> bool {
        self.username.is_none()
            && self.global_name.is_none()
            && self.avatar_url.is_none()
            && self.email.is_none()
    }
}
