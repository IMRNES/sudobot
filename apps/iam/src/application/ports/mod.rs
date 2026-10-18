//! Ports: the interfaces the application needs from the outside world.
//!
//! Each one is a trait the application owns and infrastructure implements. The
//! direction matters — `argon2` and `jsonwebtoken` are details the application
//! never names, so swapping them is an infrastructure change, not a rewrite.

pub mod clock;
pub mod discord;
pub mod password;
pub mod token;

pub use clock::{Clock, SystemClock};
pub use discord::{DiscordGuildSummary, DiscordOAuthClient, DiscordOAuthError, DiscordProfile};
pub use password::{PasswordError, PasswordService};
pub use token::{TokenClaims, TokenError, TokenPair, TokenService, TokenType};
