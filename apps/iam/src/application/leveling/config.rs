use tracing::{info, warn};

use crate::domain::leveling::{
    LevelingConfig, LevelingError, LevelingRepository, parse_curve, validate_cooldown,
    validate_xp_range,
};

/// Reads a guild's leveling settings.
///
/// A guild that has never opened the page has no row: the repository answers with
/// [`LevelingConfig::default_for`], so there is no "not found" case for a caller to
/// handle and a fresh guild's bot works before anyone visits the dashboard.
pub struct GetLevelingConfigUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> GetLevelingConfigUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<LevelingConfig, LevelingError> {
        Ok(self.leveling.find_config(guild_id).await?)
    }
}

/// A partial update to the leveling configuration.
///
/// Every field is optional, and two of them carry a two-level option: absent means
/// "leave it", `null` (the outer `Some` with an inner `None`) means "clear it".
#[derive(Debug, Clone)]
pub struct UpdateLevelingConfigCommand {
    pub guild_id: u64,
    pub enabled: Option<bool>,
    pub xp_min: Option<i32>,
    pub xp_max: Option<i32>,
    pub cooldown_secs: Option<i32>,
    pub voice_xp_multiplier: Option<f64>,
    /// `mrn`, `linear` or `flat`.
    pub curve: Option<String>,
    /// `Some(None)` clears the announcement channel.
    pub announce_channel_id: Option<Option<u64>>,
    pub stack_roles: Option<bool>,
    pub ignored_channel_ids: Option<Vec<u64>>,
    pub ignored_role_ids: Option<Vec<u64>>,
}

