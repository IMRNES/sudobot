use crate::application::ports::SystemClock;
use crate::infrastructure::repository::{
    SeaOrmAutoRoleRepository, SeaOrmEconomyRepository, SeaOrmGuildMemberRepository,
    SeaOrmGuildRepository, SeaOrmGuildRoleRepository, SeaOrmLevelingRepository,
    SeaOrmPermissionRepository, SeaOrmRefreshTokenRepository, SeaOrmUserRepository,
    SeaOrmWelcomeRepository,
};
use crate::infrastructure::service::{
    Argon2PasswordService, JwtTokenService, ReqwestDiscordOAuthClient, ReqwestGuildDirectory,
    ReqwestRolePanelPublisher,
};
use sudobot_config::DashboardAccessConfig;

/// Everything a handler needs, in one cheap-to-clone value.
///
/// Concrete types rather than `Box<dyn Trait>`: `DatabaseConnection` is internally
/// reference-counted and so is each of these, so cloning the state per request
/// copies pointers. Trait objects here would buy nothing and cost a vtable
/// indirection on every call.
///
/// Use cases are **not** stored. A handler builds the one it needs from these
/// collaborators, which keeps the state from growing a field per feature and makes
/// each handler's dependencies legible in its own body.
#[derive(Clone)]
pub struct AppState {
    pub password_service: Argon2PasswordService,
    pub token_service: JwtTokenService,
    pub oauth_client: ReqwestDiscordOAuthClient,

    /// Posts role panels as the bot.
    ///
    /// Separate from `oauth_client` because it uses the bot token rather than the
    /// caller's: a dashboard visitor has no authority to post as the bot, so
    /// publishing is the API's own act.
    pub panel_publisher: ReqwestRolePanelPublisher,

    /// Resolves the snowflakes the configuration stores back into names.
    ///
    /// Uses the bot token, so the dashboard can render "@Neophyte" and "#general"
    /// without a visitor needing any Discord scope of their own.
    pub guild_directory: ReqwestGuildDirectory,

    pub clock: SystemClock,

    /// The one guild the dashboard serves, plus who may open it.
    ///
    /// `None` when `DASHBOARD_GUILD_ID` is unset. Handlers refuse rather than
    /// pretending an empty whitelist admits everyone.
    pub access: Option<DashboardAccessConfig>,

    pub users: SeaOrmUserRepository,
    pub guilds: SeaOrmGuildRepository,
    pub guild_members: SeaOrmGuildMemberRepository,
    pub permissions: SeaOrmPermissionRepository,
    pub guild_roles: SeaOrmGuildRoleRepository,
    pub refresh_tokens: SeaOrmRefreshTokenRepository,
    pub economy: SeaOrmEconomyRepository,
    pub leveling: SeaOrmLevelingRepository,
    pub welcome: SeaOrmWelcomeRepository,
    pub auto_roles: SeaOrmAutoRoleRepository,
}
