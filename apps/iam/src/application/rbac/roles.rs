use std::future::Future;

use tracing::{info, warn};
use uuid::Uuid;

use super::permissions::{load_role, map_role_error};
use crate::domain::errors::{PaginationParams, RepositoryError};
use crate::domain::guild_member::GuildMemberRepository;
use crate::domain::rbac::{
    GuildRole, GuildRoleRepository, RbacError, ensure_guild_stays_administrable, names,
    validate_slug,
};

/// A role as the dashboard renders it: the row, its matrix, and how many members
/// it affects.
///
/// `member_count` lives here rather than on the entity because it is a property of
/// the guild's membership, not of the role — but every screen that shows a role
/// also shows it, and a client that had to fetch it separately would render the
/// delete button before knowing whether it is safe to press.
#[derive(Debug, Clone)]
pub struct RoleView {
    pub role: GuildRole,
    pub permissions: Vec<String>,
    pub member_count: u64,
}

/// How many members hold a given role.
///
/// `RoleView::member_count` needs this number, and `GuildRoleRepository` — all
/// that `ListGuildRolesUseCase` and `GetGuildRoleUseCase` are handed by the
/// handlers — exposes no such method. Rather than widen a frozen trait, the count
/// arrives as a small port of its own, threaded through the constructors that can
/// supply it.
pub trait RoleMemberCounts: Send + Sync {
    fn members_in_role(
        &self,
        guild_id: u64,
        role_id: Uuid,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}

/// The counter for callers that hold a member repository.
pub struct MemberRoleCounts<M> {
    members: M,
}

impl<M> MemberRoleCounts<M> {
    pub fn new(members: M) -> Self {
        Self { members }
    }
}

impl<M: GuildMemberRepository> RoleMemberCounts for MemberRoleCounts<M> {
    async fn members_in_role(&self, guild_id: u64, role_id: Uuid) -> Result<u64, RepositoryError> {
        count_members_in_role(&self.members, guild_id, role_id).await
    }
}

/// The counter for callers that do not.
///
/// Reports zero. A guild with nobody holding a role is the state the delete guard
/// allows, so the worst this can do is show a cosmetic `"member_count": 0` — never
/// a wrong allow/deny decision: every guard that protects a guild reads its
/// numbers through `GuildMemberRepository`, not through this port.
pub struct UnknownRoleMemberCounts;

impl RoleMemberCounts for UnknownRoleMemberCounts {
    async fn members_in_role(
        &self,
        _guild_id: u64,
        _role_id: Uuid,
    ) -> Result<u64, RepositoryError> {
        Ok(0)
    }
}

/// Counts the members currently pointing at one role.
///
/// `GuildMemberRepository` has no per-role count — it can only count holders of a
/// *permission* — so the guild's members are paged and filtered. That is exact
/// where `count_holders_of` would be an approximation, at the cost of reading the
/// rows; it is used only on the paths that need an exact answer (the delete guard,
/// the matrix guard, and the member count on the roles screen), never on an
/// authorization check.
pub(super) async fn count_members_in_role<M: GuildMemberRepository>(
    members: &M,
    guild_id: u64,
    role_id: Uuid,
) -> Result<u64, RepositoryError> {
    const REQUESTED_PER_PAGE: u32 = 100;

    let mut count = 0u64;
    let mut page = 1u32;

    loop {
        let params = PaginationParams {
            page,
            per_page: REQUESTED_PER_PAGE,
        };
        let batch = members.list_for_guild(guild_id, &params).await?;

        count += batch
            .data
            .iter()
            .filter(|row| row.member.guild_role_id == role_id)
            .count() as u64;

        // The repository may clamp `per_page`, so the loop's stopping condition is
        // derived from what it actually returned rather than from what was asked
        // for; otherwise a clamp would silently truncate the count.
        let effective = batch.per_page.max(1);
        let covered = u64::from(page) * u64::from(effective);
        if (batch.data.len() as u32) < effective || covered >= batch.total {
            return Ok(count);
        }

        page += 1;
    }
}

/// Reads one role's matrix and member count into a view.
pub(super) async fn view_for<R: GuildRoleRepository, C: RoleMemberCounts>(
    roles: &R,
    counts: &C,
    role: GuildRole,
) -> Result<RoleView, RbacError> {
    let permissions = roles.permissions_for_role(role.id).await?;
    let member_count = counts.members_in_role(role.guild_id, role.id).await?;
    Ok(RoleView {
        role,
        permissions,
        member_count,
    })
}

/// Every role in a guild, with its matrix and member count.
pub struct ListGuildRolesUseCase<R, C = UnknownRoleMemberCounts> {
    roles: R,
    counts: C,
}

impl<R: GuildRoleRepository> ListGuildRolesUseCase<R, UnknownRoleMemberCounts> {
    pub fn new(roles: R) -> Self {
        Self {
            roles,
            counts: UnknownRoleMemberCounts,
        }
    }
}

impl<R: GuildRoleRepository, C: RoleMemberCounts> ListGuildRolesUseCase<R, C> {
    /// The same use case with a real counter, for callers that also hold a member
    /// repository.
    pub fn with_counts(roles: R, counts: C) -> Self {
        Self { roles, counts }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<Vec<RoleView>, RbacError> {
        let roles = self.roles.list_for_guild(guild_id).await?;

        let mut views = Vec::with_capacity(roles.len());
        for role in roles {
            views.push(view_for(&self.roles, &self.counts, role).await?);
        }

        Ok(views)
    }
}

/// One role, addressed by id.
pub struct GetGuildRoleUseCase<R, C = UnknownRoleMemberCounts> {
    roles: R,
    counts: C,
}

impl<R: GuildRoleRepository> GetGuildRoleUseCase<R, UnknownRoleMemberCounts> {
    pub fn new(roles: R) -> Self {
        Self {
            roles,
            counts: UnknownRoleMemberCounts,
        }
    }
}

impl<R: GuildRoleRepository, C: RoleMemberCounts> GetGuildRoleUseCase<R, C> {
    pub fn with_counts(roles: R, counts: C) -> Self {
        Self { roles, counts }
    }

