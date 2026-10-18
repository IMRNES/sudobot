use anyhow::{Context, Result};
use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use std::time::Duration;
use sudobot_config::DatabaseConfig;

/// Opens the bot's database pool.
///
/// A process-local wrapper rather than a shared helper: the bot wants a smaller
/// pool than the API (it is one process doing occasional writes, not many
/// concurrent requests), and keeping the settings here makes that difference
/// visible instead of hidden behind a default.
pub async fn connect(config: &DatabaseConfig) -> Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(config.url.clone());
    options
        // The bot's queries are short and infrequent; a handful of connections
        // is plenty and leaves room for the API's pool on the same database.
        .max_connections(config.max_connections.min(5))
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .sqlx_logging(false);

    SeaDatabase::connect(options)
        .await
        .with_context(|| format!("connecting to {}", config.redacted_url()))
}
