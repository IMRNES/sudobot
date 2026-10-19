use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use super::{snowflake, snowflake_list};
use crate::application::leveling::LeaderboardRow;
use crate::domain::leveling::{LevelReward, LevelingConfig};
use crate::presentation::extract::ValidatedRequest;

#[derive(Debug, Serialize)]
pub struct LevelingConfigResponse {
    pub enabled: bool,
    pub xp_min: i32,
    pub xp_max: i32,
    pub cooldown_secs: i32,
    pub voice_xp_multiplier: f64,
    /// `mrn`, `linear` or `flat`.
    pub curve: String,
    #[serde(with = "crate::presentation::dto::optional_snowflake")]
    pub announce_channel_id: Option<u64>,
    pub stack_roles: bool,
    /// The role the ladder sits above; null means just above `@everyone`.
    #[serde(with = "crate::presentation::dto::optional_snowflake")]
    pub ladder_base_role_id: Option<u64>,
    #[serde(with = "snowflake_list")]
    pub ignored_channel_ids: Vec<u64>,
    #[serde(with = "snowflake_list")]
    pub ignored_role_ids: Vec<u64>,
}

impl From<LevelingConfig> for LevelingConfigResponse {
    fn from(config: LevelingConfig) -> Self {
        Self {
            enabled: config.enabled,
            xp_min: config.xp_min,
            xp_max: config.xp_max,
            cooldown_secs: config.cooldown_secs,
            voice_xp_multiplier: config.voice_xp_multiplier,
            curve: config.curve.as_str().to_string(),
            announce_channel_id: config.announce_channel_id,
            stack_roles: config.stack_roles,
            ladder_base_role_id: config.ladder_base_role_id,
            ignored_channel_ids: config.ignored_channel_ids,
            ignored_role_ids: config.ignored_role_ids,
        }
    }
}

/// A partial update to the leveling configuration.
///
/// Hand-validated for the same reason as `UpdateShopItemRequest`: the derive cannot
/// handle the two-level option that distinguishes "leave the channel alone" from
/// "clear it".
#[derive(Debug, Deserialize)]
pub struct UpdateLevelingConfigRequest {
    pub enabled: Option<bool>,
    pub xp_min: Option<i32>,
    pub xp_max: Option<i32>,
    pub cooldown_secs: Option<i32>,
    pub voice_xp_multiplier: Option<f64>,
    pub curve: Option<String>,
    /// `null` clears the announcement channel.
    #[serde(default, deserialize_with = "double_option")]
    pub announce_channel_id: Option<Option<String>>,
    pub stack_roles: Option<bool>,
    /// `null` places the ladder directly above `@everyone` again.
    #[serde(default, deserialize_with = "double_option")]
    pub ladder_base_role_id: Option<Option<String>>,
    pub ignored_channel_ids: Option<Vec<String>>,
    pub ignored_role_ids: Option<Vec<String>>,
}

/// Distinguishes "key absent" from "key present and null".
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl ValidatedRequest for UpdateLevelingConfigRequest {
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
        let mut issues = Vec::new();

        if let Some(curve) = value.get("curve").and_then(serde_json::Value::as_str)
            && !matches!(curve, "mrn" | "linear" | "flat")
        {
            issues.push(("curve".into(), "must be one of mrn, linear or flat".into()));
        }

        // The cross-field rules (min <= max, bounds, cooldown range) live in the
        // domain and are applied by the use case to the MERGED config, because a
        // patch that only sets `xp_min` can still invert an existing `xp_max`.
        // A multiplier above 100x would let one message grant a whole level, and a
        // negative one would subtract XP on every message.
        const MAX_VOICE_MULTIPLIER: f64 = 100.0;
        if let Some(number) = value
            .get("voice_xp_multiplier")
            .and_then(serde_json::Value::as_f64)
            && (!number.is_finite() || !(0.0..=MAX_VOICE_MULTIPLIER).contains(&number))
        {
            issues.push((
                "voice_xp_multiplier".into(),
                format!("must be between 0 and {MAX_VOICE_MULTIPLIER}"),
            ));
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LeaderboardRowResponse {
    pub rank: u64,
    #[serde(with = "snowflake")]
    pub discord_id: u64,
    /// The member's name, or null when the bot has never recorded one.
    pub display_name: Option<String>,
    pub level: i32,
    pub xp: i64,
    pub total_xp: i64,
    pub message_count: i64,
    /// Share of the way to the next level, for a progress bar.
    pub progress: f64,
}

impl From<LeaderboardRow> for LeaderboardRowResponse {
    fn from(row: LeaderboardRow) -> Self {
        Self {
            rank: row.rank,
            discord_id: row.discord_user_id,
            display_name: row.display_name,
            level: row.level,
            xp: row.xp,
            total_xp: row.total_xp,
            message_count: row.message_count,
            progress: row.progress,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LevelRewardResponse {
    pub id: Uuid,
    pub level: i32,
    #[serde(with = "snowflake")]
    pub role_id: u64,
    pub keep_on_demote: bool,
}

impl From<LevelReward> for LevelRewardResponse {
    fn from(reward: LevelReward) -> Self {
        Self {
            id: reward.id,
            level: reward.level,
            role_id: reward.role_id,
            keep_on_demote: reward.keep_on_demote,
        }
    }
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct CreateLevelRewardRequest {
    pub level: i32,
    #[zod(min_length(1), max_length(32))]
    pub role_id: String,
    #[serde(default = "default_true")]
    pub keep_on_demote: bool,
}

fn default_true() -> bool {
    true
}

crate::validated_request!(CreateLevelRewardRequest);