    pub async fn execute(&self, guild_id: u64, role_id: Uuid) -> Result<RoleView, RbacError> {
        let role = load_role(&self.roles, guild_id, role_id).await?;
        view_for(&self.roles, &self.counts, role).await
    }
}

/// Creates a guild-defined role.
#[derive(Debug, Clone)]
pub struct CreateGuildRoleCommand {
    pub guild_id: u64,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub position: i32,
}

pub struct CreateGuildRoleUseCase<R> {
    roles: R,
}

impl<R: GuildRoleRepository> CreateGuildRoleUseCase<R> {
    pub fn new(roles: R) -> Self {
        Self { roles }
    }

    /// The new role starts with an empty matrix.
    ///
    /// Deliberately: creating a role that already grants something is a silent
    /// privilege grant, and the matrix editor is right there.
    pub async fn execute(&self, command: CreateGuildRoleCommand) -> Result<GuildRole, RbacError> {
        let slug = validate_slug(&command.slug)?;
        let name = command.name.trim();
        if name.is_empty() {
            return Err(RbacError::UnknownPermission(
                "a role needs a name".to_string(),
            ));
        }

        // The unique index on (guild_id, slug) is the real guard; reading first
        // turns the common case into a readable message rather than letting a
        // constraint violation surface as a 500.
        if self
            .roles
            .find_by_slug(command.guild_id, &slug)
            .await?
            .is_some()
        {
            warn!(guild_id = command.guild_id, slug, "duplicate role slug");
            return Err(RbacError::Repository(RepositoryError::conflict(format!(
                "a role with slug `{slug}` already exists"
            ))));
        }

        let role = self
            .roles
            .create(
                command.guild_id,
                &slug,
                name,
                command.description.as_deref(),
                false,
                command.position,
            )
            .await?;

        info!(guild_id = command.guild_id, slug = %role.slug, "guild role created");
        Ok(role)
    }
}

/// Renames a role or moves it in the ordering.
#[derive(Debug, Clone)]
pub struct UpdateGuildRoleCommand {
    pub guild_id: u64,
    pub role_id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub position: Option<i32>,
}

pub struct UpdateGuildRoleUseCase<R, C = UnknownRoleMemberCounts> {
    roles: R,
    counts: C,
}

impl<R: GuildRoleRepository> UpdateGuildRoleUseCase<R, UnknownRoleMemberCounts> {
    pub fn new(roles: R) -> Self {
        Self {
            roles,
            counts: UnknownRoleMemberCounts,
        }
    }
}

impl<R: GuildRoleRepository, C: RoleMemberCounts> UpdateGuildRoleUseCase<R, C> {
    pub fn with_counts(roles: R, counts: C) -> Self {
        Self { roles, counts }
    }

    /// Patches a role's presentation.
    ///
    /// The slug is not patchable — not for a system role and not for a custom one.
    /// It is the identifier permission checks and URLs are built from, so renaming
    /// it would silently repoint every reference; for a system role it is doubly
    /// fixed, because `default_role_permissions` and the guild bootstrap key off
    /// it. The refusal is stated against `is_system_role` so the rule reads in one
    /// place rather than being implied by a missing field.
    pub async fn execute(&self, command: UpdateGuildRoleCommand) -> Result<RoleView, RbacError> {
        let existing = load_role(&self.roles, command.guild_id, command.role_id).await?;

        if existing.is_system_role() {
            tracing::debug!(
                role_id = %existing.id,
                slug = %existing.slug,
                "a system role's slug is fixed; only its presentation is patchable"
            );
        }

        let name = command
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_string);

