//! sudobot's identity, guild, economy, leveling, welcome and role-automation core.
//!
//! Layering is enforced by the module tree, and the arrows only point inward:
//!
//! ```text
//! presentation ──▶ application ──▶ domain
//! infrastructure ─▶ domain          (implements the traits domain declares)
//! presentation ──▶ infrastructure   (only to assemble AppState)
//! ```
//!
//! `domain` knows nothing about HTTP, SeaORM or Discord. It declares what the
//! business needs — entities, repository traits, errors — and the other two layers
//! satisfy those declarations. That is what lets the same use case serve an HTTP
//! handler and a slash command without either one leaking into it.

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
