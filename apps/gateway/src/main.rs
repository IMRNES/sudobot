//! The dashboard API.
//!
//! A separate binary from the bot so the two can be deployed, scaled and restarted
//! independently. The only thing they share is PostgreSQL, which is deliberate: a
//! deploy of the API cannot drop the bot's gateway connection, and a panic in a
//! slash command cannot take the dashboard down.

use anyhow::{Context, Result};
use axum::Router;
use axum::routing::get;
use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use std::time::Duration;
use sudobot_config::AppConfig;

use iam::infrastructure::repository::{
    SeaOrmAutoRoleRepository, SeaOrmEconomyRepository, SeaOrmGuildMemberRepository,
    SeaOrmGuildRepository, SeaOrmGuildRoleRepository, SeaOrmLevelingRepository,
    SeaOrmPermissionRepository, SeaOrmRefreshTokenRepository, SeaOrmUserRepository,
    SeaOrmWelcomeRepository,
};
use iam::infrastructure::service::{
    Argon2PasswordService, JwtTokenService, ReqwestDiscordOAuthClient, ReqwestGuildDirectory,
    ReqwestRolePanelPublisher,
};
use iam::presentation::{AppState, build_router};

#[tokio::main]
async fn main() -> Result<()> {
    let config = sudobot_config::load_or_exit();
    config.init_tracing()?;

    tracing::info!(
        environment = config.environment.as_str(),
        bind = %config.server.bind_address(),
        "starting the api"
    );

    let db = connect(&config).await?;
    let state = build_state(&config, db)?;

    // The router carries the CORS policy because the dashboard is a separate
    // origin in development; in production it is served from the same host and the
    // layer is a no-op.
    let app = Router::new()
        .nest("/api/v1/iam", build_router(state))
        .route("/", get(root))
        .layer(build_cors(&config));

    let listener = tokio::net::TcpListener::bind(config.server.bind_address())
        .await
        .with_context(|| format!("binding {}", config.server.bind_address()))?;

    tracing::info!("listening on {}", listener.local_addr()?);

    // Graceful shutdown so an in-flight request finishes and the pool closes
    // cleanly; a SIGKILL mid-migration is how a deploy corrupts a ledger.
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("running the http server")
}

/// Opens the API's connection pool.
async fn connect(config: &AppConfig) -> Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(config.database.url.clone());
    options
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(Duration::from_secs(config.database.acquire_timeout_secs))
        .idle_timeout(Duration::from_secs(config.database.idle_timeout_secs))
        // The pool already logs failures; per-query logging at this level is noise
        // that buries them.
        .sqlx_logging(false);

    SeaDatabase::connect(options)
        .await
        .with_context(|| format!("connecting to {}", config.database.redacted_url()))
}

/// Assembles the application state.
///
/// Every repository is built from one `DatabaseConnection`, so they share a pool.
/// Returning a `Result` rather than panicking keeps the failure at `main` where it
/// has a readable message and a non-zero exit code.
fn build_state(config: &AppConfig, db: DatabaseConnection) -> Result<AppState> {
    // One `AuthStack` carries the JWT settings and the Discord OAuth application
    // together, so a half-configured auth setup cannot be assembled by accident.
    let auth = config.auth.clone().with_discord()?;

    Ok(AppState {
        password_service: Argon2PasswordService::new(),
        token_service: JwtTokenService::new(
            auth.jwt.secret(),
            auth.jwt.access_token_ttl_secs,
            auth.jwt.refresh_token_ttl_secs,
        ),
        oauth_client: ReqwestDiscordOAuthClient::with_access(
            auth.oauth.clone(),
            config.access.clone(),
            Some(config.discord.token().to_string()),
        )
        .context("building the discord client")?,
        panel_publisher: ReqwestRolePanelPublisher::new(Some(config.discord.token().to_string()))
            .context("building the panel publisher")?,
        guild_directory: ReqwestGuildDirectory::new(Some(config.discord.token().to_string()))
            .context("building the guild directory client")?,
        clock: iam::application::ports::SystemClock,
        access: config.access.clone(),

        users: SeaOrmUserRepository::new(db.clone()),
        guilds: SeaOrmGuildRepository::new(db.clone()),
        guild_members: SeaOrmGuildMemberRepository::new(db.clone()),
        permissions: SeaOrmPermissionRepository::new(db.clone()),
        guild_roles: SeaOrmGuildRoleRepository::new(db.clone()),
        refresh_tokens: SeaOrmRefreshTokenRepository::new(db.clone()),
        economy: SeaOrmEconomyRepository::new(db.clone()),
        leveling: SeaOrmLevelingRepository::new(db.clone()),
        welcome: SeaOrmWelcomeRepository::new(db.clone()),
        auto_roles: SeaOrmAutoRoleRepository::new(db),
    })
}

/// The CORS layer, with a loud warning when production has no allowlist.
fn build_cors(config: &AppConfig) -> tower_http::cors::CorsLayer {
    if config.server.cors_origins.is_empty() && !config.environment.is_development() {
        tracing::warn!(
            "CORS_ORIGINS is empty outside development; \
             the dashboard will be unable to call this API from a browser"
        );
    }

    iam::presentation::router::cors_layer(
        &config.server.cors_origins,
        config.environment.is_development(),
    )
}

/// A one-line welcome so hitting the root is not a 404.
async fn root() -> &'static str {
    "sudobot api — see /api/v1/iam/health"
}

/// Resolves when the process is asked to stop.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("installing the ctrl-c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("installing the SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => tracing::info!("received ctrl-c; shutting down"),
        () = terminate => tracing::info!("received SIGTERM; shutting down"),
    }
}
