//! Where a request's identity and authority are established.
//!
//! Two concerns, deliberately separate:
//!
//! - [`attach_auth`] runs on every route and only *records* who is calling, when a
//!   usable token is present. It never rejects.
//! - [`AuthenticatedUser`] and [`load_guild_context`] are extractors and helpers a
//!   handler uses when it needs that identity. They are where a request is refused.
//!
//! Keeping enforcement with the handler rather than in the router is what stops a
//! route from being filed under the wrong tier — the failure mode that made the
//! health check and the login start return 401.

pub mod auth;
pub mod permission;

pub use auth::{AuthenticatedUser, attach_auth};
pub use permission::{GuildContext, ensure_any_permission, ensure_permission, load_guild_context};
