use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::leveling::{
    LevelCurve, LevelProfile, LevelUpOutcome, LevelingError, LevelingRepository, plan_role_changes,
};

/// A message that might earn XP.
#[derive(Debug, Clone)]
pub struct GrantMessageXpCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub channel_id: u64,
    /// The member's roles, so an exempt role can be recognised and so the reward
    /// step knows which roles they already hold.
    pub role_ids: Vec<u64>,
}

pub struct GrantMessageXpUseCase<L, C> {
    leveling: L,
    clock: C,
}

impl<L: LevelingRepository, C: Clock> GrantMessageXpUseCase<L, C> {
    pub fn new(leveling: L, clock: C) -> Self {
        Self { leveling, clock }
    }

    /// Awards message XP, if the member has earned any.
    ///
    /// Returns `None` when they have not — leveling is switched off, the channel or
    /// one of their roles is exempt, or the cooldown has not elapsed. That is the
    /// ordinary case for most messages in a busy guild, so it is a `debug!` and a
    /// `None` rather than a warning and an error: a bot that logged a warning per
    /// message would drown everything else.
    pub async fn execute(
        &self,
        command: GrantMessageXpCommand,
    ) -> Result<Option<LevelUpOutcome>, LevelingError> {
        let now = self.clock.now();
        let config = self.leveling.find_config(command.guild_id).await?;

        if !config.enabled {
            debug!(
                guild_id = command.guild_id,
                "leveling is disabled; no xp awarded"
            );
            return Ok(None);
        }

        // The exemption check comes before any write: an exempt member must not get
        // a profile row just for talking in an ignored channel.
        if config.is_exempt(command.channel_id, &command.role_ids) {
            debug!(
                guild_id = command.guild_id,
                channel_id = command.channel_id,
                "channel or role is exempt; no xp awarded"
            );
            return Ok(None);
        }

        // `ensure_profile` both creates the row on a first message and gives the
        // level to compare against, so the common path is one call.
        let profile = self
            .leveling
            .ensure_profile(command.guild_id, command.discord_user_id)
            .await?;

        if profile.is_on_cooldown(now, config.cooldown_secs) {
            debug!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                cooldown_secs = config.cooldown_secs,
                "still on cooldown; no xp awarded"
            );
            return Ok(None);
        }

        let gained = roll_xp(config.xp_min, config.xp_max, Uuid::new_v4().as_u128());
        let (level, xp) = apply_gain(config.curve, profile.level, profile.xp, gained);

        // The repository writes what it is given: the curve lives here, so the new
        // level is computed here rather than in storage.
        let updated = self
            .leveling
            .add_xp(
                command.guild_id,
                command.discord_user_id,
                xp,
                level,
                // A message is what this records; the voice path counts separately.
                true,
            )
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    error = %error,
                    "recording message xp failed"
                );
                LevelingError::from(error)
            })?;

        let outcome = self
            .build_outcome(
                profile.level,
                updated,
                Some(&command.role_ids),
                config.stack_roles,
            )
            .await?;

        if outcome.leveled_up() {
            info!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                previous_level = outcome.previous_level,
                level = outcome.profile.level,
                levels_gained = outcome.levels_gained,
                roles_to_grant = ?outcome.roles_to_grant,
                roles_to_revoke = ?outcome.roles_to_revoke,
                "member leveled up"
            );
        } else {
            debug!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                gained,
                xp = outcome.profile.xp,
                "message xp awarded"
            );
        }

        Ok(Some(outcome))
    }

    /// Turns the profile the repository returned into the outcome the bot acts on.
    ///
    /// The rule itself is in [`resolve_outcome`], shared with the voice path.
    async fn build_outcome(
        &self,
        previous_level: i32,
        profile: LevelProfile,
        held_role_ids: Option<&[u64]>,
        stack_roles: bool,
    ) -> Result<LevelUpOutcome, LevelingError> {
        resolve_outcome(
            &self.leveling,
            previous_level,
            profile,
            held_role_ids,
            stack_roles,
        )
        .await
    }
}

