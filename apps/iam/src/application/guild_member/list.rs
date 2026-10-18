use crate::domain::errors::{Page, PaginationParams};
use crate::domain::guild_member::{GuildMemberError, GuildMemberRepository, GuildMemberWithRole};

/// Lists one guild's dashboard memberships, each with the role it points at.
///
/// The role travels with the row because the dashboard renders its name beside
/// every member; fetching it per row is the N+1 the repository's join exists to
/// avoid.
pub struct ListGuildMembersUseCase<M> {
    members: M,
}

impl<M: GuildMemberRepository> ListGuildMembersUseCase<M> {
    pub fn new(members: M) -> Self {
        Self { members }
    }

    pub async fn execute(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<GuildMemberWithRole>, GuildMemberError> {
        Ok(self.members.list_for_guild(guild_id, params).await?)
    }
}