pub struct UpdateLevelingConfigUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> UpdateLevelingConfigUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    /// Applies a patch to the guild's settings.
    ///
    /// The order is read → merge → validate → write, and the validation runs on the
    /// **merged** config rather than on the patch. A request that sets only `xp_min`
    /// to 30 has to be refused against the stored `xp_max` of 25; validating the
    /// patch alone would see one perfectly good number and store an inverted range,
    /// which the XP roll would then trip over on every message.
    pub async fn execute(
        &self,
        command: UpdateLevelingConfigCommand,
    ) -> Result<LevelingConfig, LevelingError> {
        let mut config = self.leveling.find_config(command.guild_id).await?;
        let curve = command
            .curve
            .as_deref()
            .map(parse_curve)
            .transpose()
            .inspect_err(|error| {
                warn!(
                    guild_id = command.guild_id,
                    curve = ?command.curve,
                    error = %error,
                    "refused an unknown level curve"
                );
            })?;

        // The patch, onto the existing row.
        if let Some(enabled) = command.enabled {
            config.enabled = enabled;
        }
        if let Some(xp_min) = command.xp_min {
            config.xp_min = xp_min;
        }
        if let Some(xp_max) = command.xp_max {
            config.xp_max = xp_max;
        }
        if let Some(cooldown_secs) = command.cooldown_secs {
            config.cooldown_secs = cooldown_secs;
        }
        if let Some(multiplier) = command.voice_xp_multiplier {
            config.voice_xp_multiplier = multiplier;
        }
        if let Some(curve) = curve {
            config.curve = curve;
        }
        if let Some(announce_channel_id) = command.announce_channel_id {
            config.announce_channel_id = announce_channel_id;
        }
        if let Some(stack_roles) = command.stack_roles {
            config.stack_roles = stack_roles;
        }
        if let Some(ignored_channel_ids) = command.ignored_channel_ids {
            config.ignored_channel_ids = ignored_channel_ids;
        }
        if let Some(ignored_role_ids) = command.ignored_role_ids {
            config.ignored_role_ids = ignored_role_ids;
        }

        // The merged result, through the domain's validators. Each failure is a
        // rejected form rather than a fault, so it is logged as a warning with the
        // numbers that clashed — the dashboard shows the message and the admin
        // fixes the field.
        let (xp_min, xp_max) =
            validate_xp_range(config.xp_min, config.xp_max).inspect_err(|error| {
                warn!(
                    guild_id = command.guild_id,
                    xp_min = config.xp_min,
                    xp_max = config.xp_max,
                    error = %error,
                    "refused an invalid xp range"
                );
            })?;

        let cooldown_secs = validate_cooldown(config.cooldown_secs).inspect_err(|error| {
            warn!(
                guild_id = command.guild_id,
                cooldown_secs = config.cooldown_secs,
                error = %error,
                "refused an invalid cooldown"
            );
        })?;

        // The validators return the values they accepted, so what gets stored is
        // the checked pair rather than the raw fields.
        config.xp_min = xp_min;
        config.xp_max = xp_max;
        config.cooldown_secs = cooldown_secs;

        let saved = self.leveling.upsert_config(config).await?;

        info!(
            guild_id = saved.guild_id,
            enabled = saved.enabled,
            xp_min = saved.xp_min,
            xp_max = saved.xp_max,
            cooldown_secs = saved.cooldown_secs,
            curve = saved.curve.as_str(),
            "leveling configuration updated"
        );

        Ok(saved)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::application::leveling::test_support::FakeLevelingRepository;

    fn patch(guild_id: u64) -> UpdateLevelingConfigCommand {
        UpdateLevelingConfigCommand {
            guild_id,
            enabled: None,
            xp_min: None,
            xp_max: None,
            cooldown_secs: None,
            voice_xp_multiplier: None,
            curve: None,
            announce_channel_id: None,
            stack_roles: None,
            ignored_channel_ids: None,
            ignored_role_ids: None,
        }
    }

    #[tokio::test]
    async fn a_guild_that_never_opened_the_page_gets_the_defaults() {
        let leveling = FakeLevelingRepository::new();

        let config = GetLevelingConfigUseCase::new(leveling.clone())
            .execute(7)
            .await
            .expect("defaults are always available");

        assert_eq!(config, LevelingConfig::default_for(7));
    }

    #[tokio::test]
    async fn a_lone_xp_min_is_validated_against_the_stored_xp_max() {
        let leveling = FakeLevelingRepository::new();
        // A stored range of 15..25, which the dashboard is only patching one half of.
        let stored = LevelingConfig {
            xp_min: 15,
            xp_max: 25,
            ..LevelingConfig::default_for(7)
        };
        leveling.seed_config(stored.clone());

        let mut command = patch(7);
        command.xp_min = Some(30);

        let error = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect_err("30..25 is inverted");

        assert!(matches!(
            error,
            LevelingError::InvalidXpRange { min: 30, max: 25 }
        ));
        assert_eq!(
            leveling.config(7),
            stored,
            "a rejected patch changes nothing"
        );
    }

    #[tokio::test]
    async fn a_lone_xp_max_is_validated_against_the_stored_xp_min() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            xp_min: 15,
            xp_max: 25,
            ..LevelingConfig::default_for(7)
        });

        let mut command = patch(7);
        command.xp_max = Some(10);

        let error = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect_err("15..10 is inverted");

        assert!(matches!(
            error,
            LevelingError::InvalidXpRange { min: 15, max: 10 }
        ));
    }

    #[tokio::test]
    async fn a_patch_that_keeps_the_range_valid_is_accepted() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            xp_min: 15,
            xp_max: 25,
            ..LevelingConfig::default_for(7)
        });

        let mut command = patch(7);
        command.xp_min = Some(20);

        let config = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect("20..25 is a valid range");

        assert_eq!(config.xp_min, 20);
        assert_eq!(config.xp_max, 25, "the untouched half is preserved");
        assert_eq!(leveling.config(7).xp_min, 20, "and it was written");
    }

    #[tokio::test]
    async fn an_out_of_bounds_range_is_refused() {
        let leveling = FakeLevelingRepository::new();

        let mut command = patch(7);
        command.xp_min = Some(0);

        let error = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect_err("a min of zero would award nothing");

        assert!(matches!(error, LevelingError::XpOutOfBounds));
    }

    #[tokio::test]
    async fn an_out_of_range_cooldown_is_refused() {
        let leveling = FakeLevelingRepository::new();

        let mut command = patch(7);
        command.cooldown_secs = Some(3601);

        let error = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect_err("an hour and a second is too long");

        assert!(matches!(error, LevelingError::InvalidCooldown));
    }

    #[tokio::test]
    async fn an_unknown_curve_is_refused_before_anything_is_written() {
        let leveling = FakeLevelingRepository::new();
        let stored = LevelingConfig::default_for(7);
        leveling.seed_config(stored.clone());

        let mut command = patch(7);
        command.curve = Some("exponential".to_string());

        let error = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect_err("the bot cannot compute that curve");

        assert!(matches!(error, LevelingError::UnknownCurve(name) if name == "exponential"));
        assert_eq!(leveling.config(7), stored);
    }

    #[tokio::test]
    async fn a_known_curve_is_parsed_and_stored() {
        let leveling = FakeLevelingRepository::new();

        let mut command = patch(7);
        command.curve = Some("linear".to_string());
        command.enabled = Some(false);
        command.announce_channel_id = Some(Some(99));

        let config = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect("linear is a known curve");

        assert_eq!(config.curve.as_str(), "linear");
        assert!(!config.enabled);
        assert_eq!(config.announce_channel_id, Some(99));
    }

    #[tokio::test]
    async fn an_explicit_null_clears_the_announcement_channel() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            announce_channel_id: Some(99),
            ..LevelingConfig::default_for(7)
        });

        // Absent leaves it alone.
        let config = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(patch(7))
            .await
            .expect("an empty patch is a no-op");
        assert_eq!(config.announce_channel_id, Some(99));

        // `Some(None)` clears it.
        let mut command = patch(7);
        command.announce_channel_id = Some(None);

        let config = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect("clearing the channel is a valid patch");
        assert_eq!(config.announce_channel_id, None);
    }

    #[tokio::test]
    async fn ignored_ids_are_replaced_wholesale() {
        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            ignored_channel_ids: vec![1, 2, 3],
            ignored_role_ids: vec![9],
            ..LevelingConfig::default_for(7)
        });

        let mut command = patch(7);
        command.ignored_channel_ids = Some(vec![4]);

        let config = UpdateLevelingConfigUseCase::new(leveling.clone())
            .execute(command)
            .await
            .expect("a new exemption list");

        assert_eq!(config.ignored_channel_ids, vec![4]);
        assert_eq!(
            config.ignored_role_ids,
            vec![9],
            "a list that was not sent is left alone"
        );
    }
}
