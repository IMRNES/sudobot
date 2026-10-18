//! Automatic roles: the rules the dashboard edits, the plan the gateway builds on
//! a join, and the sweep that delivers delayed grants.
//!
//! The join and sweep paths never fail a member event — a misconfigured rule is
//! logged, and the member simply gets fewer roles.

pub mod join;
pub mod rules;
pub mod sweep;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use join::{PlanJoinAutoRolesCommand, PlanJoinAutoRolesUseCase};
pub use rules::{
    CreateAutoRoleCommand, CreateAutoRoleUseCase, DeleteAutoRoleUseCase, ListAutoRolesUseCase,
    UpdateAutoRoleCommand, UpdateAutoRoleUseCase,
};
pub use sweep::{DueGrants, SweepPendingGrantsUseCase};
