//! Applying a jail to Discord, from the API side.
//!
//! The policy — which channels, which masks — lives in [`crate::domain::jail`] and is
//! shared. This port only carries it to Discord, because the gateway is a separate
//! process from the bot and cannot borrow the bot's serenity client. Without it a
//! jail configured from the dashboard would be a database row that never seals
//! anything, which is worse than no feature: it looks like it worked.
//!
//! Deliberately narrow. It adds and removes one role and writes the overwrites; it
//! does not read guild state, because everything it needs was already decided by the
//! domain.

use std::future::Future;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EnforcerError {
    /// No bot token, so nothing here can touch a guild.
    #[error("this installation has no discord bot token configured")]
    NotConfigured,

    #[error("discord refused the request: {0}")]
    Api(String),

    #[error("discord is unreachable: {0}")]
    Transport(String),
}

/// What a seal pass managed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SealReport {
    pub written: usize,
    pub failed: usize,
}

impl SealReport {
    pub fn is_clean(&self) -> bool {
        self.failed == 0
    }
}

/// Writes the jail's permission overwrites and moves members in and out.
pub trait JailEnforcer: Send + Sync {
    fn is_configured(&self) -> bool;

    /// Writes one overwrite per channel: deny everywhere, allow in the cell.
    ///
    /// Idempotent, and meant to be re-run whenever the config or the channel list
    /// changes — a channel created after the seal is otherwise fully readable to
    /// anyone already inside.
    fn seal(
        &self,
        guild_id: u64,
        jail_role_id: u64,
        prison_channel_id: Option<u64>,
    ) -> impl Future<Output = Result<SealReport, EnforcerError>> + Send;

    /// Removes a role's overwrites from every channel.
    ///
    /// Used when the jail moves to a different role, so the old one does not keep a
    /// `View Channel` grant in every channel's permission list forever.
    fn unseal(
        &self,
        guild_id: u64,
        jail_role_id: u64,
    ) -> impl Future<Output = Result<usize, EnforcerError>> + Send;

    /// Grants the jail role.
    ///
    /// Treats "already has it" as success: a repeated sentence is the ordinary
    /// retry, not an error.
    fn put_in_cell(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        jail_role_id: u64,
    ) -> impl Future<Output = Result<(), EnforcerError>> + Send;

    /// Removes the jail role.
    ///
    /// Treats "does not have it" as success — the caller wanted them free, and they
    /// are.
    fn let_out(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        jail_role_id: u64,
    ) -> impl Future<Output = Result<(), EnforcerError>> + Send;
}
