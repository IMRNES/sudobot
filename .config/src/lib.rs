//! Process configuration.
//!
//! Every setting is read from the environment exactly once, in [`AppConfig::from_env`],
//! and handed to the rest of the program as a plain value. Nothing else in the
//! workspace reads `std::env` directly, so a misconfigured deployment fails at
//! startup with one readable error instead of halfway through a request.

pub mod access;
pub mod app;
pub mod auth;
pub mod database;
pub mod discord;
pub mod env;

use anyhow::{Context, Result};

pub use access::DashboardAccessConfig;
pub use app::AppServer;
pub use auth::{AuthConfig, AuthStack, DiscordOAuthConfig};
pub use database::DatabaseConfig;
pub use discord::DiscordConfig;
pub use env::Environment;

/// The whole process configuration, grouped by the subsystem that owns it.
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub environment: Environment,
    pub server: AppServer,
    pub database: DatabaseConfig,
    pub auth: AuthConfig,
    pub discord: DiscordConfig,
    /// Who may open the dashboard, and in which guild.
    ///
    /// Optional because the bot has no use for it: only the API serves the
    /// dashboard, and requiring it would refuse to start a bot-only deployment.
    pub access: Option<DashboardAccessConfig>,
}

impl AppConfig {
    /// Reads and validates every setting from the environment.
    ///
    /// Loads `.env` first when present, so local development does not need the
    /// variables exported by hand. Real deployments set them directly and the
    /// missing file is not an error.
    pub fn from_env() -> Result<Self> {
        let _ = dotenvy::dotenv();

        Ok(Self {
            environment: Environment::from_env()?,
            server: AppServer::from_env()?,
            database: DatabaseConfig::from_env()?,
            auth: AuthConfig::from_env()?,
            discord: DiscordConfig::from_env()?,
            access: DashboardAccessConfig::from_env_optional()?,
        })
    }

    /// Installs the tracing subscriber that every binary in the workspace shares.
    ///
    /// JSON output in production keeps logs machine-readable for a collector;
    /// human-readable output locally keeps them usable in a terminal.
    pub fn init_tracing(&self) -> Result<()> {
        use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

        let filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info,sudobot=debug,iam=debug,bot=debug"));

        let registry = tracing_subscriber::registry().with(filter);

        if self.environment.is_production() {
            registry.with(fmt::layer().json()).try_init()?;
        } else {
            registry.with(fmt::layer().pretty()).try_init()?;
        }

        Ok(())
    }
}

/// Loads the configuration or aborts the process with a readable report.
///
/// Only binaries call this: a `main` that cannot read its own configuration has
/// nothing useful to do, and continuing would fail later with a worse message.
pub fn load_or_exit() -> AppConfig {
    match AppConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration error: {error:#}");
            std::process::exit(1);
        }
    }
}

/// Helper for the "required environment variable" case shared by every config struct.
pub(crate) fn required(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required environment variable `{key}`"))
}

/// Reads an optional variable, falling back to `default`.
pub(crate) fn optional(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// Reads a variable that must parse, with the parse error naming the variable.
pub(crate) fn parse<T>(key: &str, default: &str) -> Result<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Display,
{
    let raw = optional(key, default);
    raw.parse::<T>()
        .map_err(|error| anyhow::anyhow!("`{key}` is not valid: {error} (got `{raw}`)"))
}