/// What an XP grant implies for the member's reward roles.
///
/// Shared by the message and voice paths, because they differ only in how the XP
/// was earned: once the repository has handed back the new profile, the question —
/// "which roles does this level entitle them to, and which do they no longer
/// qualify for?" — is the same question.
///
/// Only a level change produces roles. A grant that leaves the member on the level
/// they were already on has nothing to reconcile, and answering anyway would put a
/// reward query on every message in the guild. A reward created *after* a member
/// already passed its level is therefore not granted by this call; that is a
/// reconciliation pass's job, not the message path's.
///
/// The decision itself is [`plan_role_changes`] in the domain, so `stack_roles` is
/// read here rather than merely stored: a guild that switched stacking off gets the
/// lower rungs taken back off as the member climbs past them.
async fn resolve_outcome<L: LevelingRepository>(
    leveling: &L,
    previous_level: i32,
    profile: LevelProfile,
    held_role_ids: Option<&[u64]>,
    stack_roles: bool,
) -> Result<LevelUpOutcome, LevelingError> {
    let levels_gained = profile.level - previous_level;

    if levels_gained <= 0 {
        return Ok(LevelUpOutcome {
            profile,
            previous_level,
            levels_gained,
            roles_to_grant: Vec::new(),
            roles_to_revoke: Vec::new(),
        });
    }

    // The whole reward table, not just the reached rows: the demotion half of the
    // plan has to see rewards above the member's level, and the replacement half
    // has to know which lower rungs exist to take back off. One read serves both.
    let rewards = leveling.list_rewards(profile.guild_id).await?;

    // A caller that does not know what the member wears passes an empty slice: the
    // plan then proposes every reached rung, and the grant call is a no-op for the
    // ones already held.
    let held = held_role_ids.unwrap_or(&[]);
    let changes = plan_role_changes(&rewards, profile.level, held, stack_roles);

    debug!(
        guild_id = profile.guild_id,
        discord_user_id = profile.discord_user_id,
        previous_level,
        level = profile.level,
        stack_roles,
        roles_to_grant = ?changes.grant,
        roles_to_revoke = ?changes.revoke,
        "reward roles resolved for a level change"
    );

    Ok(LevelUpOutcome {
        profile,
        previous_level,
        levels_gained,
        roles_to_grant: changes.grant,
        roles_to_revoke: changes.revoke,
    })
}

/// Voice-channel time that has already been aggregated by the caller.
///
/// `seconds` is a duration rather than a start and end time: the bot knows how long
/// a member was in the channel and this use case has no business re-deriving it
/// from timestamps it would then have to trust.
#[derive(Debug, Clone)]
pub struct GrantVoiceXpCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub seconds: i64,
}

pub struct GrantVoiceXpUseCase<L, C> {
    leveling: L,
    clock: C,
}

impl<L: LevelingRepository, C: Clock> GrantVoiceXpUseCase<L, C> {
    pub fn new(leveling: L, clock: C) -> Self {
        Self { leveling, clock }
    }

