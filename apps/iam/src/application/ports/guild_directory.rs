//! Names for the Discord ids the configuration stores.
//!
//! Every feature in this crate persists Discord snowflakes — a reward's role, an
//! announcement channel, a panel button's role — because an id is what the bot
//! needs to act. An id is also useless to a human reading a dashboard, so the
//! presentation layer needs a way to turn one back into "#general" or "@Neophyte".
//!
//! That lookup has to happen here rather than in the browser: it needs the bot
//! token, and no dashboard visitor has authority to enumerate a guild's roles.
//!
//! The trait returns a whole-guild snapshot rather than a `name_of(id)` per id.
//! Six panels would otherwise make six round trips per render, and Discord rate
//! limits per route: one `GET /guilds/{id}/roles` plus one for channels is the
//! cheap shape, and the caller can index it however it likes.

use std::future::Future;
use thiserror::Error;

/// A Discord role, reduced to what a dashboard renders.
///
/// Deliberately not the full role object: permissions bitfields and icon hashes
/// are not shown anywhere, and mirroring Discord's model would mean breaking
/// whenever it gains a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRole {
    pub id: u64,
    pub name: String,
    /// 24-bit colour. `0` is Discord's "no colour set", not black.
    pub color: i32,
    pub position: i32,
    /// Displayed separately in the member list.
    pub hoist: bool,
    pub mentionable: bool,
    /// Owned by an integration, so it cannot be handed out by hand.
    pub managed: bool,
}

/// A Discord channel, reduced to what a dashboard renders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryChannel {
    pub id: u64,
    pub name: String,
    /// Discord's channel type: `0` text, `2` voice, `4` category, `5` news,
    /// `13` stage, `15` forum. Kept as the raw number so a new type Discord adds
    /// survives the round trip instead of being dropped as unknown.
    pub kind: i32,
    /// The category this channel sits under, when it has one.
    pub parent_id: Option<u64>,
}

/// One read of a guild's roles and channels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GuildDirectory {
    pub roles: Vec<DirectoryRole>,
    pub channels: Vec<DirectoryChannel>,
}

#[derive(Debug, Error)]
pub enum DirectoryError {
    /// No bot token, so nothing here can enumerate a guild.
    ///
    /// Distinct from [`Api`](Self::Api) because it is a configuration fact: an
    /// installation without a bot token cannot resolve names, which is a
    /// deployment gap rather than a rejection.
    #[error("this installation has no discord bot token configured")]
    NotConfigured,

    #[error("discord refused the request: {0}")]
    Api(String),

    #[error("discord is unreachable: {0}")]
    Transport(String),

    #[error("discord returned a response we could not read: {0}")]
    MalformedResponse(String),
}

/// Reads a guild's roles and channels.
///
/// Both the API (rendering a dashboard) and a future bot-side command want this,
/// so it is stated as a capability rather than as a concrete HTTP client.
pub trait GuildDirectorySource: Send + Sync {
    /// Whether a bot token is configured, so a caller can refuse with a message
    /// naming the missing half instead of surfacing a 401.
    fn is_configured(&self) -> bool;

    /// Reads every role and channel in the guild.
    ///
    /// Fails as a whole rather than returning a half-filled snapshot: a role list
    /// without its names is the empty answer this method exists to replace, and
    /// silently returning it would blank every label on the page.
    fn fetch(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<GuildDirectory, DirectoryError>> + Send;
}
