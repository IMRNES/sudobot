use chrono::{DateTime, Utc};
use uuid::Uuid;

/// Per-guild XP configuration, with the defaults a fresh guild gets.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelingConfig {
    pub guild_id: u64,
    pub enabled: bool,
    pub xp_min: i32,
    pub xp_max: i32,
    /// Seconds between two message-XP grants for the same member.
    pub cooldown_secs: i32,
    pub voice_xp_multiplier: f64,
    pub curve: LevelCurve,
    pub announce_channel_id: Option<u64>,
    /// Whether reward roles accumulate or replace one another.
    pub stack_roles: bool,
    pub ignored_channel_ids: Vec<u64>,
    pub ignored_role_ids: Vec<u64>,
}

impl LevelingConfig {
    /// What a guild that has never opened the leveling page uses.
    pub fn default_for(guild_id: u64) -> Self {
        Self {
            guild_id,
            enabled: true,
            xp_min: 15,
            xp_max: 25,
            cooldown_secs: 60,
            voice_xp_multiplier: 1.0,
            curve: LevelCurve::Mrn,
            announce_channel_id: None,
            // Replace, not stack. A ladder is a statement of where a member stands
            // now, and a member wearing twenty badges says less than one wearing
            // the badge they earned. Stacking stays available as an opt-in for
            // guilds that want trophies, but it is not what an unconfigured guild
            // gets.
            stack_roles: false,
            ignored_channel_ids: Vec::new(),
            ignored_role_ids: Vec::new(),
        }
    }

    /// True when a channel and the member's roles exempt them from earning XP.
    pub fn is_exempt(&self, channel_id: u64, role_ids: &[u64]) -> bool {
        self.ignored_channel_ids.contains(&channel_id)
            || role_ids
                .iter()
                .any(|role| self.ignored_role_ids.contains(role))
    }
}

/// How the XP required for the next level grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelCurve {
    /// Mee6-compatible curve: `5 * level^2 + 50 * level + 100`.
    Mrn,
    /// Linear: `100 * level`.
    Linear,
    /// Flat: every level costs the same `100` XP.
    Flat,
}

impl LevelCurve {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mrn" => Some(Self::Mrn),
            "linear" => Some(Self::Linear),
            "flat" => Some(Self::Flat),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mrn => "mrn",
            Self::Linear => "linear",
            Self::Flat => "flat",
        }
    }

    /// XP needed to go from `level` to `level + 1`.
    pub fn xp_for_level(self, level: i32) -> i64 {
        let level = i64::from(level.max(0));
        match self {
            Self::Mrn => 5 * level * level + 50 * level + 100,
            Self::Linear => 100 * level,
            Self::Flat => 100,
        }
    }

    /// Total XP needed to reach `level` from zero.
    ///
    /// Summed rather than a closed form so every curve stays correct if a term is
    /// tweaked; levels stay small, so the loop is never the bottleneck.
    pub fn total_xp_for_level(self, level: i32) -> i64 {
        (0..level.max(0)).map(|step| self.xp_for_level(step)).sum()
    }

    /// The level a total XP amount corresponds to.
    pub fn level_for_xp(self, xp: i64) -> i32 {
        let mut level = 0;
        let mut remaining = xp;
        loop {
            let needed = self.xp_for_level(level);
            if remaining < needed {
                return level;
            }
            remaining -= needed;
            level += 1;
        }
    }
}