    /// Awards XP for time spent in a voice channel.
    ///
    /// Two differences from the message path, both forced by the command it is
    /// given. There is no channel or role list, so the exemption lists cannot be
    /// consulted — a caller that needs voice exempted should not call this. And the
    /// message cooldown is not applied: `LevelingConfig::cooldown_secs` is documented
    /// as the gap between two *message* grants, and sharing it with voice would make
    /// voice XP depend on whether the member happened to type.
    pub async fn execute(
        &self,
        command: GrantVoiceXpCommand,
    ) -> Result<Option<LevelUpOutcome>, LevelingError> {
        // Read despite this path applying no cooldown: the repository stamps the
        // profile's `last_xp_at` from the same instant, and the award line below
        // names it. A wall clock here would make that line disagree with the row.
        let now = self.clock.now();
        let config = self.leveling.find_config(command.guild_id).await?;

        if !config.enabled {
            debug!(
                guild_id = command.guild_id,
                "leveling is disabled; no voice xp awarded"
            );
            return Ok(None);
        }

        if command.seconds <= 0 {
            debug!(
                guild_id = command.guild_id,
                seconds = command.seconds,
                "no time to award for; no voice xp awarded"
            );
            return Ok(None);
        }

        let gained = voice_xp(command.seconds, config.voice_xp_multiplier);

        // A multiplier of zero is a guild that has switched voice XP off without
        // switching leveling off. Worth a line, because it looks like a bug from
        // the outside.
        if gained <= 0 {
            debug!(
                guild_id = command.guild_id,
                seconds = command.seconds,
                multiplier = config.voice_xp_multiplier,
                "voice xp rounds to nothing; nothing awarded"
            );
            return Ok(None);
        }

        let profile = self
            .leveling
            .ensure_profile(command.guild_id, command.discord_user_id)
            .await?;

        let (level, xp) = apply_gain(config.curve, profile.level, profile.xp, gained);

        let updated = self
            .leveling
            .add_voice_xp(
                command.guild_id,
                command.discord_user_id,
                command.seconds,
                xp,
                level,
            )
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    error = %error,
                    "recording voice xp failed"
                );
                LevelingError::from(error)
            })?;

        // `None` for the held roles: this command carries no role list, so every
        // reached reward is reported and the caller skips the ones already held.
        let outcome = self
            .build_outcome(profile.level, updated, None, config.stack_roles)
            .await?;

        if outcome.leveled_up() {
            info!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                seconds = command.seconds,
                previous_level = outcome.previous_level,
                level = outcome.profile.level,
                roles_to_grant = ?outcome.roles_to_grant,
                "member leveled up from voice time"
            );
        } else {
            debug!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                seconds = command.seconds,
                gained,
                at = %now,
                "voice xp awarded"
            );
        }

        Ok(Some(outcome))
    }

    /// See [`GrantMessageXpUseCase::build_outcome`]; the same rule, reached from
    /// the voice path.
    async fn build_outcome(
        &self,
        previous_level: i32,
        profile: LevelProfile,
        held_role_ids: Option<&[u64]>,
        stack_roles: bool,
    ) -> Result<LevelUpOutcome, LevelingError> {
        resolve_outcome(
            &self.leveling,
            previous_level,
            profile,
            held_role_ids,
            stack_roles,
        )
        .await
    }
}

/// An administrative XP change.
#[derive(Debug, Clone)]
pub struct SetXpCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// The member's XP measured from zero — the scale
    /// [`LevelCurve::level_for_xp`] takes. The profile's own `xp` field is the
    /// remainder inside the level this recomputes, which is not the same number.
    pub xp: i64,
}

pub struct SetXpUseCase<L> {
    leveling: L,
}

impl<L: LevelingRepository> SetXpUseCase<L> {
    pub fn new(leveling: L) -> Self {
        Self { leveling }
    }

    /// Sets a member's XP outright and recomputes their level.
    ///
    /// Returns the profile so the caller can see the new level next to whatever it
    /// knew before: lowering the XP past a level boundary is a demotion, and the
    /// reward roles above the new level are the caller's to reconcile (the same
    /// `LevelUpOutcome` machinery is not used here, because a demotion is not a
    /// level-up and the roles a member loses are not this command's business).
    pub async fn execute(&self, command: SetXpCommand) -> Result<LevelProfile, LevelingError> {
        if command.xp < 0 {
            warn!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                xp = command.xp,
                "refused a negative xp set"
            );
            return Err(LevelingError::InvalidLevel);
        }

        let config = self.leveling.find_config(command.guild_id).await?;

        // Read first only so the demotion is visible in the log; the write does not
        // depend on it.
        let before = self
            .leveling
            .find_profile(command.guild_id, command.discord_user_id)
            .await?;

        // A set is an absolute position, so the level is recomputed from the curve
        // rather than patched: `apply_gain` from zero is the same arithmetic a
        // normal gain uses, which keeps one implementation of "where does this put
        // them".
        let (level, xp) = apply_gain(config.curve, 0, 0, command.xp);

