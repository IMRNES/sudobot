// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail loudly
// is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! HTTP-level tests: the router, the auth layer, and the authorization checks.
//!
//! These drive the assembled router directly rather than binding a socket, so they
//! exercise the whole extractor stack — path parsing, the `AuthenticatedUser`
//! extractor, `ValidatedJson`, the guild context — without a network.
//!
//! The Discord client is built from placeholder credentials that must come from the
//! environment (`DiscordOAuthConfig` reads them itself and has no public
//! constructor); no test reaches Discord, so any value will do:
//!
//! ```text
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/sudobot \
//! JWT_SECRET=0123456789012345678901234567890123456789 \
//! DISCORD_CLIENT_ID=test DISCORD_CLIENT_SECRET=test \
//!   cargo test -p iam --test http_api -- --ignored
//! ```

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use std::time::Duration;
use tower::ServiceExt;
use uuid::Uuid;

use iam::application::ports::{Clock, SystemClock, TokenService};
use iam::domain::guild::{GuildRepository, NewGuild};
use iam::domain::user::{NewUser, UserRepository};
use iam::infrastructure::repository::{
    SeaOrmAutoRoleRepository, SeaOrmEconomyRepository, SeaOrmGuildMemberRepository,
    SeaOrmGuildRepository, SeaOrmGuildRoleRepository, SeaOrmLevelingRepository,
    SeaOrmPermissionRepository, SeaOrmRefreshTokenRepository, SeaOrmUserRepository,
    SeaOrmWelcomeRepository,
};
use iam::infrastructure::service::{
    Argon2PasswordService, JwtTokenService, ReqwestDiscordOAuthClient,
};
use iam::presentation::{AppState, build_router};

/// The secret the test service signs with.
///
/// Not the one in the environment: the point is that this test controls both ends,
/// so a token it mints is one the router accepts and nothing else is.
const SECRET: &[u8] = b"http-api-test-secret-that-is-at-least-32-bytes";

/// How long a test access token lives.
const ACCESS_TTL_SECS: i64 = 900;

async fn connect() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL").ok()?;

    let mut options = ConnectOptions::new(url);
    options
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(5))
        .sqlx_logging(false);

    SeaDatabase::connect(options).await.ok()
}

/// Mounts the IAM router the way the gateway does.
///
/// The prefix is part of the contract clients depend on, so the tests exercise the
/// same paths a browser would rather than the router's internal ones.
fn mount(state: AppState) -> axum::Router {
    axum::Router::new().nest("/api/v1/iam", build_router(state))
}

/// Builds the real application state against the test database.
async fn state(db: DatabaseConnection) -> AppState {
    let oauth = sudobot_config::DiscordOAuthConfig::from_env("http://localhost:5173")
        .expect("DISCORD_CLIENT_ID and DISCORD_CLIENT_SECRET must be set for this test");

    AppState {
        password_service: Argon2PasswordService::new(),
        token_service: JwtTokenService::new(SECRET, ACCESS_TTL_SECS, 2_592_000),
        oauth_client: ReqwestDiscordOAuthClient::new(oauth).expect("the discord client builds"),
        clock: SystemClock,

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
    }
}

/// Issues an access token the way a completed login would.
async fn token_for(state: &AppState, user_id: Uuid) -> String {
    state
        .token_service
        .issue_pair(user_id, state.clock.now())
        .await
        .expect("issuing a token should succeed")
        .access_token
}

/// Creates an account, optionally promoting it to platform administrator.
async fn seed_user(users: &SeaOrmUserRepository, is_platform_admin: bool) -> Uuid {
    let user = users
        .create(NewUser {
            id: Uuid::new_v4(),
            // Unique per call, derived from the clock's sub-second part.
            discord_id: 700_000_000_000_000_000 + u64::from(Utc::now().timestamp_subsec_micros()),
            username: "http-test".to_string(),
            global_name: None,
            avatar_url: None,
            email: None,
        })
        .await
        .expect("the user should insert");

    if is_platform_admin {
        // `set_super_admin` returns the updated account, so the id is read back
        // from it rather than assumed unchanged.
        users
            .set_super_admin(user.id, true)
            .await
            .expect("promotion should succeed")
            .id
    } else {
        user.id
    }
}

