use tracing::info;
use uuid::Uuid;

use crate::domain::guild_member::{
    GuildMemberError, GuildMemberRepository, ensure_can_change_membership, ensure_holder_remains,
};
use crate::domain::rbac::{GuildRoleRepository, names};

/// Revokes a member's dashboard access.
///
/// The same two guards as a role change, minus the role check — there is no new
/// role to validate, but removing somebody who is the guild's last administrator
/// is exactly the mistake the holder guard exists to catch.
pub struct RemoveGuildMemberUseCase<M, R> {
    members: M,
    roles: R,
}

impl<M: GuildMemberRepository, R: GuildRoleRepository> RemoveGuildMemberUseCase<M, R> {
    pub fn new(members: M, roles: R) -> Self {
        Self { members, roles }
    }

    pub async fn execute(
        &self,
        guild_id: u64,
        actor_user_id: Uuid,
        target_user_id: Uuid,
    ) -> Result<(), GuildMemberError> {
        // As in the role change, the guild-owner half of the rule cannot be
        // evaluated here: neither this use case's ports nor
        // `GuildMemberRepository` expose the target's Discord id, so there is
        // nothing to compare against `Guild.owner_discord_id`. The self-removal
        // guard and the last-holder guard below are the two that can be enforced,
        // and the second is what keeps the guild reachable.
        ensure_can_change_membership(actor_user_id, target_user_id, false)?;

        let member = self
            .members
            .find(guild_id, target_user_id)
            .await?
            .ok_or(GuildMemberError::NotFound)?;

        let held_by_target = self
            .roles
            .permissions_for_role(member.guild_role_id)
            .await?
            .iter()
            .any(|name| name == names::DASHBOARD_MANAGE);

        if held_by_target {
            let holders = self
                .members
                .count_holders_of(guild_id, names::DASHBOARD_MANAGE)
                .await?;

            // The row being deleted is one of those holders.
            let remaining = holders.saturating_sub(1);
            ensure_holder_remains(remaining as usize, names::DASHBOARD_MANAGE)?;
        }

        self.members.delete(guild_id, target_user_id).await?;

        info!(
            guild_id,
            actor = %actor_user_id,
            target = %target_user_id,
            "guild member removed"
        );

        Ok(())
    }
}