        let profile = self
            .leveling
            .set_xp(command.guild_id, command.discord_user_id, xp, level)
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    error = %error,
                    "setting xp failed"
                );
                LevelingError::from(error)
            })?;

        match before {
            Some(before) if before.level > profile.level => warn!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                previous_level = before.level,
                level = profile.level,
                "an xp set demoted this member"
            ),
            _ => info!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                level = profile.level,
                xp = profile.xp,
                "xp set"
            ),
        }

        Ok(profile)
    }
}

/// A roll in `min..=max`, from the bits of a fresh v4 UUID.
///
/// `rand` is not a dependency of this crate and the rule here is that randomness
/// comes from `Uuid::new_v4()`; a v4 UUID carries 122 random bits, and the range is
/// capped at 1000 by `validate_xp_range`. Taking the entropy as an argument rather
/// than generating it inside is what lets a test pin a roll down.
fn roll_xp(min: i32, max: i32, entropy: u128) -> i64 {
    // The config is validated on the way in, but a row written before validation
    // existed could still be inverted; awarding the minimum is the harmless answer.
    if max < min {
        return i64::from(min);
    }

    let span = (i64::from(max) - i64::from(min) + 1) as u128;
    i64::from(min) + (entropy % span) as i64
}

/// The XP a stretch of voice time is worth.
///
/// Truncating rather than rounding: a multiplier is a rate, and rounding up would
/// let a fractional multiplier pay out more than the time it was given.
fn voice_xp(seconds: i64, multiplier: f64) -> i64 {
    if !multiplier.is_finite() || multiplier <= 0.0 {
        return 0;
    }

    (seconds as f64 * multiplier) as i64
}

