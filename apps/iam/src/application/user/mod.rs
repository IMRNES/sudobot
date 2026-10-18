//! The account use cases behind `/me` and the platform-admin user routes.
//!
//! `GetMeUseCase` is the interesting one: it answers the dashboard's very first
//! question — who is signed in, and which guilds can they act in — in a shape the
//! client can render directly, permissions included.

pub mod admin;
pub mod errors;
pub mod me;

#[cfg(test)]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use admin::{DeleteUserUseCase, GetUserUseCase, ListUsersUseCase, SetSuperAdminUseCase};
pub use errors::UserAdminError;
pub use me::{GetMeUseCase, MeView, MyGuildView};
