use std::future::Future;
use uuid::Uuid;

use super::entity::{LevelProfile, LevelReward, LevelingConfig};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

pub trait LevelingRepository: Send + Sync {
    /// The guild's configuration, or the defaults when it has never been set.
    fn find_config(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<LevelingConfig, RepositoryError>> + Send;

    fn upsert_config(
        &self,
        config: LevelingConfig,
    ) -> impl Future<Output = Result<LevelingConfig, RepositoryError>> + Send;

    fn find_profile(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<Option<LevelProfile>, RepositoryError>> + Send;

    fn ensure_profile(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<LevelProfile, RepositoryError>> + Send;

    /// Adds XP and records the message, in one transaction.
    ///
    /// Returns the profile as it stands after the write so the caller can compare
    /// the level before and after without a second read.
    fn add_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        xp: i64,
        level: i32,
        counted_message: bool,
    ) -> impl Future<Output = Result<LevelProfile, RepositoryError>> + Send;

    /// Adds voice time and the XP it earned.
    fn add_voice_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        seconds: i64,
        xp: i64,
        level: i32,
    ) -> impl Future<Output = Result<LevelProfile, RepositoryError>> + Send;

    /// Applies an administrative XP change, which may be negative.
    fn set_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        xp: i64,
        level: i32,
    ) -> impl Future<Output = Result<LevelProfile, RepositoryError>> + Send;

    fn list_leaderboard(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<LevelProfile>, RepositoryError>> + Send;

    /// Every profile in the guild.
    ///
    /// Unpaginated on purpose, and used only by reconciliation: placing an entire
    /// guild on the ladder they have already climbed needs all of them, and paging
    /// through a leaderboard to find them would recompute ranks the caller never
    /// reads. Kept off the request path — a guild's member count is bounded at
    /// Discord's ceiling, but this is still not something a handler should call.
    fn list_profiles(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<Vec<LevelProfile>, RepositoryError>> + Send;

    /// Records the name the bot saw for a member, so a leaderboard can show it.
    ///
    /// Kept on this trait because it is the same per-member row the XP grant already
    /// touches on every message, and keeping the two writes on one port means a
    /// caller cannot record progress without recording who made it.
    fn record_member_name(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        display_name: &str,
        is_bot: bool,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// The names cached for these members, keyed by discord id.
    ///
    /// A member missing from the map has no name on record — deliberately not an
    /// error, and deliberately not a placeholder.
    fn member_names(
        &self,
        guild_id: u64,
        discord_user_ids: &[u64],
    ) -> impl Future<Output = Result<std::collections::HashMap<u64, String>, RepositoryError>> + Send;

    /// The member's rank by XP, 1-based.
    fn rank_of(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;

    fn list_rewards(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<Vec<LevelReward>, RepositoryError>> + Send;

    fn create_reward(
        &self,
        reward: LevelReward,
    ) -> impl Future<Output = Result<LevelReward, RepositoryError>> + Send;

    fn delete_reward(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Reward roles a member at `level` should hold, ordered by level.
    fn rewards_reached(
        &self,
        guild_id: u64,
        level: i32,
    ) -> impl Future<Output = Result<Vec<LevelReward>, RepositoryError>> + Send;
}