/// A member's standing in one guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelProfile {
    pub id: Uuid,
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// XP inside the current level.
    pub xp: i64,
    pub level: i32,
    /// XP ever earned, which does not fall when a penalty is applied.
    pub total_xp: i64,
    pub message_count: i64,
    pub voice_seconds: i64,
    pub last_xp_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl LevelProfile {
    /// Whether this member may earn XP again yet.
    pub fn is_on_cooldown(&self, now: DateTime<Utc>, cooldown_secs: i32) -> bool {
        match self.last_xp_at {
            None => false,
            Some(last) => now.signed_duration_since(last).num_seconds() < i64::from(cooldown_secs),
        }
    }

    /// XP still needed to reach the next level.
    pub fn xp_to_next_level(&self, curve: LevelCurve) -> i64 {
        curve.xp_for_level(self.level) - self.xp
    }

    /// Share of the way to the next level, for a progress bar.
    pub fn progress(&self, curve: LevelCurve) -> f64 {
        let needed = curve.xp_for_level(self.level);
        if needed <= 0 {
            return 1.0;
        }
        (self.xp as f64 / needed as f64).clamp(0.0, 1.0)
    }
}

/// A reward role for reaching a level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelReward {
    pub id: Uuid,
    pub guild_id: u64,
    pub level: i32,
    pub role_id: u64,
    /// When false, the role is taken back if the member drops below the level.
    pub keep_on_demote: bool,
}

/// The outcome of applying an XP gain.
///
/// Returned by the use case so the bot knows whether to announce, and the
/// dashboard can show what changed without re-reading.
#[derive(Debug, Clone)]
pub struct LevelUpOutcome {
    pub profile: LevelProfile,
    pub previous_level: i32,
    pub levels_gained: i32,
    pub roles_to_grant: Vec<u64>,
    pub roles_to_revoke: Vec<u64>,
}

impl LevelUpOutcome {
    pub fn leveled_up(&self) -> bool {
        self.levels_gained > 0
    }
}

/// The role changes a member's standing implies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleChanges {
    /// Roles the member qualifies for and should be wearing.
    pub grant: Vec<u64>,
    /// Roles the member is wearing and should no longer wear.
    pub revoke: Vec<u64>,
}

impl RoleChanges {
    pub fn is_empty(&self) -> bool {
        self.grant.is_empty() && self.revoke.is_empty()
    }
}

