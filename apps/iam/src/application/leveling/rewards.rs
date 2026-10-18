//! Reward roles: one role per level, granted as members reach it.

use tracing::{error, info, warn};
use uuid::Uuid;

use crate::domain::errors::RepositoryError;
use crate::domain::leveling::{LevelReward, LevelingError, LevelingRepository, validate_level};

pub struct ListLevelRewardsUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> ListLevelRewardsUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<Vec<LevelReward>, LevelingError> {
        Ok(self.leveling.list_rewards(guild_id).await?)
    }
}

/// A new reward role.
#[derive(Debug, Clone)]
pub struct CreateLevelRewardCommand {
    /// The level that earns the role; at least 1, because level 0 is where
    /// everybody starts and a reward there would fire for the whole guild at once.
    pub guild_id: u64,
    pub level: i32,
    pub role_id: u64,
    /// Whether the role survives a demotion.
    pub keep_on_demote: bool,
}

pub struct CreateLevelRewardUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> CreateLevelRewardUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    /// Adds a reward role.
    ///
    /// One reward per level is a database constraint (the unique index on
    /// `(guild_id, level)`), and it is reported as one: checking first would let two
    /// admins adding level 5 at the same moment both pass the read and one of them
    /// silently lose. The repository's `Conflict` is translated into the domain's
    /// own `RewardLevelTaken`, which is the message worth showing.
    pub async fn execute(
        &self,
        command: CreateLevelRewardCommand,
    ) -> Result<LevelReward, LevelingError> {
        let level = validate_level(command.level).inspect_err(|error| {
            warn!(
                guild_id = command.guild_id,
                level = command.level,
                error = %error,
                "refused a reward for an invalid level"
            );
        })?;

        let reward = LevelReward {
            id: Uuid::new_v4(),
            guild_id: command.guild_id,
            level,
            role_id: command.role_id,
            keep_on_demote: command.keep_on_demote,
        };

        match self.leveling.create_reward(reward).await {
            Ok(created) => {
                info!(
                    guild_id = created.guild_id,
                    reward_id = %created.id,
                    level = created.level,
                    role_id = created.role_id,
                    keep_on_demote = created.keep_on_demote,
                    "level reward created"
                );
                Ok(created)
            }
            Err(RepositoryError::Conflict(_)) => {
                warn!(
                    guild_id = command.guild_id,
                    level, "a reward already exists for this level"
                );
                Err(LevelingError::RewardLevelTaken(level))
            }
            Err(error) => {
                error!(
                    guild_id = command.guild_id,
                    level,
                    error = %error,
                    "creating a level reward failed"
                );
                Err(LevelingError::from(error))
            }
        }
    }
}

pub struct DeleteLevelRewardUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> DeleteLevelRewardUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    /// Removes a reward.
    ///
    /// Deleting a reward never takes the role away from members who already earned
    /// it: that role is now theirs, and silently stripping it from everyone who
    /// passed level 5 would be a purge nobody asked for. The caller decides whether
    /// to reconcile, which is why this returns nothing.
    pub async fn execute(&self, id: Uuid) -> Result<(), LevelingError> {
        self.leveling
            .delete_reward(id)
            .await
            .map_err(|error| map_reward_error(id, error))?;

        info!(reward_id = %id, "level reward deleted");
        Ok(())
    }
}

