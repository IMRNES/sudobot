//! Membership use cases: the dashboard's member list, the role change, and
//! revoking access.
//!
//! Every one of these is a "who may do what to whom" question, so the guards live
//! in `domain::guild_member` and these use cases only sequence them: refuse the
//! change, check the new role belongs here, confirm the guild keeps an
//! administrator, then write.

pub mod list;
pub mod remove;
pub mod update_role;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use list::ListGuildMembersUseCase;
pub use remove::RemoveGuildMemberUseCase;
pub use update_role::{UpdateGuildMemberRoleCommand, UpdateGuildMemberRoleUseCase};
