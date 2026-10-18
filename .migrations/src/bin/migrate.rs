use anyhow::Result;
use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectOptions, Database};
use std::time::Duration;

/// Standalone migration runner.
///
/// Deliberately separate from the gateway: applying a schema change should be a
/// deliberate deploy step with its own exit code, not something that happens
/// implicitly when a web server restarts.
///
/// ```text
/// cargo run -p migration -- up
/// cargo run -p migration -- down 2
/// cargo run -p migration -- status
/// ```
#[tokio::main]
async fn main() -> Result<()> {
    sudobot_config::load_or_exit().init_tracing()?;

    let config = sudobot_config::DatabaseConfig::from_env()?;
    tracing::info!(database = %config.redacted_url(), "connecting");

    let mut options = ConnectOptions::new(config.url.clone());
    options
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .sqlx_logging(false);

    let db = Database::connect(options).await?;

    let command = std::env::args().nth(1).unwrap_or_else(|| "up".to_string());
    match command.as_str() {
        "up" => {
            Migrator::up(&db, None).await?;
            tracing::info!("migrations applied");
        }
        "down" => {
            let steps = std::env::args()
                .nth(2)
                .and_then(|value| value.parse::<u32>().ok())
                .unwrap_or(1);
            Migrator::down(&db, Some(steps)).await?;
            tracing::info!(steps, "migrations rolled back");
        }
        "status" => {
            Migrator::status(&db).await?;
        }
        "fresh" => {
            Migrator::fresh(&db).await?;
            tracing::warn!("database dropped and rebuilt from scratch");
        }
        other => {
            anyhow::bail!("unknown command `{other}`; expected up | down <n> | status | fresh");
        }
    }

    Ok(())
}