/// A reward that is not there is the reward the caller named.
///
/// `delete_reward` reports a missing row as a repository `NotFound`; the domain's
/// `RewardNotFound` is the answer a client can act on, and the id is only in hand
/// here.
fn map_reward_error(id: Uuid, error: RepositoryError) -> LevelingError {
    match error {
        RepositoryError::NotFound { .. } => LevelingError::RewardNotFound(id),
        other => LevelingError::Repository(other),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::application::leveling::test_support::FakeLevelingRepository;

    fn command(level: i32) -> CreateLevelRewardCommand {
        CreateLevelRewardCommand {
            guild_id: 7,
            level,
            role_id: 555,
            keep_on_demote: true,
        }
    }

    #[tokio::test]
    async fn a_reward_is_created_for_a_valid_level() {
        let leveling = FakeLevelingRepository::new();

        let reward = CreateLevelRewardUseCase::new(leveling.clone())
            .execute(command(5))
            .await
            .expect("level 5 is valid");

        assert_eq!(reward.level, 5);
        assert_eq!(reward.role_id, 555);
        assert!(reward.keep_on_demote);
        assert_eq!(leveling.rewards(7).len(), 1);
    }

    #[tokio::test]
    async fn a_negative_level_is_refused() {
        let leveling = FakeLevelingRepository::new();

        for level in [-1, -100] {
            let error = CreateLevelRewardUseCase::new(leveling.clone())
                .execute(command(level))
                .await
                .expect_err("a reward cannot sit below where everyone starts");

            assert!(
                matches!(error, LevelingError::InvalidLevel),
                "level {level}"
            );
        }

        assert!(leveling.rewards(7).is_empty());
    }

    #[tokio::test]
    async fn a_reward_at_level_zero_is_allowed() {
        // The ladder's first rung is level 0, which is where every member starts:
        // Neophyte has to be expressible as a reward or a new arrival has no title.
        let leveling = FakeLevelingRepository::new();

        let reward = CreateLevelRewardUseCase::new(leveling.clone())
            .execute(command(0))
            .await
            .expect("level 0 is Neophyte");

        assert_eq!(reward.level, 0);
        assert_eq!(leveling.rewards(7).len(), 1);
    }

    #[tokio::test]
    async fn a_second_reward_for_the_same_level_is_a_conflict() {
        let leveling = FakeLevelingRepository::new();

        CreateLevelRewardUseCase::new(leveling.clone())
            .execute(command(5))
            .await
            .expect("the first one is fine");

        let error = CreateLevelRewardUseCase::new(leveling.clone())
            .execute(CreateLevelRewardCommand {
                role_id: 777,
                ..command(5)
            })
            .await
            .expect_err("one reward per level");

        assert!(matches!(error, LevelingError::RewardLevelTaken(5)));
        assert_eq!(leveling.rewards(7).len(), 1);
    }

    #[tokio::test]
    async fn the_same_level_in_another_guild_is_not_a_conflict() {
        let leveling = FakeLevelingRepository::new();

        CreateLevelRewardUseCase::new(leveling.clone())
            .execute(command(5))
            .await
            .expect("guild 7");

        CreateLevelRewardUseCase::new(leveling.clone())
            .execute(CreateLevelRewardCommand {
                guild_id: 8,
                ..command(5)
            })
            .await
            .expect("guild 8 has its own rewards");
    }

    #[tokio::test]
    async fn listing_returns_every_reward_in_the_guild() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_reward(5, 555, true);
        leveling.seed_reward(10, 1000, false);

        let rewards = ListLevelRewardsUseCase::new(leveling.clone())
            .execute(7)
            .await
            .expect("a list, not a page");

        assert_eq!(rewards.len(), 2);
        assert_eq!(rewards[0].level, 5);
        assert_eq!(rewards[1].level, 10);
    }

    #[tokio::test]
    async fn deleting_an_existing_reward_succeeds() {
        let leveling = FakeLevelingRepository::new();
        let reward = leveling.seed_reward(5, 555, true);

        DeleteLevelRewardUseCase::new(leveling.clone())
            .execute(reward)
            .await
            .expect("the reward exists");

        assert!(leveling.rewards(7).is_empty());
    }

    #[tokio::test]
    async fn deleting_a_missing_reward_names_the_reward() {
        let leveling = FakeLevelingRepository::new();
        let missing = Uuid::new_v4();

        let error = DeleteLevelRewardUseCase::new(leveling.clone())
            .execute(missing)
            .await
            .expect_err("there is no such reward");

        assert!(matches!(error, LevelingError::RewardNotFound(id) if id == missing));
    }
}
