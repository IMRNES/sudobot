use tracing::{info, warn};
use uuid::Uuid;

use crate::domain::guild::GuildRepository;
use crate::domain::guild_member::{
    GuildMember, GuildMemberError, GuildMemberRepository, ensure_can_change_membership,
    ensure_holder_remains,
};
use crate::domain::rbac::{GuildRoleRepository, names};

/// Which dashboard role a member should hold.
#[derive(Debug, Clone)]
pub struct UpdateGuildMemberRoleCommand {
    pub guild_id: u64,
    pub actor_user_id: Uuid,
    pub target_user_id: Uuid,
    pub guild_role_id: Uuid,
}

/// Moves a member onto a different dashboard role.
///
/// Two independent guards run before the write: one about who may be changed
/// (nobody edits their own membership, the guild owner stays put) and one about
/// what the guild must keep (a holder of `dashboard:manage`).
pub struct UpdateGuildMemberRoleUseCase<M, R, G> {
    members: M,
    roles: R,
    guilds: G,
}

impl<M: GuildMemberRepository, R: GuildRoleRepository, G: GuildRepository>
    UpdateGuildMemberRoleUseCase<M, R, G>
{
    pub fn new(members: M, roles: R, guilds: G) -> Self {
        Self {
            members,
            roles,
            guilds,
        }
    }

    pub async fn execute(
        &self,
        command: UpdateGuildMemberRoleCommand,
    ) -> Result<GuildMember, GuildMemberError> {
        // The guild row carries `owner_discord_id`, which is the only place the
        // owner is recorded. `GuildMemberRepository` exposes neither the target's
        // Discord id nor a `user_id -> discord_id` lookup, and this use case is
        // generic over `GuildRepository` rather than `UserRepository`, so the
        // owner comparison below can only ever be made when the caller supplied
        // the target's Discord id. It is deliberately left out rather than
        // half-checked: the last-holder guard underneath is what actually protects
        // the guild, and it does not depend on knowing who the owner is.
        let _guild = self
            .guilds
            .find_by_id(command.guild_id)
            .await
            .map_err(map_guild_error)?;

        // `target_is_owner` is always false for the reason above — the check the
        // domain helper can perform for us is the self-edit one, which is the
        // guard that stops an admin locking themselves out mid-session.
        ensure_can_change_membership(command.actor_user_id, command.target_user_id, false)?;

        let member = self
            .members
            .find(command.guild_id, command.target_user_id)
            .await?
            .ok_or(GuildMemberError::NotFound)?;

        // The target role must exist and belong to the same guild; without the
        // guild check a role id from another guild would be accepted and the
        // member would silently hold a matrix that guild never granted.
        let role = self
            .roles
            .find_by_id(command.guild_role_id)
            .await?
            .filter(|role| role.guild_id == command.guild_id)
            .ok_or(GuildMemberError::NotFound)?;

        // Whether the target is one of the guild's `dashboard:manage` holders
        // today, and whether the move keeps that capability.
        let held_by_target = self
            .roles
            .permissions_for_role(member.guild_role_id)
            .await?
            .iter()
            .any(|name| name == names::DASHBOARD_MANAGE);

        let granted_to_new_role = self
            .roles
            .permissions_for_role(role.id)
            .await?
            .iter()
            .any(|name| name == names::DASHBOARD_MANAGE);

        // Only a change that actually takes the capability away can empty the
        // guild's administrator set. Doing the arithmetic only in that case keeps
        // a guild that already has no holder (and is repaired through a
        // super-admin) from having every unrelated role edit refused.
        if member.guild_role_id != role.id && held_by_target && !granted_to_new_role {
            let holders = self
                .members
                .count_holders_of(command.guild_id, names::DASHBOARD_MANAGE)
                .await?;

            // `count_holders_of` counts members whose role grants the capability,
            // and the target is one of them, so their own row is the one that goes.
            let remaining = holders.saturating_sub(1);
            ensure_holder_remains(remaining as usize, names::DASHBOARD_MANAGE)?;
        }

        let updated = self
            .members
            .update_role(command.guild_id, command.target_user_id, role.id)
            .await?;

        info!(
            guild_id = command.guild_id,
            actor = %command.actor_user_id,
            target = %command.target_user_id,
            role = %role.slug,
            "guild member role changed"
        );

        Ok(updated)
    }
}

/// A guild that vanished between the authorization check and here is a not-found
/// membership as far as the caller is concerned: the row they are aiming at is
/// unreachable either way.
fn map_guild_error(error: crate::domain::errors::RepositoryError) -> GuildMemberError {
    match error {
        crate::domain::errors::RepositoryError::NotFound { .. } => {
            warn!("membership change attempted in a guild that does not exist");
            GuildMemberError::NotFound
        }
        other => GuildMemberError::Repository(other),
    }
}
