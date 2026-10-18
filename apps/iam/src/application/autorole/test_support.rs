// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Hand-written fakes for the auto role use cases.
//!
//! The interesting rules live in the domain (`AutoRolePlan::build`,
//! `PendingGrant::record_failure`), and these fakes exist so the use cases that
//! wrap them can be exercised without a database or a Discord connection. The
//! clock is fake too: a due grant is a matter of arithmetic, not of sleeping.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::autorole::{AutoRole, AutoRoleRepository, AutoRoleTrigger, PendingGrant};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

/// Rules and the pending queue, in memory.
#[derive(Default)]
pub struct FakeAutoRoleStore {
    pub rules: Vec<AutoRole>,
    pub pending: Vec<PendingGrant>,
}

pub type Store = Arc<Mutex<FakeAutoRoleStore>>;

pub fn store() -> Store {
    Arc::new(Mutex::new(FakeAutoRoleStore::default()))
}

fn lock(store: &Store) -> std::sync::MutexGuard<'_, FakeAutoRoleStore> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A rule with sensible defaults.
pub fn rule(
    trigger: AutoRoleTrigger,
    delay_seconds: i32,
    role_id: u64,
    applies_to_bots: bool,
) -> AutoRole {
    let now = Utc::now();
    AutoRole {
        id: Uuid::new_v4(),
        guild_id: 1,
        role_id,
        trigger,
        delay_seconds,
        applies_to_bots,
        is_active: true,
        created_at: now,
        updated_at: now,
    }
}

pub fn seed_rules(store: &Store, rules: Vec<AutoRole>) {
    lock(store).rules.extend(rules);
}

/// A pending grant, due now unless a time is given.
pub fn grant(
    guild_id: u64,
    discord_user_id: u64,
    role_id: u64,
    due_at: DateTime<Utc>,
    attempts: i32,
) -> PendingGrant {
    PendingGrant {
        id: Uuid::new_v4(),
        guild_id,
        discord_user_id,
        auto_role_id: Uuid::new_v4(),
        role_id,
        due_at,
        attempts,
        last_error: None,
        created_at: Utc::now(),
    }
}

/// A clock frozen at a chosen instant, so "due" is decided by the test.
pub struct FakeClock {
    now: DateTime<Utc>,
}

impl FakeClock {
    pub fn at(now: DateTime<Utc>) -> Self {
        Self { now }
    }

    /// A fixed, readable instant: 2026-10-18T12:00:00Z.
    ///
    /// Built from a constant rather than a `TimeZone` call so the test fakes need
    /// no extra trait import.
    pub fn fixed() -> Self {
        let now = DateTime::from_timestamp(1_792_406_400, 0).expect("that instant exists");
        Self::at(now)
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    async fn sleep_until(&self, _deadline: DateTime<Utc>) {
        // A test never waits: the clock is the thing under control, not the timer.
    }
}

#[derive(Clone)]
pub struct FakeAutoRoles {
    store: Store,
}

impl FakeAutoRoles {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// The queue, for assertions.
    pub fn pending(&self) -> Vec<PendingGrant> {
        lock(&self.store).pending.clone()
    }
}

impl AutoRoleRepository for FakeAutoRoles {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<AutoRole>, RepositoryError> {
        Ok(lock(&self.store)
            .rules
            .iter()
            .find(|rule| rule.id == id)
            .cloned())
    }

    async fn list_for_guild(&self, guild_id: u64) -> Result<Vec<AutoRole>, RepositoryError> {
        Ok(lock(&self.store)
            .rules
            .iter()
            .filter(|rule| rule.guild_id == guild_id)
            .cloned()
            .collect())
    }

    async fn list(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<AutoRole>, RepositoryError> {
        let rules: Vec<AutoRole> = lock(&self.store)
            .rules
            .iter()
            .filter(|rule| rule.guild_id == guild_id)
            .cloned()
            .collect();
        let total = rules.len() as u64;
        Ok(Page::new(rules, params, total))
    }

    async fn create(&self, rule: AutoRole) -> Result<AutoRole, RepositoryError> {
        lock(&self.store).rules.push(rule.clone());
        Ok(rule)
    }

    async fn update(
        &self,
        id: Uuid,
        trigger: Option<AutoRoleTrigger>,
        delay_seconds: Option<i32>,
        applies_to_bots: Option<bool>,
        is_active: Option<bool>,
    ) -> Result<AutoRole, RepositoryError> {
        let mut state = lock(&self.store);
        let rule = state
            .rules
            .iter_mut()
            .find(|rule| rule.id == id)
            .ok_or_else(|| RepositoryError::not_found("auto_role"))?;
        if let Some(trigger) = trigger {
            rule.trigger = trigger;
        }
        if let Some(delay_seconds) = delay_seconds {
            rule.delay_seconds = delay_seconds;
        }
        if let Some(applies_to_bots) = applies_to_bots {
            rule.applies_to_bots = applies_to_bots;
        }
        if let Some(is_active) = is_active {
            rule.is_active = is_active;
        }
        rule.updated_at = Utc::now();
        Ok(rule.clone())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut state = lock(&self.store);
        let before = state.rules.len();
        state.rules.retain(|rule| rule.id != id);
        if state.rules.len() == before {
            return Err(RepositoryError::not_found("auto_role"));
        }
        Ok(())
    }

    async fn enqueue_pending(
        &self,
        grant: PendingGrant,
    ) -> Result<Option<PendingGrant>, RepositoryError> {
        let mut state = lock(&self.store);
        // The (rule, member) unique index, in miniature.
        if state.pending.iter().any(|queued| {
            queued.auto_role_id == grant.auto_role_id
                && queued.discord_user_id == grant.discord_user_id
        }) {
            return Ok(None);
        }
        state.pending.push(grant.clone());
        Ok(Some(grant))
    }

    async fn due_pending(
        &self,
        now: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<PendingGrant>, RepositoryError> {
        let mut due: Vec<PendingGrant> = lock(&self.store)
            .pending
            .iter()
            .filter(|grant| grant.due_at <= now)
            .cloned()
            .collect();
        due.sort_by_key(|grant| grant.due_at);
        due.truncate(limit as usize);
        Ok(due)
    }

    async fn record_attempt(
        &self,
        id: Uuid,
        error: Option<&str>,
        next_due_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError> {
        let mut state = lock(&self.store);
        let grant = state
            .pending
            .iter_mut()
            .find(|grant| grant.id == id)
            .ok_or_else(|| RepositoryError::not_found("pending_grant"))?;
        grant.attempts += 1;
        if let Some(error) = error {
            grant.last_error = Some(error.to_string());
        }
        if let Some(next_due_at) = next_due_at {
            grant.due_at = next_due_at;
        }
        Ok(())
    }

    async fn delete_pending(&self, id: Uuid) -> Result<(), RepositoryError> {
        lock(&self.store).pending.retain(|grant| grant.id != id);
        Ok(())
    }

    async fn count_pending_for_user(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<u64, RepositoryError> {
        Ok(lock(&self.store)
            .pending
            .iter()
            .filter(|grant| grant.guild_id == guild_id && grant.discord_user_id == discord_user_id)
            .count() as u64)
    }
}
