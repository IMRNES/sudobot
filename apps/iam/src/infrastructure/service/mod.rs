//! Implementations of the application's ports.
//!
//! Each one wraps a single library — `argon2`, `jsonwebtoken`, `reqwest` — and the
//! application only ever sees the trait it declared in `application::ports`.

mod discord;
mod guild_directory;
mod jail_enforcer;
mod panel_publisher;
mod password;
mod token;

pub use discord::ReqwestDiscordOAuthClient;
pub use guild_directory::ReqwestGuildDirectory;
pub use jail_enforcer::ReqwestJailEnforcer;
pub use panel_publisher::ReqwestRolePanelPublisher;
pub use password::Argon2PasswordService;
pub use token::JwtTokenService;
