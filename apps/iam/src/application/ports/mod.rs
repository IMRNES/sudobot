//! Ports: the interfaces the application needs from the outside world.
//!
//! Each one is a trait the application owns and infrastructure implements. The
//! direction matters — `argon2` and `jsonwebtoken` are details the application
//! never names, so swapping them is an infrastructure change, not a rewrite.

pub mod clock;
pub mod discord;
pub mod guild_directory;
pub mod member_directory;
pub mod panel_publisher;
pub mod password;
pub mod token;

pub use clock::{Clock, SystemClock};
pub use discord::{DiscordOAuthClient, DiscordOAuthError, DiscordProfile, GuildMembership};
pub use guild_directory::{
    DirectoryChannel, DirectoryError, DirectoryRole, GuildDirectory, GuildDirectorySource,
};
pub use member_directory::MemberDirectory;
pub use panel_publisher::{PanelPublishError, RolePanelPublisher, render_panel};
pub use password::{PasswordError, PasswordService};
pub use token::{TokenClaims, TokenError, TokenPair, TokenService, TokenType};
