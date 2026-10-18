use std::future::Future;
use uuid::Uuid;

use super::entity::{AutoRole, PendingGrant};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

pub trait AutoRoleRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<AutoRole>, RepositoryError>> + Send;

    fn list_for_guild(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<Vec<AutoRole>, RepositoryError>> + Send;

    fn list(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<AutoRole>, RepositoryError>> + Send;

    fn create(
        &self,
        rule: AutoRole,
    ) -> impl Future<Output = Result<AutoRole, RepositoryError>> + Send;

    fn update(
        &self,
        id: Uuid,
        trigger: Option<super::entity::AutoRoleTrigger>,
        delay_seconds: Option<i32>,
        applies_to_bots: Option<bool>,
        is_active: Option<bool>,
    ) -> impl Future<Output = Result<AutoRole, RepositoryError>> + Send;

    fn delete(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Queues a delayed grant, ignoring a repeat for the same rule and member.
    ///
    /// Idempotent because a rejoin must not stack grants — `ON CONFLICT DO NOTHING`
    /// on the (rule, member) unique index.
    fn enqueue_pending(
        &self,
        grant: PendingGrant,
    ) -> impl Future<Output = Result<Option<PendingGrant>, RepositoryError>> + Send;

    /// Grants that are due, oldest first, capped so one sweep cannot run forever.
    fn due_pending(
        &self,
        now: chrono::DateTime<chrono::Utc>,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<PendingGrant>, RepositoryError>> + Send;

    /// Records an attempt: reschedules with backoff, or removes it when the
    /// retries are exhausted.
    fn record_attempt(
        &self,
        id: Uuid,
        error: Option<&str>,
        next_due_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn delete_pending(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn count_pending_for_user(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}
