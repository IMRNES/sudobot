// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use chrono::{Duration, Utc};

use super::join::{PlanJoinAutoRolesCommand, PlanJoinAutoRolesUseCase};
use super::rules::{CreateAutoRoleCommand, CreateAutoRoleUseCase, DeleteAutoRoleUseCase};
use super::sweep::SweepPendingGrantsUseCase;
use super::test_support::{FakeAutoRoles, FakeClock, grant, rule, seed_rules, store};
use crate::domain::autorole::{AutoRoleError, AutoRoleTrigger};

const GUILD: u64 = 1;
const MEMBER: u64 = 555;

// --- the zero-delay case (the one that is easy to get wrong) ----------------

#[tokio::test]
async fn a_zero_delay_rule_grants_immediately_and_queues_nothing() {
    let store = store();
    // `delay` with a zero wait is indistinguishable from `join`; queueing it would
    // only add a sweep cycle of latency.
    seed_rules(&store, vec![rule(AutoRoleTrigger::Delay, 0, 42, false)]);
    let auto_roles = FakeAutoRoles::new(store.clone());

    let use_case = PlanJoinAutoRolesUseCase::new(auto_roles.clone(), FakeClock::fixed());

    let plan = use_case
        .execute(PlanJoinAutoRolesCommand {
            guild_id: GUILD,
            discord_user_id: MEMBER,
            is_bot: false,
            joined_at: Utc::now(),
        })
        .await
        .expect("a join never fails");

    assert_eq!(plan.immediate, vec![42]);
    assert!(plan.deferred.is_empty());
    assert!(
        auto_roles.pending().is_empty(),
        "a zero-delay grant must not enter the queue"
    );
}

#[tokio::test]
async fn a_genuinely_delayed_rule_is_queued_and_not_granted_yet() {
    let store = store();
    let joined = Utc::now();
    seed_rules(&store, vec![rule(AutoRoleTrigger::Delay, 600, 42, false)]);
    let auto_roles = FakeAutoRoles::new(store.clone());

    let use_case = PlanJoinAutoRolesUseCase::new(auto_roles.clone(), FakeClock::fixed());

    let plan = use_case
        .execute(PlanJoinAutoRolesCommand {
            guild_id: GUILD,
            discord_user_id: MEMBER,
            is_bot: false,
            joined_at: joined,
        })
        .await
        .expect("a join never fails");

    assert!(plan.immediate.is_empty());
    assert_eq!(plan.deferred.len(), 1);

    let queued = auto_roles.pending();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].role_id, 42);
    assert_eq!(queued[0].attempts, 0);
    assert!(queued[0].due_at > joined);
}

#[tokio::test]
async fn a_join_rule_is_granted_immediately() {
    let store = store();
    seed_rules(&store, vec![rule(AutoRoleTrigger::Join, 0, 7, false)]);
    let use_case =
        PlanJoinAutoRolesUseCase::new(FakeAutoRoles::new(store.clone()), FakeClock::fixed());

    let plan = use_case
        .execute(PlanJoinAutoRolesCommand {
            guild_id: GUILD,
            discord_user_id: MEMBER,
            is_bot: false,
            joined_at: Utc::now(),
        })
        .await
        .expect("a join never fails");

    assert_eq!(plan.immediate, vec![7]);
    assert!(
        store
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .pending
            .is_empty()
    );
}

#[tokio::test]
async fn a_bot_joining_does_not_get_a_human_only_role() {
    let store = store();
    seed_rules(&store, vec![rule(AutoRoleTrigger::Join, 0, 7, false)]);
    let use_case =
        PlanJoinAutoRolesUseCase::new(FakeAutoRoles::new(store.clone()), FakeClock::fixed());

    let plan = use_case
        .execute(PlanJoinAutoRolesCommand {
            guild_id: GUILD,
            discord_user_id: MEMBER,
            is_bot: true,
            joined_at: Utc::now(),
        })
        .await
        .expect("a join never fails");

    assert!(plan.is_empty());
}

#[tokio::test]
async fn rejoining_does_not_queue_a_second_grant() {
    let store = store();
    let joined = Utc::now();
    seed_rules(&store, vec![rule(AutoRoleTrigger::Delay, 600, 42, false)]);
    let auto_roles = FakeAutoRoles::new(store.clone());
    let use_case = PlanJoinAutoRolesUseCase::new(auto_roles.clone(), FakeClock::fixed());

    let command = || PlanJoinAutoRolesCommand {
        guild_id: GUILD,
        discord_user_id: MEMBER,
        is_bot: false,
        joined_at: joined,
    };

    use_case
        .execute(command())
        .await
        .expect("a join never fails");
    use_case
        .execute(command())
        .await
        .expect("a rejoin never fails");

    assert_eq!(
        auto_roles.pending().len(),
        1,
        "the (rule, member) pair is queued once"
    );
}

// --- the sweeper ------------------------------------------------------------

