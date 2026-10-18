use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use super::snowflake;
use crate::domain::autorole::AutoRole;

#[derive(Debug, Serialize)]
pub struct AutoRoleResponse {
    pub id: Uuid,
    #[serde(with = "snowflake")]
    pub role_id: u64,
    /// `join`, `delay` or `human`.
    pub trigger: String,
    pub delay_seconds: i32,
    pub applies_to_bots: bool,
    pub is_active: bool,
}

impl From<AutoRole> for AutoRoleResponse {
    fn from(rule: AutoRole) -> Self {
        Self {
            id: rule.id,
            role_id: rule.role_id,
            trigger: rule.trigger.as_str().to_string(),
            delay_seconds: rule.delay_seconds,
            applies_to_bots: rule.applies_to_bots,
            is_active: rule.is_active,
        }
    }
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct CreateAutoRoleRequest {
    #[zod(min_length(1), max_length(32))]
    pub role_id: String,
    #[zod(min_length(1), max_length(16))]
    pub trigger: String,
    #[serde(default)]
    pub delay_seconds: i32,
    #[serde(default)]
    pub applies_to_bots: bool,
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct UpdateAutoRoleRequest {
    #[zod(max_length(16))]
    pub trigger: Option<String>,
    pub delay_seconds: Option<i32>,
    pub applies_to_bots: Option<bool>,
    pub is_active: Option<bool>,
}

crate::validated_request!(CreateAutoRoleRequest);

crate::validated_request!(UpdateAutoRoleRequest);
