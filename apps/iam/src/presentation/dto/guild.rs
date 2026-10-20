//! Guild request and response shapes.
//!
//! The account and guild summaries the login returns live in `auth.rs`, next to
//! the flow that produces them; this module holds the guild-scoped CRUD shapes.

use serde::Serialize;

use super::{optional_snowflake, snowflake};
use crate::application::guild::GuildDirectoryView;
use crate::application::ports::guild_directory::{DirectoryChannel, DirectoryRole};

pub use super::auth::{GuildResponse, GuildSummaryResponse, MeResponse, UpdateGuildRequest};

/// The names behind the snowflakes a guild's configuration stores.
///
/// A dashboard that shows `1460146703637287115` where a role name belongs has
/// given the reader nothing to decide with. This is the endpoint that lets every
/// picker speak in names, without the API keeping its own copy of them — Discord
/// owns the list, so a rename there shows up on the next load rather than never.
#[derive(Debug, Serialize)]
pub struct GuildDirectoryResponse {
    pub roles: Vec<DirectoryRoleResponse>,
    pub channels: Vec<DirectoryChannelResponse>,
}

#[derive(Debug, Serialize)]
pub struct DirectoryRoleResponse {
    #[serde(with = "snowflake")]
    pub id: u64,
    pub name: String,
    /// 24-bit colour, `0` when Discord has none set for the role.
    pub color: i32,
    pub position: i32,
    pub hoist: bool,
    pub mentionable: bool,
    /// True for bot and integration roles, which cannot be handed out by hand.
    /// The pickers mark these so an admin is not left wondering why nothing happens.
    pub managed: bool,
}

#[derive(Debug, Serialize)]
pub struct DirectoryChannelResponse {
    #[serde(with = "snowflake")]
    pub id: u64,
    pub name: String,
    /// Discord's channel type: `0` text, `2` voice, `4` category, `5` news,
    /// `13` stage, `15` forum.
    pub kind: i32,
    #[serde(with = "optional_snowflake")]
    pub parent_id: Option<u64>,
}

impl From<GuildDirectoryView> for GuildDirectoryResponse {
    fn from(view: GuildDirectoryView) -> Self {
        Self {
            roles: view.roles.into_iter().map(Into::into).collect(),
            channels: view.channels.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<DirectoryRole> for DirectoryRoleResponse {
    fn from(role: DirectoryRole) -> Self {
        Self {
            id: role.id,
            name: role.name,
            color: role.color,
            position: role.position,
            hoist: role.hoist,
            mentionable: role.mentionable,
            managed: role.managed,
        }
    }
}

impl From<DirectoryChannel> for DirectoryChannelResponse {
    fn from(channel: DirectoryChannel) -> Self {
        Self {
            id: channel.id,
            name: channel.name,
            kind: channel.kind,
            parent_id: channel.parent_id,
        }
    }
}