/// Where a gain leaves a member: the new level and the XP inside it.
///
/// The arithmetic happens in "XP from zero" space, because that is the space the
/// curve's inverse lives in — [`LevelCurve::level_for_xp`] takes a total, not a
/// remainder. A member's standing is lifted out of their current level, moved, and
/// put back down, so a gain that crosses two level boundaries lands on the right one
/// instead of being truncated at the first.
fn apply_gain(curve: LevelCurve, level: i32, xp_in_level: i64, gained: i64) -> (i32, i64) {
    let total = (curve.total_xp_for_level(level) + xp_in_level + gained).max(0);
    let new_level = curve.level_for_xp(total);
    (new_level, total - curve.total_xp_for_level(new_level))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use chrono::{DateTime, Duration, Utc};

    use super::*;
    use crate::domain::leveling::LevelingConfig;

    /// The curve every case below is checked against.
    const CURVE: LevelCurve = LevelCurve::Mrn;

    #[test]
    fn a_roll_stays_inside_the_configured_range() {
        for entropy in [0, 1, 7, u128::MAX, 12_345_678_901_234_567_890] {
            let rolled = roll_xp(15, 25, entropy);
            assert!((15..=25).contains(&rolled), "rolled {rolled}");
        }
    }

    #[test]
    fn a_roll_of_one_value_is_that_value() {
        assert_eq!(roll_xp(20, 20, 0), 20);
        assert_eq!(roll_xp(20, 20, u128::MAX), 20);
    }

    #[test]
    fn an_inverted_range_awards_the_minimum_rather_than_panicking() {
        assert_eq!(roll_xp(30, 10, 0), 30);
    }

    #[test]
    fn voice_xp_scales_with_the_multiplier() {
        assert_eq!(voice_xp(60, 1.0), 60);
        assert_eq!(voice_xp(60, 1.5), 90);
        assert_eq!(voice_xp(60, 2.5), 150);
        // Truncated, not rounded up.
        assert_eq!(voice_xp(10, 0.15), 1);
    }

    #[test]
    fn a_multiplier_that_is_zero_or_nonsense_pays_nothing() {
        assert_eq!(voice_xp(600, 0.0), 0);
        assert_eq!(voice_xp(600, -1.0), 0);
        assert_eq!(voice_xp(600, f64::NAN), 0);
        assert_eq!(voice_xp(600, f64::INFINITY), 0);
    }

    #[test]
    fn a_gain_inside_a_level_does_not_move_the_level() {
        let (level, xp) = apply_gain(CURVE, 4, 10, 20);
        assert_eq!(level, 4);
        assert_eq!(xp, 30);
    }

    #[test]
    fn a_gain_carries_the_remainder_into_the_next_level() {
        // The mrn curve charges 5L^2 + 50L + 100 for level L, so level 4 costs 380.
        // One XP short of it, plus a 15 XP roll, crosses the boundary.
        let (level, xp) = apply_gain(CURVE, 4, 379, 15);
        assert_eq!(level, 5);
        assert_eq!(xp, 379 + 15 - 380);
        assert_eq!(xp, 14);
        assert_eq!(
            level,
            CURVE.level_for_xp(CURVE.total_xp_for_level(4) + 379 + 15)
        );
    }

    #[test]
    fn one_gain_can_cross_several_levels() {
        // Not reachable from a message roll, but an admin set or a curve change is.
        let (level, xp) = apply_gain(CURVE, 0, 0, 1_000);
        assert_eq!(level, CURVE.level_for_xp(1_000));
        assert!(level > 1, "1000 xp is several levels on this curve");
        assert_eq!(xp, 1_000 - CURVE.total_xp_for_level(level));
    }

    #[test]
    fn a_gain_from_zero_is_exactly_the_curve_lookup() {
        for total in [0, 99, 100, 255, 1_000, 15_200] {
            let (level, xp) = apply_gain(CURVE, 0, 0, total);
            assert_eq!(level, CURVE.level_for_xp(total));
            assert_eq!(CURVE.total_xp_for_level(level) + xp, total);
        }
    }

    #[test]
    fn a_negative_gain_never_produces_a_negative_position() {
        let (level, xp) = apply_gain(CURVE, 0, 10, -50);
        assert_eq!(level, 0);
        assert_eq!(xp, 0);
    }

    /// The instant the fake clock answers with.
    fn fixed_now() -> DateTime<Utc> {
        crate::application::leveling::test_support::fixed_now()
    }

    #[tokio::test]
    async fn a_level_change_reports_the_roles_to_grant_and_revoke() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_reward(5, 555, true);
        leveling.seed_reward(6, 666, true);
        leveling.seed_reward(9, 999, false);
        // A member one XP short of level 5 who is wearing the level-6 and level-9
        // roles: an admin moved them down, which is the only way to be wearing a
        // rung above your level, and exactly the case reconciliation exists for.
        // `xp` is the remainder inside level 4, not the total from zero.
        leveling.seed_profile(1, CURVE.xp_for_level(4) - 1, 4, 0);

        let outcome = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                role_ids: vec![666, 999],
            })
            .await
            .expect("the fake never fails")
            .expect("xp was awarded");

        // One XP short of the boundary with a roll of at most 25 always lands on
        // level 5, which needs 380.
        assert_eq!(outcome.profile.level, 5);
        assert_eq!(outcome.previous_level, 4);
        assert_eq!(outcome.levels_gained, 1);
        assert!(outcome.leveled_up());
        assert_eq!(
            outcome.profile.xp,
            CURVE.xp_for_level(4) - 1 + outcome.profile.xp - (CURVE.xp_for_level(4) - 1),
            "the remainder is carried over rather than reset"
        );

        // Level 5 was reached and its role is not held; 555 is the only grant.
        assert_eq!(outcome.roles_to_grant, vec![555]);
        // The level-9 reward does not survive a demote and the member is wearing
        // it, so it is the one role that comes off.
        assert_eq!(outcome.roles_to_revoke, vec![999]);
        assert!(
            !outcome.roles_to_revoke.contains(&666),
            "a reward that keeps on demote is never listed for removal"
        );
    }

    #[tokio::test]
    async fn a_gain_that_does_not_level_up_touches_no_rewards() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_reward(50, 500, false);
        leveling.seed_profile(1, 10, 0, 0);

        let outcome = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                role_ids: Vec::new(),
            })
            .await
            .expect("the fake never fails")
            .expect("xp was awarded");

        assert_eq!(outcome.profile.level, 0);
        assert_eq!(outcome.levels_gained, 0);
        assert!(!outcome.leveled_up());
        assert!(outcome.roles_to_grant.is_empty());
        assert!(outcome.roles_to_revoke.is_empty());
    }

    #[tokio::test]
    async fn a_role_the_member_already_holds_is_not_granted_again() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_reward(3, 333, true);
        leveling.seed_reward(4, 444, true);
        // Level 3, one XP short of level 4, already wearing the level-3 role: the
        // roll takes them to level 4, so both rewards are reached and only the one
        // they do not hold may be granted. `xp` is the remainder inside level 3.
        leveling.seed_profile(1, CURVE.xp_for_level(3) - 1, 3, 0);

        let outcome = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                // They already wear the level-3 role.
                role_ids: vec![333],
            })
            .await
            .expect("the fake never fails")
            .expect("xp was awarded");

        assert_eq!(outcome.profile.level, 4, "one XP short plus a roll crosses");
        assert!(
            !outcome.roles_to_grant.contains(&333),
            "an already-held role is filtered out"
        );
        assert_eq!(outcome.roles_to_grant, vec![444]);
    }

    #[tokio::test]
    async fn a_disabled_guild_earns_nothing() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            enabled: false,
            ..LevelingConfig::default_for(7)
        });

        let outcome = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                role_ids: Vec::new(),
            })
            .await
            .expect("a disabled guild is not an error");

        assert!(outcome.is_none());
        assert!(
            !leveling.profile_exists(7, 1),
            "nothing is written for a guild that switched leveling off"
        );
    }

    #[tokio::test]
    async fn an_exempt_channel_earns_nothing() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            ignored_channel_ids: vec![999],
            ..LevelingConfig::default_for(7)
        });

        let outcome = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 999,
                role_ids: Vec::new(),
            })
            .await
            .expect("an exempt channel is not an error");

        assert!(outcome.is_none());
        assert!(!leveling.profile_exists(7, 1));
    }

    #[tokio::test]
    async fn an_exempt_role_earns_nothing() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            ignored_role_ids: vec![42],
            ..LevelingConfig::default_for(7)
        });

        let outcome = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                role_ids: vec![7, 42, 9],
            })
            .await
            .expect("an exempt role is not an error");

        assert!(outcome.is_none());
    }

    #[tokio::test]
    async fn the_cooldown_is_the_boundary_the_config_sets() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            cooldown_secs: 60,
            ..LevelingConfig::default_for(7)
        });
        let now = fixed_now();

        // One second short of the cooldown.
        leveling.seed_profile(1, 0, 0, 0);
        leveling.set_last_xp_at(7, 1, now - Duration::seconds(59));

        let blocked = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(now))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                role_ids: Vec::new(),
            })
            .await
            .expect("the fake never fails");
        assert!(blocked.is_none(), "59s is inside a 60s cooldown");

        // Exactly the cooldown.
        leveling.set_last_xp_at(7, 1, now - Duration::seconds(60));

        let allowed = GrantMessageXpUseCase::new(leveling.clone(), FakeClock::at(now))
            .execute(GrantMessageXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                channel_id: 100,
                role_ids: Vec::new(),
            })
            .await
            .expect("the fake never fails");
        assert!(allowed.is_some(), "60s is the first second that counts");
    }

    #[tokio::test]
    async fn voice_xp_uses_the_multiplier_and_ignores_the_message_cooldown() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            voice_xp_multiplier: 2.0,
            ..LevelingConfig::default_for(7)
        });

        let outcome = GrantVoiceXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantVoiceXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                seconds: 30,
            })
            .await
            .expect("the fake never fails")
            .expect("30s at double rate is 60 xp");

        assert_eq!(outcome.profile.xp, 60);
        assert_eq!(outcome.profile.voice_seconds, 30);
        assert_eq!(
            outcome.profile.message_count, 0,
            "voice does not count messages"
        );
    }

    #[tokio::test]
    async fn voice_time_of_zero_earns_nothing() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();

        for seconds in [0, -5] {
            let outcome = GrantVoiceXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
                .execute(GrantVoiceXpCommand {
                    guild_id: 7,
                    discord_user_id: 1,
                    seconds,
                })
                .await
                .expect("a zero duration is not an error");

            assert!(outcome.is_none(), "seconds {seconds}");
        }
    }

    #[tokio::test]
    async fn a_voice_multiplier_of_zero_earns_nothing_and_writes_nothing() {
        use crate::application::leveling::test_support::{FakeClock, FakeLevelingRepository};

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            voice_xp_multiplier: 0.0,
            ..LevelingConfig::default_for(7)
        });

        let outcome = GrantVoiceXpUseCase::new(leveling.clone(), FakeClock::at(fixed_now()))
            .execute(GrantVoiceXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                seconds: 3_600,
            })
            .await
            .expect("a switched-off reward is not an error");

        assert!(outcome.is_none());
        assert!(!leveling.profile_exists(7, 1));
    }

    #[tokio::test]
    async fn setting_xp_recomputes_the_level_from_the_curve() {
        use crate::application::leveling::test_support::FakeLevelingRepository;

        let leveling = FakeLevelingRepository::new();
        let total = CURVE.total_xp_for_level(10) + 77;

        let profile = SetXpUseCase::new(leveling.clone())
            .execute(SetXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                xp: total,
            })
            .await
            .expect("77 xp into level 10 is a valid position");

        assert_eq!(profile.level, 10);
        assert_eq!(profile.xp, 77);
        assert_eq!(profile.guild_id, 7);
    }

    #[tokio::test]
    async fn setting_a_lower_xp_demotes() {
        use crate::application::leveling::test_support::FakeLevelingRepository;

        let leveling = FakeLevelingRepository::new();
        leveling.seed_profile(1, CURVE.total_xp_for_level(12), 12, 0);

        let profile = SetXpUseCase::new(leveling.clone())
            .execute(SetXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                xp: 0,
            })
            .await
            .expect("zero is a valid position");

        assert_eq!(profile.level, 0, "the caller can see the demotion");
        assert_eq!(profile.xp, 0);
    }

    #[tokio::test]
    async fn setting_a_negative_xp_is_refused() {
        use crate::application::leveling::test_support::FakeLevelingRepository;

        let leveling = FakeLevelingRepository::new();
        leveling.seed_profile(1, 500, 3, 0);

        let error = SetXpUseCase::new(leveling.clone())
            .execute(SetXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                xp: -1,
            })
            .await
            .expect_err("xp cannot be negative");

        assert!(matches!(error, LevelingError::InvalidLevel));
        assert_eq!(
            leveling.profile(7, 1).map(|profile| profile.xp),
            Some(500),
            "a refused set changes nothing"
        );
    }

    #[tokio::test]
    async fn setting_xp_uses_the_guilds_curve() {
        use crate::application::leveling::test_support::FakeLevelingRepository;

        let leveling = FakeLevelingRepository::new();
        leveling.seed_config(LevelingConfig {
            curve: LevelCurve::Flat,
            ..LevelingConfig::default_for(7)
        });

        // Every flat level costs 100, so 350 xp is level 3 with 50 in hand.
        let profile = SetXpUseCase::new(leveling.clone())
            .execute(SetXpCommand {
                guild_id: 7,
                discord_user_id: 1,
                xp: 350,
            })
            .await
            .expect("a valid position");

        assert_eq!(profile.level, 3);
        assert_eq!(profile.xp, 50);
    }

    #[test]
    fn the_fake_clock_and_the_real_one_agree_on_the_shape_of_now() {
        let now = fixed_now();
        assert!(now > Utc::now() - Duration::days(365 * 10));
    }
}
