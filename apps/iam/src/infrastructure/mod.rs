//! Adapters: the concrete implementations of what the domain and application
//! layers declared.
//!
//! Nothing above this module names SeaORM, `argon2`, `jsonwebtoken` or
//! `reqwest`. That is the whole point of the layer: swapping a driver or a
//! hashing algorithm is a change here and nowhere else.

pub mod repository;
pub mod service;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use sudobot_config::DatabaseConfig;

/// Opens the API's database pool.
///
/// The pool is sized from configuration rather than left at SeaORM's defaults so
/// that a deployment can budget connections across the API, the bot and the
/// migration runner.
pub async fn connect(url: &str, config: &DatabaseConfig) -> anyhow::Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(url.to_string());
    options
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        // SQLx logs every statement at INFO; the application's own `tracing`
        // spans are the useful signal, and the driver's are noise.
        .sqlx_logging(false);

    SeaDatabase::connect(options)
        .await
        .map_err(|error| anyhow::anyhow!("connecting to the database: {error}"))
}
