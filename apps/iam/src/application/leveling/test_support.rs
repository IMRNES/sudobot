// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! A hand-written [`LevelingRepository`] for the use-case tests.
//!
//! Same spirit as the economy fake: no database, readable in one sitting, and it
//! records which methods ran so a test can prove that the leaderboard does not call
//! `rank_of` once per row or read the config once per row.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::leveling::{
    LevelCurve, LevelProfile, LevelReward, LevelingConfig, LevelingRepository,
};

/// A fixed instant the fakes build their timestamps from.
pub fn fixed_now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_780_000_000, 0).unwrap_or_else(Utc::now)
}

/// A [`Clock`] that answers whatever the test decided "now" is.
#[derive(Debug, Clone, Copy)]
pub struct FakeClock {
    now: DateTime<Utc>,
}

impl FakeClock {
    pub fn at(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    async fn sleep_until(&self, _deadline: DateTime<Utc>) {
        // Nothing in the leveling use cases waits.
    }
}

/// A profile with the fields a test usually does not care about filled in.
///
/// Used both to seed the fake and, in the leaderboard tests, to compute the
/// expected `progress` independently of the code under test.
pub fn profile(discord_user_id: u64, xp: i64, level: i32, message_count: i64) -> LevelProfile {
    let now = fixed_now();
    LevelProfile {
        id: Uuid::new_v4(),
        guild_id: 7,
        discord_user_id,
        xp,
        level,
        total_xp: xp,
        message_count,
        voice_seconds: 0,
        last_xp_at: None,
        created_at: now,
        updated_at: now,
    }
}

/// A repository call, for asserting order and for proving an absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelingCall {
    FindConfig {
        guild_id: u64,
    },
    UpsertConfig {
        guild_id: u64,
    },
    EnsureProfile {
        discord_user_id: u64,
    },
    FindProfile {
        discord_user_id: u64,
    },
    AddXp {
        discord_user_id: u64,
        xp: i64,
        level: i32,
    },
    AddVoiceXp {
        discord_user_id: u64,
        seconds: i64,
        xp: i64,
    },
    SetXp {
        discord_user_id: u64,
        xp: i64,
        level: i32,
    },
    ListLeaderboard,
    ListProfiles,
    RankOf {
        discord_user_id: u64,
    },
    ListRewards,
    RewardsReached {
        level: i32,
    },
    CreateReward {
        level: i32,
    },
    UpdateReward {
        id: Uuid,
        level: i32,
    },
    DeleteReward {
        id: Uuid,
    },
}

/// An in-memory leveling store: one config per guild, profiles and rewards.
#[derive(Debug, Clone, Default)]
pub struct FakeLevelingRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    configs: Vec<LevelingConfig>,
    profiles: Vec<LevelProfile>,
    rewards: Vec<LevelReward>,
    /// Cached display names, keyed by (guild, member) — the same key the real
    /// `guild_member_profiles` row uses.
    names: std::collections::HashMap<(u64, u64), String>,
    calls: Vec<LevelingCall>,
    /// When set, every write fails, so the error paths can be exercised.
    fail_writes: bool,
}

