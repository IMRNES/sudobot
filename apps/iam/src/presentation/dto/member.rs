use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use super::snowflake;
use crate::domain::guild_member::GuildMemberWithRole;

/// A member of a guild, with the dashboard role they hold.
#[derive(Debug, Serialize)]
pub struct MemberResponse {
    pub user_id: Uuid,
    #[serde(with = "snowflake")]
    pub discord_id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
    pub guild_role_id: Uuid,
    pub role_slug: String,
    pub role_name: String,
    /// Discord's own permission bitfield, as a string for the same reason
    /// snowflakes are: it is a 64-bit value a JavaScript client would mangle.
    pub discord_permissions: String,
    pub is_owner: bool,
    pub synced_at: DateTime<Utc>,
}

/// Builds the response from the joined membership row.
///
/// The account fields come in separately because the repository joins the
/// membership to its role, not to the user — the dashboard's member list wants the
/// name, so the handler supplies it rather than the repository widening its join
/// for one screen.
pub fn member_response(
    member: &GuildMemberWithRole,
    account: Option<&crate::domain::user::User>,
    owner_discord_id: Option<u64>,
) -> MemberResponse {
    let discord_id = account.map(|user| user.discord_id).unwrap_or_default();

    MemberResponse {
        user_id: member.member.user_id,
        discord_id,
        username: account
            .map(|user| user.username.clone())
            .unwrap_or_else(|| "(unknown)".to_string()),
        global_name: account.and_then(|user| user.global_name.clone()),
        avatar_url: account.and_then(|user| user.avatar_url.clone()),
        guild_role_id: member.member.guild_role_id,
        role_slug: member.role.slug.clone(),
        role_name: member.role.name.clone(),
        discord_permissions: member.member.discord_permissions.to_string(),
        is_owner: owner_discord_id == Some(discord_id) && discord_id != 0,
        synced_at: member.member.synced_at,
    }
}

/// Changes which dashboard role a member holds.
#[derive(Debug, Deserialize, ZodSchema)]
pub struct UpdateMemberRoleRequest {
    /// The guild role's internal id, not the Discord role id.
    #[zod(min_length(36), max_length(36))]
    pub guild_role_id: String,
}

crate::validated_request!(UpdateMemberRoleRequest);
