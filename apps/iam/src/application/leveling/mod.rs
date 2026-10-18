//! XP, levels, reward roles and the per-guild settings that govern them.
//!
//! The curve lives in the domain, and so does every rule about a profile: whether
//! it is on cooldown, how far along it is. What happens here is the conversation
//! around those rules — read the config, decide whether this event earns anything,
//! work out which reward roles the change implies, and hand the write to the
//! repository with the level already computed.

pub mod config;
pub mod leaderboard;
pub mod rewards;
pub mod xp;

#[cfg(test)]
pub mod test_support;

pub use config::{
    GetLevelingConfigUseCase, UpdateLevelingConfigCommand, UpdateLevelingConfigUseCase,
};
pub use leaderboard::{LeaderboardRow, LevelLeaderboardUseCase};
pub use rewards::{
    CreateLevelRewardCommand, CreateLevelRewardUseCase, DeleteLevelRewardUseCase,
    ListLevelRewardsUseCase,
};
pub use xp::{
    GrantMessageXpCommand, GrantMessageXpUseCase, GrantVoiceXpCommand, GrantVoiceXpUseCase,
    SetXpCommand, SetXpUseCase,
};
