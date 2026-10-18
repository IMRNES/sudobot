use chrono::{DateTime, Utc};

/// A Discord guild the bot is installed in.
///
/// `id` is Discord's snowflake. There is no surrogate key: the snowflake is
/// stable, unique and already the value every other table references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guild {
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    pub owner_discord_id: Option<u64>,
    pub member_count: i32,
    pub bot_present: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A guild as first seen by the bot (join event, or first login for an
/// installation that predates the record).
#[derive(Debug, Clone)]
pub struct NewGuild {
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    pub owner_discord_id: Option<u64>,
    pub member_count: i32,
}

/// Partial update from either a Discord sync or a dashboard edit.
#[derive(Debug, Clone, Default)]
pub struct GuildPatch {
    pub name: Option<String>,
    pub icon_url: Option<String>,
    pub owner_discord_id: Option<u64>,
    pub member_count: Option<i32>,
    pub bot_present: Option<bool>,
}

impl GuildPatch {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.icon_url.is_none()
            && self.owner_discord_id.is_none()
            && self.member_count.is_none()
            && self.bot_present.is_none()
    }

    /// The patch the bot applies when it is removed from a guild.
    ///
    /// Explicit rather than `..Default::default()` so the intent is greppable.
    pub fn bot_left() -> Self {
        Self {
            bot_present: Some(false),
            ..Default::default()
        }
    }
}

/// The Discord permissions a guild administrator must hold to use the dashboard.
///
/// Mirrors Discord's bit flags for "Manage Server" and "Administrator". Stored as
/// a constant here so both the bot and the API agree on the threshold.
pub const DISCORD_MANAGE_GUILD: u64 = 0x20;
pub const DISCORD_ADMINISTRATOR: u64 = 0x8;

/// Whether a raw Discord permission bitfield grants guild administration.
pub fn has_manage_guild(permissions: u64) -> bool {
    permissions & (DISCORD_ADMINISTRATOR | DISCORD_MANAGE_GUILD) != 0
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn administrator_implies_manage() {
        assert!(has_manage_guild(DISCORD_ADMINISTRATOR));
    }

    #[test]
    fn plain_member_does_not_manage() {
        // send messages | read history
        assert!(!has_manage_guild(0x400 | 0x10000));
    }
}