/// A guild id nothing else in the suite uses.
fn unique_guild_id() -> u64 {
    800_000_000_000_000_000 + u64::from(Utc::now().timestamp_subsec_micros())
}

/// Creates a guild with the bot present.
async fn seed_guild(guilds: &SeaOrmGuildRepository, name: &str) -> u64 {
    let guild_id = unique_guild_id();
    guilds
        .upsert(NewGuild {
            id: guild_id,
            name: name.to_string(),
            icon_url: None,
            owner_discord_id: Some(1),
            member_count: 1,
        })
        .await
        .expect("the guild should insert");
    guild_id
}

/// A request with a bearer token attached.
fn authed(method: &str, uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

/// Reads a JSON response body.
async fn json_body(response: axum::response::Response) -> serde_json::Value {
    let bytes = axum::body::to_bytes(response.into_body(), 256 * 1024)
        .await
        .expect("the body should be readable");
    serde_json::from_slice(&bytes).expect("the body should be JSON")
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn the_health_check_answers_without_a_token() {
    let Some(db) = connect().await else { return };
    let app = mount(state(db).await);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/iam/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a health check that requires a token cannot be used to check health"
    );

    let json = json_body(response).await;
    assert_eq!(json["status"], "ok");
    assert_eq!(
        json["database"], "ok",
        "the probe reports the database round trip, so a dead pool is visible"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_discord_redirect_to_the_api_path_forwards_to_the_dashboard() {
    let Some(db) = connect().await else { return };
    let app = mount(state(db).await);

    // Discord redirects with a browser GET and the query it was given. Answering
    // 405 here would strand a user whose authorization code is already spent, so
    // the API forwards to the dashboard page that performs the exchange.
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/iam/auth/discord/callback?code=abc&state=deadbeef")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        response.status().is_redirection(),
        "a redirect must be followed, not refused (got {})",
        response.status()
    );

    let location = response
        .headers()
        .get(header::LOCATION)
        .expect("a redirect without Location goes nowhere")
        .to_str()
        .unwrap();

    assert_eq!(
        location, "/auth/callback?code=abc&state=deadbeef",
        "the query is forwarded verbatim: `state` is compared byte for byte"
    );
    assert!(
        location.starts_with('/'),
        "relative, so it cannot disagree with the host the browser used: {location}"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn starting_a_login_needs_no_token() {
    let Some(db) = connect().await else { return };
    let app = mount(state(db).await);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/iam/auth/discord/authorize")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "the login start is what a signed-out visitor calls first"
    );

    let json = json_body(response).await;
    let url = json["authorize_url"].as_str().unwrap();
    assert!(
        url.starts_with("https://discord.com/oauth2/authorize"),
        "{url}"
    );
    assert!(
        url.contains("state="),
        "the server mints the state; a URL without one cannot be verified on return"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn the_session_endpoints_reject_anonymous_callers() {
    let Some(db) = connect().await else { return };
    let app = mount(state(db).await);

    for (method, uri) in [
        ("GET", "/api/v1/iam/me"),
        ("GET", "/api/v1/iam/guilds"),
        ("GET", "/api/v1/iam/guilds/1/members"),
        ("POST", "/api/v1/iam/auth/sync-guilds"),
    ] {
        // No header at all.
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must not answer an anonymous caller"
        );

        // A token this service never signed.
        let response = app
            .clone()
            .oneshot(authed(method, uri, "not.a.real.token"))
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {uri} must not accept a forged token"
        );
    }
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn me_returns_the_token_subject_and_their_guilds() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let user_id = seed_user(&users, false).await;
    let token = token_for(&app_state, user_id).await;

    let response = app
        .oneshot(authed("GET", "/api/v1/iam/me", &token))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let json = json_body(response).await;
    assert_eq!(
        json["user"]["id"].as_str().unwrap(),
        user_id.to_string(),
        "the response describes the token's subject, not a parameter"
    );
    assert_eq!(json["user"]["username"], "http-test");
    assert!(
        json["guilds"].as_array().unwrap().is_empty(),
        "an account with no memberships has no guilds"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_non_member_is_refused_at_the_tenant_boundary() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let guild_id = seed_guild(&guilds, "Scope Test").await;
    let outsider = seed_user(&users, false).await;
    let token = token_for(&app_state, outsider).await;

    let response = app
        .oneshot(authed(
            "GET",
            &format!("/api/v1/iam/guilds/{guild_id}/members"),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "membership is the tenant boundary: a non-member gets nothing"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn an_unknown_guild_is_missing_rather_than_forbidden() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let user_id = seed_user(&users, true).await;
    let token = token_for(&app_state, user_id).await;

    let response = app
        .oneshot(authed(
            "GET",
            "/api/v1/iam/guilds/999999999999999999/members",
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "an unknown guild is missing, not forbidden — a client acts on the difference"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_guild_the_bot_left_reports_a_conflict_not_a_crash() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let guild_id = seed_guild(&guilds, "Left Behind").await;
    // The bot is removed: the row stays so audit and economy history survive, but
    // there is nothing left to administer.
    guilds
        .update(guild_id, iam::domain::guild::GuildPatch::bot_left())
        .await
        .expect("marking the guild as left should succeed");

    let user_id = seed_user(&users, true).await;
    let token = token_for(&app_state, user_id).await;

    let response = app
        .oneshot(authed(
            "GET",
            &format!("/api/v1/iam/guilds/{guild_id}/members"),
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::CONFLICT,
        "a guild the bot has left is a state problem the admin can fix, not a crash"
    );

    let json = json_body(response).await;
    assert!(
        json["error"].as_str().unwrap().contains("invite it again"),
        "the message says what to do about it, got: {}",
        json["error"]
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_validation_failure_is_a_400_that_names_the_field() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let user_id = seed_user(&users, false).await;
    let token = token_for(&app_state, user_id).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/iam/auth/refresh")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"refresh_token": ""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = json_body(response).await;
    assert!(
        json["error"].as_str().unwrap().contains("refresh_token"),
        "a validation failure names the field that failed, got: {}",
        json["error"]
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_malformed_json_body_is_a_400_rather_than_a_500() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let user_id = seed_user(&users, false).await;
    let token = token_for(&app_state, user_id).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/iam/auth/refresh")
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{not json"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "a client's broken JSON is the client's error"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_guild_id_that_is_not_a_snowflake_is_rejected_before_the_database() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db.clone());
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    let user_id = seed_user(&users, true).await;
    let token = token_for(&app_state, user_id).await;

    // This is what arrives when a client round-trips a snowflake through a
    // JavaScript number.
    let response = app
        .oneshot(authed(
            "GET",
            "/api/v1/iam/guilds/123456789012345680.0/members",
            &token,
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let json = json_body(response).await;
    assert!(
        json["error"].as_str().unwrap().contains("discord id"),
        "the message names the real problem, got: {}",
        json["error"]
    );
}

/// The `#[zod(...)]` attributes are the only thing enforcing field lengths, and
/// they are invisible to `cargo check`: a struct deriving `ZodSchema` with no
/// attributes compiles and validates nothing. When a cleanup pass stripped them,
/// every length rule in `API.md` silently stopped being applied. This test is what
/// makes that failure loud.
#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn request_length_rules_are_actually_enforced() {
    let Some(db) = connect().await else { return };
    let app_state = state(db).await;
    let app = mount(app_state.clone());

    // `refresh_token` is declared `min_length(1), max_length(512)`. An empty one
    // must be refused before the handler ever looks at it.
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/iam/auth/refresh")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"refresh_token": ""}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "an empty refresh_token violates min_length(1); a 401 here means the \
         constraint is not being applied at all"
    );

    let json = json_body(response).await;
    let message = json["error"].as_str().unwrap();
    assert!(
        message.contains("refresh_token"),
        "the failure names the offending field, got: {message}"
    );
    assert!(
        message.contains("small") || message.contains("least") || message.contains("short"),
        "the failure explains the rule rather than saying 'invalid': {message}"
    );
}

/// The same rule, for a body whose `max_length` is exceeded.
#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn an_over_long_field_is_refused_too() {
    let Some(db) = connect().await else { return };
    let app_state = state(db).await;
    let app = mount(app_state);

    let too_long = "x".repeat(600);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/iam/auth/refresh")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(format!(r#"{{"refresh_token": "{too_long}"}}"#)))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "600 characters exceeds max_length(512)"
    );
}
