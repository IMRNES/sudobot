//! RBAC use cases: the catalog, guild roles, and the permission matrix.
//!
//! The matrix edit in [`matrix`] is the one that carries a real rule — a guild
//! must keep at least one role granting `dashboard:manage` — and it is enforced
//! by calling the domain's `ensure_guild_stays_administrable`, never by
//! re-implementing the rule inline.

pub mod matrix;
pub mod permissions;
pub mod roles;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use matrix::{ReplaceRolePermissionsCommand, ReplaceRolePermissionsUseCase};
pub use permissions::ListPermissionsUseCase;
pub use roles::{
    CreateGuildRoleCommand, CreateGuildRoleUseCase, DeleteGuildRoleUseCase, GetGuildRoleUseCase,
    ListGuildRolesUseCase, MemberRoleCounts, RoleMemberCounts, RoleView, UpdateGuildRoleCommand,
    UpdateGuildRoleUseCase,
};
