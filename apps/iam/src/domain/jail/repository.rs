use std::future::Future;

use super::entity::{Imprisonment, JailConfig};
use crate::domain::errors::RepositoryError;

pub trait JailRepository: Send + Sync {
    /// The guild's jail configuration, or the defaults when it has never been set.
    ///
    /// Deliberately not an error: an unconfigured guild is the normal state of a
    /// fresh install, and every caller would otherwise have to treat "no row" as a
    /// fallible case for no reason.
    fn find_config(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<JailConfig, RepositoryError>> + Send;

    fn upsert_config(
        &self,
        config: JailConfig,
    ) -> impl Future<Output = Result<JailConfig, RepositoryError>> + Send;

    /// One member's imprisonment, if any.
    fn find(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<Option<Imprisonment>, RepositoryError>> + Send;

    /// Records a sentence, replacing any existing one for the member.
    ///
    /// Upsert rather than insert so re-jailing somebody who is already in serves the
    /// new sentence instead of failing on the primary key — a moderator extending a
    /// sentence should not have to release first.
    fn upsert(
        &self,
        imprisonment: Imprisonment,
    ) -> impl Future<Output = Result<Imprisonment, RepositoryError>> + Send;

    fn delete(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Sentences whose time is up as of `now`, oldest deadline first.
    ///
    /// Indefinite sentences (`expires_at IS NULL`) are excluded by construction:
    /// they end when a moderator says so, never on a timer.
    fn expired(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<Imprisonment>, RepositoryError>> + Send;

    /// Everyone currently jailed in a guild, newest first.
    ///
    /// Needed because a member can be freed by hand in Discord (the role removed
    /// without the bot being told), and the sweeper has to reconcile that rather
    /// than leave a row claiming they are still inside.
    fn list_for_guild(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<Vec<Imprisonment>, RepositoryError>> + Send;
}
