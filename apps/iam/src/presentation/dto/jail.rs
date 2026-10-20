//! The jail's HTTP surface.
//!
//! Split config from sentences because they are written by different people at
//! different times: an admin sets the jail up once, moderators add and drop
//! sentences constantly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{optional_snowflake, snowflake, triple_snowflake};
use crate::domain::jail::{Imprisonment, JailConfig};
use crate::presentation::extract::ValidatedRequest;

/// The guild's jail, as the dashboard sees it.
#[derive(Debug, Serialize)]
pub struct JailConfigResponse {
    #[serde(with = "snowflake")]
    pub guild_id: u64,

    /// The role that denies every channel. `None` means the jail is not set up.
    #[serde(with = "optional_snowflake")]
    pub jail_role_id: Option<u64>,

    /// The one channel a prisoner can still see.
    #[serde(with = "optional_snowflake")]
    pub jail_channel_id: Option<u64>,

    /// What `/jail add` uses when no duration is given.
    pub default_sentence_seconds: Option<i64>,

    /// What a prisoner may pay to leave early. `None` disables bail entirely.
    pub bail_amount: Option<i64>,
}

impl From<JailConfig> for JailConfigResponse {
    fn from(config: JailConfig) -> Self {
        Self {
            guild_id: config.guild_id,
            jail_role_id: config.jail_role_id,
            jail_channel_id: config.jail_channel_id,
            default_sentence_seconds: config.default_sentence_seconds,
            bail_amount: config.bail_amount,
        }
    }
}

/// A partial update.
///
/// Every field is doubly optional so the dashboard can tell "leave this alone"
/// (absent) from "clear it" (explicit null). Collapsing the two would make it
/// impossible to switch bail off without also wiping the sentence default. The derive
/// cannot express that shape, which is why this type validates by hand.
#[derive(Debug, Deserialize)]
pub struct UpdateJailConfigRequest {
    /// Absent = unchanged, `null` = no role (jail disabled).
    #[serde(default, with = "triple_snowflake")]
    pub jail_role_id: Option<Option<u64>>,

    #[serde(default, with = "triple_snowflake")]
    pub jail_channel_id: Option<Option<u64>>,

    pub default_sentence_seconds: Option<Option<i64>>,
    pub bail_amount: Option<Option<i64>>,
}

/// One sentence, as listed in the dashboard.
#[derive(Debug, Serialize)]
pub struct SentenceResponse {
    #[serde(with = "snowflake")]
    pub discord_user_id: u64,

    #[serde(with = "snowflake")]
    pub jailed_by: u64,

    pub reason: String,

    /// `None` is an indefinite sentence: only a moderator or bail ends it.
    pub expires_at: Option<DateTime<Utc>>,

    pub created_at: DateTime<Utc>,

    /// Seconds left, computed server-side so the dashboard does not need the
    /// operator's clock to be right. `None` for an indefinite sentence.
    pub remaining_seconds: Option<i64>,
}

impl SentenceResponse {
    pub fn new(sentence: Imprisonment, now: DateTime<Utc>) -> Self {
        // Computed before the struct takes ownership of `reason`; asking afterwards
        // would need the partly-moved value back.
        let remaining_seconds = sentence.remaining_seconds(now);

        Self {
            discord_user_id: sentence.discord_user_id,
            jailed_by: sentence.jailed_by,
            reason: sentence.reason,
            expires_at: sentence.expires_at,
            created_at: sentence.created_at,
            remaining_seconds,
        }
    }
}

/// Jail a member from the dashboard.
#[derive(Debug, Deserialize)]
pub struct JailMemberRequest {
    #[serde(with = "snowflake")]
    pub discord_user_id: u64,

    #[serde(default)]
    pub reason: String,

    /// Absent uses the guild's default; `0` means indefinite.
    pub duration_seconds: Option<i64>,
}

/// What a release did, so the caller can tell the two cases apart.
#[derive(Debug, Serialize)]
pub struct ReleaseResponse {
    /// False when the member had no live sentence — already free.
    pub was_imprisoned: bool,
}

impl ValidatedRequest for UpdateJailConfigRequest {
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
        let mut issues = Vec::new();

        // Zero is rejected rather than treated as "clear": the two are different
        // intents, and a dashboard sending 0 almost certainly meant "no limit" and
        // would silently get "use the default" instead.
        if let Some(seconds) = value
            .get("default_sentence_seconds")
            .and_then(serde_json::Value::as_i64)
            && seconds <= 0
        {
            issues.push((
                "default_sentence_seconds".into(),
                "must be greater than zero; send null to require an explicit duration".into(),
            ));
        }

        if let Some(amount) = value.get("bail_amount").and_then(serde_json::Value::as_i64)
            && amount <= 0
        {
            issues.push((
                "bail_amount".into(),
                "must be greater than zero; send null to disable bail".into(),
            ));
        }

        // A jail whose role is also its cell seals the prisoner out of the only
        // channel they were meant to keep.
        if let (Some(role), Some(channel)) = (
            value
                .get("jail_role_id")
                .and_then(serde_json::Value::as_str),
            value
                .get("jail_channel_id")
                .and_then(serde_json::Value::as_str),
        ) && role == channel
        {
            issues.push((
                "jail_channel_id".into(),
                "must be a channel, not the jail role".into(),
            ));
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

impl ValidatedRequest for JailMemberRequest {
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
        let mut issues = Vec::new();

        if let Some(reason) = value.get("reason").and_then(serde_json::Value::as_str)
            && reason.chars().count() > 500
        {
            issues.push(("reason".into(), "must be at most 500 characters".into()));
        }

        // Negative durations are meaningless; 0 is the explicit "indefinite" and is
        // allowed, which is why this is not simply a `> 0` check.
        if let Some(seconds) = value
            .get("duration_seconds")
            .and_then(serde_json::Value::as_i64)
            && seconds < 0
        {
            issues.push((
                "duration_seconds".into(),
                "must not be negative; use 0 for an indefinite sentence".into(),
            ));
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}
