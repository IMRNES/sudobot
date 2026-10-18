//! Use cases — the application's verbs.
//!
//! A use case orchestrates: it takes a command, applies domain rules, calls the
//! repositories and services it was handed, and returns a domain result. It knows
//! nothing about HTTP, SeaORM or Discord: every collaborator arrives as a trait
//! from [`ports`], which is what makes the same `GrantCurrencyUseCase` serve both
//! the dashboard and a slash command.
//!
//! Use cases are constructed per request from `AppState` rather than stored. They
//! hold no mutable state, so building one is a few pointer copies.

pub mod auth;
pub mod autorole;
pub mod economy;
pub mod guild;
pub mod guild_member;
pub mod leveling;
pub mod ports;
pub mod rbac;
pub mod user;
pub mod welcome;

pub use ports::{
    Clock, DiscordGuildSummary, DiscordOAuthClient, DiscordOAuthError, DiscordProfile,
    PasswordError, PasswordService, SystemClock, TokenClaims, TokenError, TokenPair, TokenService,
    TokenType,
};
