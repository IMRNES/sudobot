//! Implementations of the application's ports.
//!
//! Each one wraps a single library — `argon2`, `jsonwebtoken`, `reqwest` — and the
//! application only ever sees the trait it declared in `application::ports`.

mod discord;
mod password;
mod token;

pub use discord::ReqwestDiscordOAuthClient;
pub use password::Argon2PasswordService;
pub use token::JwtTokenService;
