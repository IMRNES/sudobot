//! The account use cases behind the platform-admin user routes.
//!
//! The dashboard's own "who am I" no longer lives here: with a single configured
//! guild and a role whitelist, that answer is produced by
//! [`DashboardMembershipUseCase`](crate::application::auth::DashboardMembershipUseCase)
//! during login, so there is nothing left to resolve from a list of memberships.

pub mod admin;
pub mod errors;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use admin::{DeleteUserUseCase, GetUserUseCase, ListUsersUseCase, SetSuperAdminUseCase};
pub use errors::UserAdminError;