/// Decides which reward roles a member should hold at `level`, and which of the
/// ones they hold must come off.
///
/// This is the whole stacking rule, and it lives in the domain because the bot and
/// the dashboard must not be able to disagree about it — the same reason the curve
/// is here.
///
/// Two configurations, and the difference only shows once a member has passed more
/// than one rung:
///
/// - **Stacking** (`stack_roles: true`): every reached rung is worn, so a climb
///   from 3 to 12 leaves `Apprentice` and `Pathfinder` on. Titles accumulate, which
///   is what a ladder of trophies is for.
/// - **Replacing** (`stack_roles: false`): only the highest reached rung is worn,
///   and the lower ones come off. The member's role list then reads as their
///   *current* standing rather than their history.
///
/// `keep_on_demote` protects a role from removal for being *above* the member's
/// level; it never protects a lower rung from being replaced in the
/// non-stacking configuration, because that removal is the configuration working
/// as asked rather than a demotion. Reading it as a shield against everything would
/// make a non-stacking guild stack anyway as soon as anyone was promoted.
///
/// `held_role_ids` is what lets an already-worn role be left alone: granting a role
/// a member has is a wasted API call, and Discord answers a revocation of a role
/// the member does not have with an error. An empty slice means *wears nothing*,
/// not *unknown*, and both halves honour it — nothing is revoked, and every reached
/// rung is proposed as a grant, which is idempotent on Discord's side. A caller that
/// does not know the member's roles therefore passes `&[]` and gets the safe
/// outcome rather than an error.
pub fn plan_role_changes(
    rewards: &[LevelReward],
    level: i32,
    held_role_ids: &[u64],
    stack_roles: bool,
) -> RoleChanges {
    let holds = |role_id: u64| held_role_ids.contains(&role_id);

    // Highest first, so the non-stacking branch can take the head as the title and
    // treat the rest as replaced.
    let mut reached: Vec<&LevelReward> = rewards
        .iter()
        .filter(|reward| reward.level <= level)
        .collect();
    reached.sort_by_key(|reward| std::cmp::Reverse(reward.level));

    let wanted: Vec<u64> = if stack_roles {
        reached.iter().map(|reward| reward.role_id).collect()
    } else {
        reached
            .first()
            .map(|top| vec![top.role_id])
            .unwrap_or_default()
    };

    let grant: Vec<u64> = wanted
        .iter()
        .copied()
        .filter(|role_id| !holds(*role_id))
        .collect();

    // Roles above the member's level that do not survive a demotion. The whole
    // reward table has to be consulted: a member can be wearing a role from a level
    // they no longer hold if an admin moved them down earlier.
    //
    // Filtered by what they wear, for the same reason the grant side is: Discord
    // answers a removal of a role the member does not have with an error, so
    // proposing it turns every reconcile into a failed API call.
    let mut revoke: Vec<u64> = rewards
        .iter()
        .filter(|reward| reward.level > level && !reward.keep_on_demote)
        .map(|reward| reward.role_id)
        .filter(|role_id| holds(*role_id))
        .collect();

    // In the replacing configuration the lower rungs come off as well. Roles the
    // member is not wearing are skipped so the revoke list only names real work,
    // and the demotion list above is disjoint from this one by construction — it is
    // strictly above the level, this is strictly at or below it.
    if !stack_roles {
        revoke.extend(
            reached
                .iter()
                .skip(1)
                .map(|reward| reward.role_id)
                .filter(|role_id| holds(*role_id)),
        );
    }

    RoleChanges { grant, revoke }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn mrn_curve_matches_the_reference_bot() {
        // The formula every Mee6 migration expects.
        assert_eq!(LevelCurve::Mrn.xp_for_level(0), 100);
        assert_eq!(LevelCurve::Mrn.xp_for_level(1), 155);
        assert_eq!(LevelCurve::Mrn.xp_for_level(2), 220);
    }

    #[test]
    fn level_lookup_inverts_the_curve() {
        for level in 0..25 {
            let total = LevelCurve::Mrn.total_xp_for_level(level);
            assert_eq!(LevelCurve::Mrn.level_for_xp(total), level);
            // One XP short must still be the previous level.
            if level > 0 {
                assert_eq!(LevelCurve::Mrn.level_for_xp(total - 1), level - 1);
            }
        }
    }

    #[test]
    fn progress_is_bounded() {
        let profile = LevelProfile {
            id: Uuid::nil(),
            guild_id: 1,
            discord_user_id: 1,
            xp: 50,
            level: 0,
            total_xp: 50,
            message_count: 0,
            voice_seconds: 0,
            last_xp_at: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let progress = profile.progress(LevelCurve::Mrn);
        assert!((0.0..=1.0).contains(&progress));
        assert_eq!(profile.xp_to_next_level(LevelCurve::Mrn), 50);
    }

    /// A reward table covering levels 0, 3, 10 and 50, with the level-50 rung
    /// configured to come off on a demotion.
    fn ladder_rewards() -> Vec<LevelReward> {
        vec![
            reward(0, 100, true),
            reward(3, 300, true),
            reward(10, 1000, true),
            reward(50, 5000, false),
        ]
    }

    fn reward(level: i32, role_id: u64, keep_on_demote: bool) -> LevelReward {
        LevelReward {
            id: Uuid::new_v4(),
            guild_id: 1,
            level,
            role_id,
            keep_on_demote,
        }
    }

    #[test]
    fn stacking_keeps_every_rung_the_member_has_passed() {
        // A member at level 12 has passed 0, 3 and 10.
        let changes = plan_role_changes(&ladder_rewards(), 12, &[], true);

        assert_eq!(changes.grant, vec![1000, 300, 100]);
        assert!(
            changes.revoke.is_empty(),
            "nothing above the level is worn, so nothing comes off"
        );
    }

    #[test]
    fn replacing_wears_only_the_highest_reached_rung() {
        // The same member, on a guild that does not stack: only level 10's role
        // stays, and the two lower ones they are wearing come off.
        let changes = plan_role_changes(&ladder_rewards(), 12, &[100, 300], false);

        assert_eq!(changes.grant, vec![1000]);
        assert_eq!(changes.revoke.len(), 2);
        assert!(changes.revoke.contains(&100));
        assert!(changes.revoke.contains(&300));
    }

    #[test]
    fn replacing_never_revokes_a_rung_the_member_is_not_wearing() {
        // An unworn lower rung is not work to do. Proposing the removal anyway
        // would make every reconcile call Discord for roles that were never there.
        let changes = plan_role_changes(&ladder_rewards(), 12, &[], false);

        assert_eq!(changes.grant, vec![1000]);
        assert!(
            changes.revoke.is_empty(),
            "no lower rung is held, so there is nothing to strip"
        );
    }

    #[test]
    fn a_role_already_worn_is_not_granted_again() {
        let changes = plan_role_changes(&ladder_rewards(), 12, &[300], true);

        assert!(!changes.grant.contains(&300), "already held");
        assert_eq!(changes.grant, vec![1000, 100]);
    }

    #[test]
    fn a_role_above_the_level_comes_off_unless_it_keeps_on_demote() {
        // Level 12, wearing the level-50 role, which does not survive a demote.
        let changes = plan_role_changes(&ladder_rewards(), 12, &[5000], true);

        assert_eq!(changes.revoke, vec![5000]);
        // The level-0 and level-10 rungs are still grants; only the replacement
        // case strips roles, and this guild stacks.
        assert_eq!(changes.grant, vec![1000, 300, 100]);
    }

    #[test]
    fn keep_on_demote_protects_against_the_demotion_rule_only() {
        // Every rung keeps on demote here, so nothing is ever stripped for being
        // above the member's level — even at level 0.
        let all_kept: Vec<LevelReward> = ladder_rewards()
            .into_iter()
            .map(|reward| LevelReward {
                keep_on_demote: true,
                ..reward
            })
            .collect();

        let changes = plan_role_changes(&all_kept, 0, &[5000], true);
        assert!(
            changes.revoke.is_empty(),
            "keep_on_demote shields a role from the demotion rule"
        );
    }

    #[test]
    fn a_member_below_every_rung_is_still_placed_by_the_level_zero_rung() {
        // Level 0 is a real rung — that is what makes Neophyte grantable at all.
        let changes = plan_role_changes(&ladder_rewards(), 0, &[], true);
        assert_eq!(changes.grant, vec![100]);
    }

    #[test]
    fn nothing_qualifies_when_the_level_is_below_every_rung() {
        let changes = plan_role_changes(&ladder_rewards(), -1, &[], true);

        assert!(changes.grant.is_empty());
        assert!(changes.is_empty());
    }

    #[test]
    fn an_empty_ladder_asks_for_nothing() {
        let changes = plan_role_changes(&[], 100, &[1, 2], true);

        assert!(changes.is_empty());
    }

    #[test]
    fn a_demotion_to_level_zero_revokes_the_rungs_above_it() {
        // Falling from 12 to 0 in a stacking guild: the level-0 rung goes back on
        // and the rungs above the new level come off.
        //
        // The fixture's level-3 and level-10 rungs keep on demote, so they have to
        // be configured to come off for this rule to be exercised at all. That is
        // the configuration an admin picks when they want level roles to track
        // standing rather than history, and it is the only one in which a demotion
        // changes anything.
        let falling: Vec<LevelReward> = ladder_rewards()
            .into_iter()
            .map(|reward| match reward.level {
                3 | 10 => LevelReward {
                    keep_on_demote: false,
                    ..reward
                },
                _ => reward,
            })
            .collect();

        let changes = plan_role_changes(&falling, 0, &[1000, 300], true);

        assert_eq!(changes.grant, vec![100]);
        assert_eq!(changes.revoke.len(), 2);
        assert!(changes.revoke.contains(&1000));
        assert!(changes.revoke.contains(&300));
    }
}
