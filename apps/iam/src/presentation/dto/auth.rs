use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use super::{optional_snowflake, snowflake};
use crate::application::auth::GuildView;
use crate::domain::user::User;

/// The signed-in account, as the dashboard sees it.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    #[serde(with = "snowflake")]
    pub discord_id: u64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_super_admin: bool,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            discord_id: user.discord_id,
            username: user.username,
            global_name: user.global_name,
            avatar_url: user.avatar_url,
            is_super_admin: user.is_super_admin,
        }
    }
}

/// One guild in the caller's guild picker.
///
/// Carries the caller's role and resolved permissions so the dashboard can gate
/// controls from the login response alone, without a request per guild.
#[derive(Debug, Serialize)]
pub struct GuildSummaryResponse {
    #[serde(with = "snowflake")]
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    pub member_count: i32,
    pub bot_present: bool,
    pub role_slug: String,
    pub role_name: String,
    pub permissions: Vec<String>,
}

impl From<GuildView> for GuildSummaryResponse {
    fn from(view: GuildView) -> Self {
        Self {
            id: view.id,
            name: view.name,
            icon_url: view.icon_url,
            member_count: view.member_count,
            bot_present: view.bot_present,
            role_slug: view.role_slug,
            role_name: view.role_name,
            permissions: view.permissions,
        }
    }
}

/// Everything the dashboard needs right after signing in.
#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub user: UserResponse,
    pub guilds: Vec<GuildSummaryResponse>,
}

#[derive(Debug, Serialize)]
pub struct AuthorizeResponse {
    pub authorize_url: String,
    pub state: String,
}

/// The OAuth callback, forwarded by the browser.
#[derive(Debug, Deserialize, ZodSchema)]
pub struct CallbackRequest {
    #[zod(min_length(1), max_length(512))]
    pub code: String,
    #[zod(min_length(1), max_length(128))]
    pub state: String,
}

/// A completed login.
///
/// `guilds` carries the one guild this dashboard serves, so the client renders the
/// picker from the login response alone. There is no longer a second call to import
/// guilds: the guild is configuration, and the whitelist gate has already run.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub user: UserResponse,
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds until the access token expires.
    pub expires_in: i64,
    pub guilds: Vec<GuildSummaryResponse>,
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct RefreshRequest {
    #[zod(min_length(1), max_length(512))]
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct LogoutRequest {
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub everywhere: bool,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub revoked: u64,
}

/// A guild row in the platform-admin listing.
#[derive(Debug, Serialize)]
pub struct GuildResponse {
    #[serde(with = "snowflake")]
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    #[serde(with = "optional_snowflake")]
    pub owner_discord_id: Option<u64>,
    pub member_count: i32,
    pub bot_present: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<crate::domain::guild::Guild> for GuildResponse {
    fn from(guild: crate::domain::guild::Guild) -> Self {
        Self {
            id: guild.id,
            name: guild.name,
            icon_url: guild.icon_url,
            owner_discord_id: guild.owner_discord_id,
            member_count: guild.member_count,
            bot_present: guild.bot_present,
            created_at: guild.created_at,
            updated_at: guild.updated_at,
        }
    }
}

/// Renames a guild from the dashboard.
#[derive(Debug, Deserialize, ZodSchema)]
pub struct UpdateGuildRequest {
    #[zod(min_length(1), max_length(100))]
    pub name: Option<String>,
}

/// The health check's body.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub database: &'static str,
    pub version: &'static str,
}

crate::validated_request!(CallbackRequest);

crate::validated_request!(RefreshRequest);

crate::validated_request!(LogoutRequest);

crate::validated_request!(UpdateGuildRequest);
