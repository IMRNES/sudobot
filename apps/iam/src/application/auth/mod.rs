//! Logging a user in through Discord, and keeping them logged in.

pub mod login;
pub mod logout;
pub mod refresh;
pub mod sync_guilds;

pub use login::{DiscordLoginCommand, DiscordLoginUseCase, LoginResult};
pub use logout::{LogoutCommand, LogoutUseCase};
pub use refresh::{RefreshCommand, RefreshUseCase};
pub use sync_guilds::{GuildSyncOutcome, SyncGuildsCommand, SyncGuildsUseCase};
