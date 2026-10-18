use tracing::{info, warn};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::autorole::{AutoRoleError, AutoRoleRepository, PendingGrant};

/// The grants one sweep pass should attempt.
#[derive(Debug, Clone)]
pub struct DueGrants {
    pub grants: Vec<PendingGrant>,
}

/// The sweeper: finds delayed grants that have come due and reports their fate.
///
/// The caller applies each grant in Discord and then calls [`Self::complete`] or
/// [`Self::fail`]; this use case owns the queue's bookkeeping, not the Discord
/// call, so the retry policy is testable without a gateway connection.
pub struct SweepPendingGrantsUseCase<A, C> {
    auto_roles: A,
    clock: C,
}

impl<A: AutoRoleRepository, C: Clock> SweepPendingGrantsUseCase<A, C> {
    pub fn new(auto_roles: A, clock: C) -> Self {
        Self { auto_roles, clock }
    }

    /// Reads the grants that are due.
    ///
    /// The repository's `due_pending` already filters on `due_at`, and
    /// `PendingGrant::is_due` is applied again here: the clock is the authority on
    /// what "due" means, and a row read a moment before the deadline (or written
    /// by a process with a slightly different clock) must not be acted on early.
    pub async fn execute(&self, limit: u32) -> Result<Vec<PendingGrant>, AutoRoleError> {
        let now = self.clock.now();

        let due: Vec<PendingGrant> = self
            .auto_roles
            .due_pending(now, limit)
            .await?
            .into_iter()
            .filter(|grant| grant.is_due(now))
            .collect();

        if !due.is_empty() {
            info!(due = due.len(), "auto role grants are due");
        }

        Ok(due)
    }

    /// The same read, grouped, for callers that prefer the named type.
    pub async fn due(&self, limit: u32) -> Result<DueGrants, AutoRoleError> {
        Ok(DueGrants {
            grants: self.execute(limit).await?,
        })
    }

    /// The grant was applied in Discord; remove it from the queue.
    pub async fn complete(&self, id: Uuid) -> Result<(), AutoRoleError> {
        self.auto_roles.delete_pending(id).await?;
        info!(grant_id = %id, "auto role grant completed");
        Ok(())
    }

    /// The grant failed; back off, or drop it when the retries are exhausted.
    ///
    /// Returns whether the grant was **dropped**, so the caller can log the
    /// give-up once instead of on every sweep — by the fifth failure the row is
    /// gone and there is nothing left to report.
    pub async fn fail(&self, grant: &PendingGrant, error: &str) -> Result<bool, AutoRoleError> {
        let now = self.clock.now();

        // `record_failure` is the domain's backoff: it increments the attempt
        // count, keeps the message, and pushes `due_at` out proportionally so a
        // guild-wide permission problem does not hammer the API.
        let next = grant.record_failure(error, now);
        let dropped = !next.has_attempts_left();

        if dropped {
            self.auto_roles.delete_pending(next.id).await?;
            warn!(
                grant_id = %next.id,
                guild_id = grant.guild_id,
                role_id = grant.role_id,
                attempts = next.attempts,
                error,
                "auto role grant dropped after exhausting its retries"
            );
        } else {
            self.auto_roles
                .record_attempt(next.id, Some(error), Some(next.due_at))
                .await?;
            warn!(
                grant_id = %next.id,
                guild_id = grant.guild_id,
                role_id = grant.role_id,
                attempts = next.attempts,
                error,
                "auto role grant failed; rescheduled"
            );
        }

        Ok(dropped)
    }
}