impl FakeLevelingRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// A poisoned lock only means another test thread panicked; the state is still
    /// readable, and recovering keeps this file free of `unwrap`.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn seed_config(&self, config: LevelingConfig) {
        let mut state = self.state();
        state
            .configs
            .retain(|existing| existing.guild_id != config.guild_id);
        state.configs.push(config);
    }

    pub fn set_curve(&self, guild_id: u64, curve: LevelCurve) {
        let mut state = self.state();
        match state
            .configs
            .iter_mut()
            .find(|config| config.guild_id == guild_id)
        {
            Some(config) => config.curve = curve,
            None => {
                let mut config = LevelingConfig::default_for(guild_id);
                config.curve = curve;
                state.configs.push(config);
            }
        }
    }

    pub fn seed_profile(&self, discord_user_id: u64, xp: i64, level: i32, message_count: i64) {
        let mut state = self.state();
        state
            .profiles
            .retain(|existing| existing.discord_user_id != discord_user_id);
        state
            .profiles
            .push(profile(discord_user_id, xp, level, message_count));
    }

    /// Backdates (or forwards-dates) a seeded profile's last XP, for the cooldown
    /// boundary test.
    pub fn set_last_xp_at(&self, guild_id: u64, discord_user_id: u64, at: DateTime<Utc>) {
        let mut state = self.state();
        if let Some(profile) = state.profiles.iter_mut().find(|profile| {
            profile.guild_id == guild_id && profile.discord_user_id == discord_user_id
        }) {
            profile.last_xp_at = Some(at);
        }
    }

    pub fn seed_reward(&self, level: i32, role_id: u64, keep_on_demote: bool) -> Uuid {
        let id = Uuid::new_v4();
        self.state().rewards.push(LevelReward {
            id,
            guild_id: 7,
            level,
            role_id,
            keep_on_demote,
        });
        id
    }

    pub fn fail_writes(&self) {
        self.state().fail_writes = true;
    }

    /// The config as stored, or the defaults when the guild has no row — the same
    /// answer a test expects the use case to have seen.
    pub fn config(&self, guild_id: u64) -> LevelingConfig {
        self.state()
            .configs
            .iter()
            .find(|config| config.guild_id == guild_id)
            .cloned()
            .unwrap_or_else(|| LevelingConfig::default_for(guild_id))
    }

    pub fn profile(&self, guild_id: u64, discord_user_id: u64) -> Option<LevelProfile> {
        self.state()
            .profiles
            .iter()
            .find(|profile| {
                profile.guild_id == guild_id && profile.discord_user_id == discord_user_id
            })
            .cloned()
    }

    pub fn profile_exists(&self, guild_id: u64, discord_user_id: u64) -> bool {
        self.profile(guild_id, discord_user_id).is_some()
    }

    pub fn rewards(&self, guild_id: u64) -> Vec<LevelReward> {
        self.state()
            .rewards
            .iter()
            .filter(|reward| reward.guild_id == guild_id)
            .cloned()
            .collect()
    }

    pub fn calls(&self) -> Vec<LevelingCall> {
        self.state().calls.clone()
    }
}

impl State {
    fn profile_mut(&mut self, guild_id: u64, discord_user_id: u64) -> Option<&mut LevelProfile> {
        self.profiles.iter_mut().find(|profile| {
            profile.guild_id == guild_id && profile.discord_user_id == discord_user_id
        })
    }
}