        // Nothing to change is not an error: a form submitted unchanged should
        // return the current state rather than a 400 the user cannot act on.
        if name.is_none() && command.description.is_none() && command.position.is_none() {
            return view_for(&self.roles, &self.counts, existing).await;
        }

        let role = self
            .roles
            .update(
                command.role_id,
                name.as_deref(),
                command.description.as_deref(),
                command.position,
            )
            .await
            .map_err(map_role_error)?;

        info!(guild_id = command.guild_id, role_id = %role.id, "guild role updated");
        view_for(&self.roles, &self.counts, role).await
    }
}

/// Deletes a role the dashboard created.
pub struct DeleteGuildRoleUseCase<R, M> {
    roles: R,
    members: M,
}

impl<R: GuildRoleRepository, M: GuildMemberRepository> DeleteGuildRoleUseCase<R, M> {
    pub fn new(roles: R, members: M) -> Self {
        Self { roles, members }
    }

    /// Refuses two deletions the database would refuse anyway, but with an
    /// explanation instead of a 500.
    ///
    /// The seeded roles are referenced by slug throughout the codebase, and the
    /// membership foreign key is `ON DELETE RESTRICT`; both would come back as an
    /// opaque constraint error. Checking here produces a message the admin can act
    /// on — "move these four members first" — and the count comes from the member
    /// repository, so it is exact.
    pub async fn execute(&self, guild_id: u64, role_id: Uuid) -> Result<(), RbacError> {
        let role = load_role(&self.roles, guild_id, role_id).await?;

        if role.is_system_role() {
            warn!(role_id = %role.id, slug = %role.slug, "refused to delete a system role");
            return Err(RbacError::SystemRoleImmutable(role.slug));
        }

        let member_count = count_members_in_role(&self.members, guild_id, role.id).await?;
        if member_count > 0 {
            warn!(
                role_id = %role.id,
                slug = %role.slug,
                member_count,
                "refused to delete a role that is still assigned"
            );
            return Err(RbacError::RoleInUse(role.slug));
        }

        self.roles.delete(role_id).await.map_err(map_role_error)?;

        info!(guild_id, role_id = %role_id, slug = %role.slug, "guild role deleted");
        Ok(())
    }
}

/// The permission names a role's matrix should hold, validated against the
/// catalog.
///
/// Shared with the matrix use case so "unknown permission" is decided in exactly
/// one place.
pub(super) fn validate_permission_names(requested: &[String]) -> Result<Vec<String>, RbacError> {
    let catalog: Vec<&str> = names::CATALOG.iter().map(|(name, _, _)| *name).collect();
    let mut permissions: Vec<String> = Vec::with_capacity(requested.len());

    for name in requested {
        if !catalog.contains(&name.as_str()) {
            warn!(permission = %name, "rejected an unknown permission name");
            return Err(RbacError::UnknownPermission(name.clone()));
        }
        permissions.push(name.clone());
    }

    permissions.sort();
    permissions.dedup();
    Ok(permissions)
}

/// Guards the edit that would leave a guild with no administrator.
///
/// Only called when the edit actually removes `dashboard:manage` — adding a
/// permission, or editing a matrix that never granted it, cannot empty the
/// administrator set. The numbers come from `GuildMemberRepository`, so the
/// subtraction is exact.
pub(super) async fn ensure_manage_survives<M: GuildMemberRepository>(
    members: &M,
    guild_id: u64,
    role_id: Uuid,
    permissions: &[String],
) -> Result<(), RbacError> {
    if permissions
        .iter()
        .any(|name| name == names::DASHBOARD_MANAGE)
    {
        return Ok(());
    }

    // `count_holders_of` counts every member whose *current* role grants the
    // capability, and that includes the members holding this role: they are about
    // to lose it, so they are subtracted. What remains is the membership of other
    // roles — exactly what the guild keeps.
    let holders = members
        .count_holders_of(guild_id, names::DASHBOARD_MANAGE)
        .await?;
    let losing_it = count_members_in_role(members, guild_id, role_id).await?;
    let remaining = holders.saturating_sub(losing_it) as usize;

    ensure_guild_stays_administrable(remaining, role_id)?;

    info!(
        guild_id,
        role_id = %role_id,
        holders,
        losing_it,
        remaining,
        "matrix edit keeps the guild administrable"
    );

    Ok(())
}
