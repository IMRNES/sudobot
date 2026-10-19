use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use super::{optional_snowflake, snowflake};
use crate::domain::welcome::{
    ALLOWED_PLACEHOLDERS, RolePanel, RolePanelOption, RolePanelWithOptions, WelcomeConfig,
};
use crate::presentation::extract::ValidatedRequest;

#[derive(Debug, Serialize)]
pub struct WelcomeConfigResponse {
    pub welcome_enabled: bool,
    #[serde(with = "optional_snowflake")]
    pub welcome_channel_id: Option<u64>,
    pub welcome_message: Option<String>,
    pub welcome_embed_color: Option<i32>,
    pub welcome_dm_enabled: bool,
    /// The shared embed card, echoed so the dashboard can edit it.
    pub embed_title: Option<String>,
    pub embed_description: Option<String>,
    pub embed_author: Option<String>,
    pub embed_image_url: Option<String>,
    pub embed_thumbnail_url: Option<String>,
    pub goodbye_enabled: bool,
    #[serde(with = "optional_snowflake")]
    pub goodbye_channel_id: Option<u64>,
    pub goodbye_message: Option<String>,
    pub goodbye_embed_color: Option<i32>,
    /// Echoed so the dashboard's template editor can show the hint without
    /// hard-coding a list that must then be kept in step with the server.
    pub allowed_placeholders: Vec<String>,
}

impl From<WelcomeConfig> for WelcomeConfigResponse {
    fn from(config: WelcomeConfig) -> Self {
        Self {
            welcome_enabled: config.welcome_enabled,
            welcome_channel_id: config.welcome_channel_id,
            welcome_message: config.welcome_message,
            welcome_embed_color: config.welcome_embed_color,
            welcome_dm_enabled: config.welcome_dm_enabled,
            embed_title: config.embed_title,
            embed_description: config.embed_description,
            embed_author: config.embed_author,
            embed_image_url: config.embed_image_url,
            embed_thumbnail_url: config.embed_thumbnail_url,
            goodbye_enabled: config.goodbye_enabled,
            goodbye_channel_id: config.goodbye_channel_id,
            goodbye_message: config.goodbye_message,
            goodbye_embed_color: config.goodbye_embed_color,
            allowed_placeholders: ALLOWED_PLACEHOLDERS
                .iter()
                .map(|placeholder| placeholder.to_string())
                .collect(),
        }
    }
}

/// A partial update. Every field is doubly optional: absent leaves the value alone,
/// `null` clears it. A channel cannot be "set to nothing" any other way.
///
/// Hand-validated: `zod-rs`'s derive unwraps one `Option` and then requires a
/// `schema()` on the inner type, so `Option<Option<T>>` cannot be derived.
#[derive(Debug, Deserialize)]
pub struct UpdateWelcomeConfigRequest {
    pub welcome_enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub welcome_channel_id: Option<Option<String>>,
    pub welcome_message: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub welcome_embed_color: Option<Option<i32>>,
    pub welcome_dm_enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub embed_title: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub embed_description: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub embed_author: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub embed_image_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub embed_thumbnail_url: Option<Option<String>>,
    pub goodbye_enabled: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub goodbye_channel_id: Option<Option<String>>,
    pub goodbye_message: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub goodbye_embed_color: Option<Option<i32>>,
}

fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl ValidatedRequest for UpdateWelcomeConfigRequest {
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
        let mut issues = Vec::new();

        for field in ["welcome_message", "goodbye_message"] {
            let Some(text) = value.get(field).and_then(serde_json::Value::as_str) else {
                continue;
            };
            let length = text.chars().count();
            if length == 0 {
                issues.push((field.into(), "must not be empty".into()));
            } else if length > 2000 {
                issues.push((field.into(), "must be at most 2000 characters".into()));
            }
        }

        for field in ["welcome_embed_color", "goodbye_embed_color"] {
            if let Some(colour) = value.get(field).and_then(serde_json::Value::as_i64)
                && !(0..=0xFF_FFFF).contains(&colour)
            {
                issues.push((field.into(), "must be between 0 and 16777215".into()));
            }
        }

        // Placeholder validation happens in the use case, against the merged
        // config: it needs `validate_template`, which is domain knowledge.
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PanelOptionResponse {
    pub id: Uuid,
    pub label: String,
    pub emoji: Option<String>,
    #[serde(with = "snowflake")]
    pub role_id: u64,
    pub style: i16,
    pub position: i32,
    pub behavior: String,
}

impl From<RolePanelOption> for PanelOptionResponse {
    fn from(option: RolePanelOption) -> Self {
        Self {
            id: option.id,
            label: option.label,
            emoji: option.emoji,
            role_id: option.role_id,
            style: option.style,
            position: option.position,
            behavior: option.behavior.as_str().to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PanelResponse {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(with = "optional_snowflake")]
    pub channel_id: Option<u64>,
    #[serde(with = "optional_snowflake")]
    pub message_id: Option<u64>,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub options: Vec<PanelOptionResponse>,
}

impl From<RolePanelWithOptions> for PanelResponse {
    fn from(panel: RolePanelWithOptions) -> Self {
        Self {
            id: panel.panel.id,
            slug: panel.panel.slug,
            title: panel.panel.title,
            description: panel.panel.description,
            channel_id: panel.panel.channel_id,
            message_id: panel.panel.message_id,
            is_published: panel.panel.is_published,
            created_at: panel.panel.created_at,
            updated_at: panel.panel.updated_at,
            options: panel.options.into_iter().map(Into::into).collect(),
        }
    }
}

/// A panel row in the listing, without its options.
#[derive(Debug, Serialize)]
pub struct PanelSummaryResponse {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    #[serde(with = "optional_snowflake")]
    pub channel_id: Option<u64>,
    pub is_published: bool,
    pub option_count: usize,
}

impl PanelSummaryResponse {
    pub fn new(panel: RolePanel, option_count: usize) -> Self {
        Self {
            id: panel.id,
            slug: panel.slug,
            title: panel.title,
            channel_id: panel.channel_id,
            is_published: panel.is_published,
            option_count,
        }
    }
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct UpsertPanelRequest {
    #[zod(min_length(1), max_length(48))]
    pub slug: String,
    #[zod(min_length(1), max_length(128))]
    pub title: String,
    #[zod(max_length(1000))]
    pub description: Option<String>,
    pub channel_id: Option<String>,
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct AddPanelOptionRequest {
    #[zod(min_length(1), max_length(80))]
    pub label: String,
    #[zod(max_length(64))]
    pub emoji: Option<String>,
    #[zod(min_length(1), max_length(32))]
    pub role_id: String,
    pub style: Option<i16>,
    /// `toggle`, `add`, `remove` or `exclusive`.
    pub behavior: Option<String>,
}

/// Where to post a role panel.
///
/// One field, because posting is the API's job: the bot token lives here, so this
/// asks for the destination and nothing else. `message_id` used to be required,
/// which made the endpoint a record of somebody else's work and never a publish.
#[derive(Debug, Deserialize, ZodSchema)]
pub struct PublishPanelRequest {
    #[zod(min_length(1), max_length(32))]
    pub channel_id: String,
}

crate::validated_request!(UpsertPanelRequest);

crate::validated_request!(AddPanelOptionRequest);

crate::validated_request!(PublishPanelRequest);
