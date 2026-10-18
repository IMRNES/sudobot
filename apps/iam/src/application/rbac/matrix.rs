use tracing::info;
use uuid::Uuid;

use super::permissions::load_role;
use super::roles::{
    RoleView, count_members_in_role, ensure_manage_survives, validate_permission_names,
};
use crate::domain::guild_member::GuildMemberRepository;
use crate::domain::rbac::{GuildRoleRepository, RbacError, names};

/// A role's new permission matrix, submitted as a whole.
#[derive(Debug, Clone)]
pub struct ReplaceRolePermissionsCommand {
    pub guild_id: u64,
    pub role_id: Uuid,
    pub permissions: Vec<String>,
}

/// Replaces a role's matrix.
///
/// This is the use case that stops an administrator from unchecking the last
/// capability and losing the guild: if the new matrix drops `dashboard:manage`
/// and nothing else in the guild keeps it, the edit is refused by the domain's
/// `ensure_guild_stays_administrable`.
pub struct ReplaceRolePermissionsUseCase<R, M> {
    roles: R,
    members: M,
}

impl<R: GuildRoleRepository, M: GuildMemberRepository> ReplaceRolePermissionsUseCase<R, M> {
    pub fn new(roles: R, members: M) -> Self {
        Self { roles, members }
    }

    /// Replaces rather than diffs: the dashboard submits the matrix as a whole,
    /// and comparing it against server state invites a lost update.
    pub async fn execute(
        &self,
        command: ReplaceRolePermissionsCommand,
    ) -> Result<RoleView, RbacError> {
        let role = load_role(&self.roles, command.guild_id, command.role_id).await?;

        // 1. Every name must exist in the catalog. `PermissionRepository` is not
        //    among this use case's ports, so the catalog constant is the source of
        //    truth — the same list bootstrap seeds, and the same one the
        //    repository would have returned.
        let permissions = validate_permission_names(&command.permissions)?;

        // 2. The guard, and only when the edit actually removes the capability:
        //    granting permissions, or saving a matrix that never held
        //    `dashboard:manage`, cannot leave the guild unadministrable, and those
        //    are the common case.
        let currently_granted = self
            .roles
            .permissions_for_role(role.id)
            .await?
            .iter()
            .any(|name| name == names::DASHBOARD_MANAGE);

        if currently_granted {
            ensure_manage_survives(&self.members, command.guild_id, role.id, &permissions).await?;
        }

        // 3. Write, then read back so the response is the stored state rather than
        //    the requested one — a repository that normalizes the order or drops a
        //    duplicate must not make the client's copy disagree with the server's.
        self.roles
            .replace_role_permissions(role.id, &permissions)
            .await?;

        let stored = self.roles.permissions_for_role(role.id).await?;
        let member_count = count_members_in_role(&self.members, command.guild_id, role.id).await?;

        info!(
            guild_id = command.guild_id,
            role_id = %role.id,
            slug = %role.slug,
            permissions = stored.len(),
            "role matrix replaced"
        );

        Ok(RoleView {
            role,
            permissions: stored,
            member_count,
        })
    }
}
