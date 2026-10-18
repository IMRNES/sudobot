//! Guild request and response shapes.
//!
//! The account and guild summaries the login returns live in `auth.rs`, next to
//! the flow that produces them; this module holds the guild-scoped CRUD shapes.

pub use super::auth::{GuildResponse, GuildSummaryResponse, MeResponse, UpdateGuildRequest};
