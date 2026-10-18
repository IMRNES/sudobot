use std::future::Future;
use uuid::Uuid;

use super::entity::{GuildMember, GuildMemberWithRole, NewGuildMember};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

pub trait GuildMemberRepository: Send + Sync {
    fn find(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<GuildMember>, RepositoryError>> + Send;

    /// Membership plus the role it points at, for the dashboard's member list.
    fn find_with_role(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Option<GuildMemberWithRole>, RepositoryError>> + Send;

    fn list_for_guild(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<GuildMemberWithRole>, RepositoryError>> + Send;

    /// Every guild a user belongs to, used to build the guild picker at login.
    fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> impl Future<Output = Result<Vec<GuildMemberWithRole>, RepositoryError>> + Send;

    fn create(
        &self,
        member: NewGuildMember,
    ) -> impl Future<Output = Result<GuildMember, RepositoryError>> + Send;

    /// Inserts or refreshes a membership from a Discord login.
    ///
    /// Rejoining a guild, or logging in again after a permission change, must
    /// update the existing row instead of failing on the unique pair.
    fn upsert_from_discord(
        &self,
        member: NewGuildMember,
    ) -> impl Future<Output = Result<GuildMember, RepositoryError>> + Send;

    fn update_role(
        &self,
        guild_id: u64,
        user_id: Uuid,
        guild_role_id: Uuid,
    ) -> impl Future<Output = Result<GuildMember, RepositoryError>> + Send;

    fn delete(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// How many members in this guild hold a role whose matrix includes
    /// `permission`. Guards the "do not lock everyone out" rule.
    fn count_holders_of(
        &self,
        guild_id: u64,
        permission: &str,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}