impl LevelingRepository for FakeLevelingRepository {
    async fn find_config(&self, guild_id: u64) -> Result<LevelingConfig, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::FindConfig { guild_id });

        Ok(state
            .configs
            .iter()
            .find(|config| config.guild_id == guild_id)
            .cloned()
            .unwrap_or_else(|| LevelingConfig::default_for(guild_id)))
    }

    async fn upsert_config(
        &self,
        config: LevelingConfig,
    ) -> Result<LevelingConfig, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::UpsertConfig {
            guild_id: config.guild_id,
        });

        if state.fail_writes {
            return Err(RepositoryError::database("the fake was told to fail"));
        }

        state
            .configs
            .retain(|existing| existing.guild_id != config.guild_id);
        state.configs.push(config.clone());
        Ok(config)
    }

    async fn find_profile(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Option<LevelProfile>, RepositoryError> {
        let mut state = self.state();
        state
            .calls
            .push(LevelingCall::FindProfile { discord_user_id });

        Ok(state
            .profiles
            .iter()
            .find(|profile| {
                profile.guild_id == guild_id && profile.discord_user_id == discord_user_id
            })
            .cloned())
    }

    async fn ensure_profile(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<LevelProfile, RepositoryError> {
        let mut state = self.state();
        state
            .calls
            .push(LevelingCall::EnsureProfile { discord_user_id });

        if let Some(existing) = state.profile_mut(guild_id, discord_user_id) {
            return Ok(existing.clone());
        }

        let created = profile(discord_user_id, 0, 0, 0);
        let mut created = LevelProfile {
            guild_id,
            ..created
        };
        created.created_at = fixed_now();
        created.updated_at = created.created_at;

        state.profiles.push(created.clone());
        Ok(created)
    }

    async fn add_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        xp: i64,
        level: i32,
        counted_message: bool,
    ) -> Result<LevelProfile, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::AddXp {
            discord_user_id,
            xp,
            level,
        });

        if state.fail_writes {
            return Err(RepositoryError::database("the fake was told to fail"));
        }

        let now = fixed_now();
        let curve = state
            .configs
            .iter()
            .find(|config| config.guild_id == guild_id)
            .map(|config| config.curve)
            .unwrap_or(LevelCurve::Mrn);
        let profile = state
            .profile_mut(guild_id, discord_user_id)
            .ok_or_else(|| RepositoryError::not_found("leveling profile"))?;

        // The caller computed the level from the curve, so the write stores what it
        // was handed. `total_xp` is the one figure it does not hand over, and the
        // fake derives it from the position it already held — the same way the real
        // adapter would from the columns it is updating.
        let before = curve.total_xp_for_level(profile.level) + profile.xp;
        profile.xp = xp;
        profile.level = level;
        let after = curve.total_xp_for_level(level) + xp;
        profile.total_xp += (after - before).max(0);
        if counted_message {
            profile.message_count += 1;
        }
        profile.last_xp_at = Some(now);
        profile.updated_at = now;

        Ok(profile.clone())
    }

    async fn add_voice_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        seconds: i64,
        xp: i64,
        level: i32,
    ) -> Result<LevelProfile, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::AddVoiceXp {
            discord_user_id,
            seconds,
            xp,
        });

        if state.fail_writes {
            return Err(RepositoryError::database("the fake was told to fail"));
        }

        let now = fixed_now();
        let curve = state
            .configs
            .iter()
            .find(|config| config.guild_id == guild_id)
            .map(|config| config.curve)
            .unwrap_or(LevelCurve::Mrn);
        let profile = state
            .profile_mut(guild_id, discord_user_id)
            .ok_or_else(|| RepositoryError::not_found("leveling profile"))?;

        let before = curve.total_xp_for_level(profile.level) + profile.xp;
        profile.xp = xp;
        profile.level = level;
        profile.voice_seconds += seconds;
        let after = curve.total_xp_for_level(level) + xp;
        profile.total_xp += (after - before).max(0);
        profile.updated_at = now;

        Ok(profile.clone())
    }

    async fn set_xp(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        xp: i64,
        level: i32,
    ) -> Result<LevelProfile, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::SetXp {
            discord_user_id,
            xp,
            level,
        });

        if state.fail_writes {
            return Err(RepositoryError::database("the fake was told to fail"));
        }

        if state.profile_mut(guild_id, discord_user_id).is_none() {
            let mut created = profile(discord_user_id, 0, 0, 0);
            created.guild_id = guild_id;
            state.profiles.push(created);
        }

        let now = fixed_now();
        let profile = state
            .profile_mut(guild_id, discord_user_id)
            .ok_or_else(|| RepositoryError::not_found("leveling profile"))?;

        profile.xp = xp;
        profile.level = level;
        profile.updated_at = now;

        Ok(profile.clone())
    }

    async fn list_leaderboard(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<LevelProfile>, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::ListLeaderboard);

        let mut matching: Vec<LevelProfile> = state
            .profiles
            .iter()
            .filter(|profile| profile.guild_id == guild_id)
            .cloned()
            .collect();

        // Ordered by total XP, which is what a rank means; ties fall back to the
        // member id so the order is stable between calls.
        matching.sort_by(|left, right| {
            right
                .total_xp
                .cmp(&left.total_xp)
                .then(left.discord_user_id.cmp(&right.discord_user_id))
        });

        let total = matching.len() as u64;
        let data = matching
            .into_iter()
            .skip(params.offset() as usize)
            .take(params.per_page as usize)
            .collect();

        Ok(Page::new(data, params, total))
    }

    async fn list_profiles(&self, guild_id: u64) -> Result<Vec<LevelProfile>, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::ListProfiles);

        let mut matching: Vec<LevelProfile> = state
            .profiles
            .iter()
            .filter(|profile| profile.guild_id == guild_id)
            .cloned()
            .collect();

        // The same ordering the real repository uses, so a reconciliation test sees
        // the guild in the order production would.
        matching.sort_by(|left, right| {
            right
                .total_xp
                .cmp(&left.total_xp)
                .then(left.discord_user_id.cmp(&right.discord_user_id))
        });

        Ok(matching)
    }

    async fn record_member_name(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        display_name: &str,
        _is_bot: bool,
    ) -> Result<(), RepositoryError> {
        let mut state = self.state();
        state
            .names
            .insert((guild_id, discord_user_id), display_name.to_string());
        Ok(())
    }

    async fn member_names(
        &self,
        guild_id: u64,
        discord_user_ids: &[u64],
    ) -> Result<std::collections::HashMap<u64, String>, RepositoryError> {
        let state = self.state();
        Ok(discord_user_ids
            .iter()
            .filter_map(|id| {
                state
                    .names
                    .get(&(guild_id, *id))
                    .map(|name| (*id, name.clone()))
            })
            .collect())
    }

    async fn rank_of(&self, guild_id: u64, discord_user_id: u64) -> Result<u64, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::RankOf { discord_user_id });

        let mine = state
            .profiles
            .iter()
            .find(|profile| {
                profile.guild_id == guild_id && profile.discord_user_id == discord_user_id
            })
            .map(|profile| profile.total_xp)
            .unwrap_or_default();

        let ahead = state
            .profiles
            .iter()
            .filter(|profile| profile.guild_id == guild_id && profile.total_xp > mine)
            .count() as u64;

        Ok(ahead + 1)
    }

    async fn list_rewards(&self, guild_id: u64) -> Result<Vec<LevelReward>, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::ListRewards);

        let mut matching: Vec<LevelReward> = state
            .rewards
            .iter()
            .filter(|reward| reward.guild_id == guild_id)
            .cloned()
            .collect();

        matching.sort_by_key(|reward| reward.level);
        Ok(matching)
    }

    async fn create_reward(&self, reward: LevelReward) -> Result<LevelReward, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::CreateReward {
            level: reward.level,
        });

        if state.fail_writes {
            return Err(RepositoryError::database("the fake was told to fail"));
        }

        if state
            .rewards
            .iter()
            .any(|existing| existing.guild_id == reward.guild_id && existing.level == reward.level)
        {
            return Err(RepositoryError::conflict(
                "a reward already exists for this level",
            ));
        }

        state.rewards.push(reward.clone());
        Ok(reward)
    }

    async fn update_reward(
        &self,
        id: Uuid,
        level: i32,
        role_id: u64,
    ) -> Result<LevelReward, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::UpdateReward { id, level });

        if state.fail_writes {
            return Err(RepositoryError::database("the fake was told to fail"));
        }

        // Gather what we need under one shared borrow, then release it before
        // taking the mutable borrow that updates the row.
        let Some(existing) = state.rewards.iter().find(|reward| reward.id == id) else {
            return Err(RepositoryError::not_found("level reward"));
        };
        let existing_guild_id = existing.guild_id;

        let conflict = state
            .rewards
            .iter()
            .any(|reward| reward.guild_id == existing_guild_id && reward.level == level);
        if conflict {
            return Err(RepositoryError::conflict(
                "a reward already exists for this level",
            ));
        }

        let Some(existing) = state.rewards.iter_mut().find(|reward| reward.id == id) else {
            return Err(RepositoryError::not_found("level reward"));
        };
        existing.level = level;
        existing.role_id = role_id;
        Ok(existing.clone())
    }

    async fn delete_reward(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::DeleteReward { id });

        let before = state.rewards.len();
        state.rewards.retain(|reward| reward.id != id);

        if state.rewards.len() == before {
            return Err(RepositoryError::not_found("level reward"));
        }

        Ok(())
    }

    async fn rewards_reached(
        &self,
        guild_id: u64,
        level: i32,
    ) -> Result<Vec<LevelReward>, RepositoryError> {
        let mut state = self.state();
        state.calls.push(LevelingCall::RewardsReached { level });

        let mut matching: Vec<LevelReward> = state
            .rewards
            .iter()
            .filter(|reward| reward.guild_id == guild_id && reward.level <= level)
            .cloned()
            .collect();

        matching.sort_by_key(|reward| reward.level);
        Ok(matching)
    }
}
