use std::future::Future;

use super::entity::{Guild, GuildPatch, NewGuild};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

pub trait GuildRepository: Send + Sync {
    fn find_by_id(
        &self,
        id: u64,
    ) -> impl Future<Output = Result<Option<Guild>, RepositoryError>> + Send;

    /// Every guild the bot is currently installed in.
    fn list_active(&self) -> impl Future<Output = Result<Vec<Guild>, RepositoryError>> + Send;

    fn list(
        &self,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<Guild>, RepositoryError>> + Send;

    /// Inserts the guild, or returns the existing row when the bot re-joins.
    ///
    /// Upsert rather than create-then-conflict: a rejoin is the normal path, not
    /// an error, and `bot_present` must flip back to true.
    fn upsert(
        &self,
        guild: NewGuild,
    ) -> impl Future<Output = Result<Guild, RepositoryError>> + Send;

    fn update(
        &self,
        id: u64,
        patch: GuildPatch,
    ) -> impl Future<Output = Result<Guild, RepositoryError>> + Send;

    fn delete(&self, id: u64) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn count(&self) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}