#[tokio::test]
async fn the_sweep_reads_only_grants_that_are_due() {
    let now = Utc::now();
    let store = store();
    {
        let mut state = store.lock().unwrap_or_else(|p| p.into_inner());
        state
            .pending
            .push(grant(GUILD, MEMBER, 1, now - Duration::seconds(30), 0));
        state
            .pending
            .push(grant(GUILD, MEMBER, 2, now + Duration::seconds(300), 0));
    }

    let use_case = SweepPendingGrantsUseCase::new(FakeAutoRoles::new(store), FakeClock::at(now));

    let due = use_case.execute(10).await.expect("the queue is readable");

    assert_eq!(due.len(), 1);
    assert_eq!(due[0].role_id, 1);
}

#[tokio::test]
async fn completing_a_grant_removes_it_from_the_queue() {
    let now = Utc::now();
    let store = store();
    let pending = grant(GUILD, MEMBER, 1, now, 0);
    let id = pending.id;
    store
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .pending
        .push(pending);

    let auto_roles = FakeAutoRoles::new(store);
    let use_case = SweepPendingGrantsUseCase::new(auto_roles.clone(), FakeClock::at(now));

    use_case.complete(id).await.expect("the row exists");

    assert!(auto_roles.pending().is_empty());
}

#[tokio::test]
async fn a_failure_backs_off_and_the_last_one_drops_the_grant() {
    let now = Utc::now();
    let store = store();
    let pending = grant(GUILD, MEMBER, 1, now, 0);
    store
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .pending
        .push(pending.clone());

    let auto_roles = FakeAutoRoles::new(store.clone());
    let use_case = SweepPendingGrantsUseCase::new(auto_roles.clone(), FakeClock::at(now));

    let dropped = use_case
        .fail(&pending, "missing access")
        .await
        .expect("recording a failure succeeds");

    assert!(!dropped, "there are retries left");
    let queued = auto_roles.pending();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].attempts, 1);
    assert_eq!(queued[0].last_error.as_deref(), Some("missing access"));
    assert!(
        queued[0].due_at > now,
        "the retry was pushed into the future"
    );

    // Walk the remaining attempts; the last one gives up.
    let mut current = queued[0].clone();
    let mut drops = 0;
    for _ in 1..crate::domain::autorole::PendingGrant::MAX_ATTEMPTS {
        let dropped = use_case
            .fail(&current, "missing access")
            .await
            .expect("recording a failure succeeds");
        if dropped {
            drops += 1;
            break;
        }
        current = auto_roles
            .pending()
            .into_iter()
            .find(|grant| grant.id == current.id)
            .expect("the grant is still queued");
    }

    assert_eq!(drops, 1, "the grant must eventually be given up on");
    assert!(
        auto_roles.pending().is_empty(),
        "an exhausted grant is dropped, not retried forever"
    );
}

// --- the dashboard's rule editing -------------------------------------------

#[tokio::test]
async fn creating_a_rule_validates_the_trigger_and_the_delay() {
    let store = store();
    let use_case = CreateAutoRoleUseCase::new(FakeAutoRoles::new(store));

    let error = use_case
        .execute(CreateAutoRoleCommand {
            guild_id: GUILD,
            role_id: 42,
            trigger: "whenever".to_string(),
            delay_seconds: 0,
            applies_to_bots: false,
        })
        .await
        .expect_err("only join, delay and human exist");

    assert!(matches!(error, AutoRoleError::UnknownTrigger(trigger) if trigger == "whenever"));

    let error = use_case
        .execute(CreateAutoRoleCommand {
            guild_id: GUILD,
            role_id: 42,
            trigger: "delay".to_string(),
            delay_seconds: i32::MAX,
            applies_to_bots: false,
        })
        .await
        .expect_err("a month-long delay is a different feature");

    assert!(matches!(error, AutoRoleError::InvalidDelay));
}

#[tokio::test]
async fn a_role_cannot_be_granted_automatically_twice() {
    let store = store();
    seed_rules(&store, vec![rule(AutoRoleTrigger::Join, 0, 42, false)]);
    let use_case = CreateAutoRoleUseCase::new(FakeAutoRoles::new(store));

    let error = use_case
        .execute(CreateAutoRoleCommand {
            guild_id: GUILD,
            role_id: 42,
            trigger: "join".to_string(),
            delay_seconds: 0,
            applies_to_bots: false,
        })
        .await
        .expect_err("two rules for one role would race on every join");

    assert!(matches!(error, AutoRoleError::RoleAlreadyConfigured(42)));
}

#[tokio::test]
async fn deleting_a_rule_that_is_already_gone_is_not_found() {
    let store = store();
    let use_case = DeleteAutoRoleUseCase::new(FakeAutoRoles::new(store));

    let error = use_case
        .execute(uuid::Uuid::new_v4())
        .await
        .expect_err("a missing rule is a 404, not a silent success");

    assert!(matches!(error, AutoRoleError::NotFound(_)));
}
