use chrono::{DateTime, Utc};
use uuid::Uuid;

/// When a role is granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoRoleTrigger {
    /// Immediately on join.
    Join,
    /// After `delay_seconds` of continued membership.
    Delay,
    /// Once the member passes Discord's "not a bot" screening.
    Human,
}

impl AutoRoleTrigger {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "join" => Some(Self::Join),
            "delay" => Some(Self::Delay),
            "human" => Some(Self::Human),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Delay => "delay",
            Self::Human => "human",
        }
    }
}

/// One rule: "grant this role, this way".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoRole {
    pub id: Uuid,
    pub guild_id: u64,
    pub role_id: u64,
    pub trigger: AutoRoleTrigger,
    pub delay_seconds: i32,
    pub applies_to_bots: bool,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AutoRole {
    /// The maximum delay a rule may specify: one week.
    ///
    /// Bounded because the pending queue is swept frequently, and a rule measured
    /// in months is really a different feature.
    pub const MAX_DELAY_SECONDS: i32 = 604_800;

    /// Whether this rule applies to a joining member.
    pub fn applies_to(&self, is_bot: bool) -> bool {
        self.is_active && (self.applies_to_bots || !is_bot)
    }

    /// The moment a grant for this member comes due.
    pub fn due_at(&self, joined_at: DateTime<Utc>) -> DateTime<Utc> {
        match self.trigger {
            AutoRoleTrigger::Delay => {
                joined_at + chrono::Duration::seconds(i64::from(self.delay_seconds))
            }
            // Immediate triggers are due now; the sweep picks them up on its next
            // pass, which keeps one code path for every rule type.
            AutoRoleTrigger::Join | AutoRoleTrigger::Human => joined_at,
        }
    }
}

/// A grant waiting to happen.
///
/// Persisted rather than held in memory so a bot restart does not lose the queue —
/// which is the whole point of delayed autoroles as an anti-raid measure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingGrant {
    pub id: Uuid,
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub auto_role_id: Uuid,
    pub role_id: u64,
    pub due_at: DateTime<Utc>,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PendingGrant {
    /// How many times a failing grant is retried before being dropped.
    ///
    /// Discord returns permission errors for a role the bot cannot manage; retrying
    /// that forever would fill the queue with permanently stuck rows.
    pub const MAX_ATTEMPTS: i32 = 5;

    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        self.due_at <= now
    }

    pub fn has_attempts_left(&self) -> bool {
        self.attempts < Self::MAX_ATTEMPTS
    }

    /// The state after a failed attempt.
    pub fn record_failure(&self, error: impl Into<String>, now: DateTime<Utc>) -> Self {
        let mut next = self.clone();
        next.attempts += 1;
        next.last_error = Some(error.into());
        // Back off proportionally to how often it has failed, so a guild-wide
        // permission problem does not hammer the API.
        next.due_at = now + chrono::Duration::seconds(i64::from(next.attempts) * 60);
        next
    }
}

/// What the bot decides to do for one joining member.
#[derive(Debug, Clone)]
pub struct AutoRolePlan {
    /// Roles to add straight away.
    pub immediate: Vec<u64>,
    /// Rules to queue for later, with the moment they come due.
    pub deferred: Vec<(Uuid, u64, DateTime<Utc>)>,
}

impl AutoRolePlan {
    pub fn is_empty(&self) -> bool {
        self.immediate.is_empty() && self.deferred.is_empty()
    }

    /// Builds the plan for one join event.
    ///
    /// Pure: it reads rules and a join time and returns a decision, so the whole
    /// trigger matrix is testable without a Discord connection.
    pub fn build(rules: &[AutoRole], is_bot: bool, joined_at: DateTime<Utc>) -> Self {
        let mut immediate = Vec::new();
        let mut deferred = Vec::new();

        for rule in rules.iter().filter(|rule| rule.applies_to(is_bot)) {
            match rule.trigger {
                AutoRoleTrigger::Delay if rule.delay_seconds > 0 => {
                    deferred.push((rule.id, rule.role_id, rule.due_at(joined_at)));
                }
                _ => immediate.push(rule.role_id),
            }
        }

        immediate.sort_unstable();
        immediate.dedup();
        Self {
            immediate,
            deferred,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn rule(trigger: AutoRoleTrigger, delay: i32, applies_to_bots: bool) -> AutoRole {
        let now = Utc::now();
        AutoRole {
            id: Uuid::new_v4(),
            guild_id: 1,
            role_id: 42,
            trigger,
            delay_seconds: delay,
            applies_to_bots,
            is_active: true,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn join_rules_are_granted_immediately() {
        let plan = AutoRolePlan::build(&[rule(AutoRoleTrigger::Join, 0, false)], false, Utc::now());
        assert_eq!(plan.immediate, vec![42]);
        assert!(plan.deferred.is_empty());
    }

    #[test]
    fn delayed_rules_are_queued() {
        let joined = Utc::now();
        let plan = AutoRolePlan::build(&[rule(AutoRoleTrigger::Delay, 600, false)], false, joined);
        assert!(plan.immediate.is_empty());
        assert_eq!(plan.deferred.len(), 1);
        assert_eq!(plan.deferred[0].2, joined + chrono::Duration::seconds(600));
    }

    #[test]
    fn bot_rules_skip_humans_and_vice_versa() {
        let rules = [rule(AutoRoleTrigger::Join, 0, false)];
        assert!(AutoRolePlan::build(&rules, true, Utc::now()).is_empty());
        assert!(!AutoRolePlan::build(&rules, false, Utc::now()).is_empty());
    }

    #[test]
    fn a_zero_delay_rule_granting_now_is_not_queued() {
        // Delay(0) is indistinguishable from Join, and queueing it would just add
        // a sweep cycle of latency.
        let plan =
            AutoRolePlan::build(&[rule(AutoRoleTrigger::Delay, 0, false)], false, Utc::now());
        assert_eq!(plan.immediate, vec![42]);
    }

    #[test]
    fn failures_back_off_and_eventually_stop() {
        let now = Utc::now();
        let mut grant = PendingGrant {
            id: Uuid::new_v4(),
            guild_id: 1,
            discord_user_id: 2,
            auto_role_id: Uuid::new_v4(),
            role_id: 42,
            due_at: now,
            attempts: 0,
            last_error: None,
            created_at: now,
        };

        for _ in 0..PendingGrant::MAX_ATTEMPTS {
            assert!(grant.has_attempts_left());
            grant = grant.record_failure("missing permissions", now);
        }
        assert!(!grant.has_attempts_left());
        assert!(grant.due_at > now);
    }
}
