use chrono::{DateTime, Utc};

/// How a prisoner gets out.
///
/// The sentence is a *duration*, and serving the duration is what frees the member.
/// Storing the reason matters because "why am I in here" is the first thing anyone
/// asks, and a moderator looking at the row should not have to remember.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imprisonment {
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// Who put them in, for the audit trail.
    pub jailed_by: u64,
    /// Why they were put in. Free text, shown to the prisoner.
    pub reason: String,
    /// When the sentence ends. `None` means indefinite — an admin must release them.
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl Imprisonment {
    /// Whether the sentence has run out at `now`.
    ///
    /// An indefinite sentence never expires on its own: `None` is not "expired
    /// immediately" and must not be read as zero time remaining.
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        match self.expires_at {
            Some(deadline) => deadline <= now,
            None => false,
        }
    }

    /// Seconds left, or `None` for an indefinite sentence.
    pub fn remaining_seconds(&self, now: DateTime<Utc>) -> Option<i64> {
        self.expires_at
            .map(|deadline| (deadline - now).num_seconds().max(0))
    }
}

/// A guild's jail configuration.
///
/// Two ids, both explicit. `jail_role_id` is the role whose overwrites deny every
/// channel; `jail_channel_id` is the one channel it may still see. Neither is
/// guessed: a server can call its role "prisoner", "jailed", "muted" or anything
/// else, and picking the channel by name would break the moment someone renames it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JailConfig {
    pub guild_id: u64,
    /// The role applied to a prisoner. Removing it is what frees them.
    pub jail_role_id: Option<u64>,
    /// The only channel a prisoner keeps access to.
    pub jail_channel_id: Option<u64>,
    /// Applied when a moderator does not name a duration. `None` means indefinite.
    pub default_sentence_seconds: Option<i64>,
    /// What a prisoner may pay to walk out early. `None` disables bail.
    ///
    /// A price rather than a yes/no: bail is the one escape route that does not
    /// need a moderator awake, and charging for it keeps it from being a free
    /// instant release that makes the whole jail pointless.
    pub bail_amount: Option<i64>,
    pub updated_at: DateTime<Utc>,
}

impl JailConfig {
    pub fn default_for(guild_id: u64) -> Self {
        Self {
            guild_id,
            jail_role_id: None,
            jail_channel_id: None,
            default_sentence_seconds: None,
            bail_amount: None,
            updated_at: Utc::now(),
        }
    }

    /// Whether a jail can actually be applied.
    ///
    /// Both ids are required: a role with no channel would lock a member out of the
    /// entire server including the place they are meant to sit, which is a
    /// configuration mistake rather than a harsh sentence.
    pub fn is_configured(&self) -> bool {
        self.jail_role_id.is_some() && self.jail_channel_id.is_some()
    }
}
