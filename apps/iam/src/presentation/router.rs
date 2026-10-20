use axum::Router;
use axum::routing::{delete, get, patch, post, put};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::presentation::handlers;
use crate::presentation::middleware;
use crate::presentation::state::AppState;

/// Assembles the IAM router.
///
/// The shape encodes the authorization model: `/auth` is public, `/me` needs only
/// a valid token, and everything under `/guilds/{guildId}` additionally needs the
/// caller to hold a permission *in that guild* — which each handler checks, because
/// the check needs the guild id from the path.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // Public first, and *separately* from the protected tree.
        //
        // A `.layer(require_auth)` on the combined router would run for these too:
        // layers apply to every route present at the point they are added, so
        // merging `/health` and `/auth/discord/authorize` in and then layering
        // auth makes the health check and the login start return 401 — the two
        // endpoints that must work before anyone has a token.
        .merge(public_routes())
        .merge(protected_routes())
        // One pass over every route: it attaches the caller when the request
        // carries a usable token and does nothing otherwise. Authorization is
        // decided per handler by the `AuthenticatedUser` extractor, so there is no
        // public/private tier to file a new route under incorrectly.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::attach_auth,
        ))
        // `with_state` rather than `Extension(state)`: it gives the router its
        // final `Router<()>` type, which is what a handler's `State<AppState>`
        // extractor resolves against. Layering an extension instead leaves the
        // router generic over the state and every `State` handler fails to match.
        .with_state(state)
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &axum::http::Request<_>| {
                // The method and path identify a request; the body never appears in
                // a span, so a login's credentials cannot leak into the logs.
                tracing::info_span!(
                    "http",
                    method = %request.method(),
                    path = %request.uri().path(),
                )
            }),
        )
}

/// Routes that must work without a token.
///
/// `/auth` belongs here, which is the part that is easy to get wrong: starting a
/// login, finishing it, and refreshing an expired access token all happen *before*
/// the caller has a valid one. Only `/auth/logout` needs a session, and it checks
/// for one itself.
fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(handlers::health::health))
        .nest("/auth", auth_routes())
}

/// The guild-scoped routes. Kept as a group for readability, not for enforcement.
fn protected_routes() -> Router<AppState> {
    Router::new()
        .nest("/me", me_routes())
        .nest("/guilds", guild_routes())
}

/// Starting and ending a session.
///
/// Nested under the public tree, so none of these may assume a token. `logout`
/// is the one that does need one — it takes the `AuthenticatedUser` extractor,
/// which rejects a request whose middleware never inserted it. That is a
/// deliberate trade: the alternative is a third router tier for a single route.
fn auth_routes() -> Router<AppState> {
    Router::new()
        .route("/discord/authorize", post(handlers::auth::authorize))
        .route(
            "/discord/callback",
            // POST exchanges a code the browser already captured; GET is Discord
            // itself redirecting here. Both exist because the URI registered in the
            // Developer Portal is the operator's choice, and only Discord can change
            // it — a redirect aimed at this path must not answer 405 with the code
            // already spent.
            get(handlers::auth::callback_redirect).post(handlers::auth::callback),
        )
        .route("/refresh", post(handlers::auth::refresh))
        .route("/logout", post(handlers::auth::logout))
}

/// The signed-in caller.
fn me_routes() -> Router<AppState> {
    Router::new().route("/", get(handlers::auth::me))
}

/// Everything guild-scoped.
///
/// Order matters: `/permissions` and `/roles` are literal segments that would be
/// swallowed by a `/{guild_id}` route declared first, so the literal routes are
/// nested under the dynamic one rather than mixed with it.
fn guild_routes() -> Router<AppState> {
    Router::new()
        .route("/", get(handlers::guild::list))
        .nest("/{guild_id}", guild_scoped_routes())
}

fn guild_scoped_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::guild::detail).patch(handlers::guild::update),
        )
        // Literal segment, so it must be declared before any `/{id}` sibling could
        // swallow it — same reason `/permissions` and `/roles` are nested here.
        .route("/directory", get(handlers::guild::directory))
        .nest("/members", member_routes())
        .nest("/permissions", permission_routes())
        .nest("/roles", role_routes())
        .nest("/economy", economy_routes())
        .nest("/leveling", leveling_routes())
        .nest("/welcome", welcome_routes())
        .nest("/panels", panel_routes())
        .nest("/autoroles", autorole_routes())
}

fn member_routes() -> Router<AppState> {
    Router::new().route("/", get(handlers::member::list)).route(
        "/{user_id}",
        patch(handlers::member::update_role).delete(handlers::member::remove),
    )
}

fn permission_routes() -> Router<AppState> {
    Router::new().route("/", get(handlers::role::list_permissions))
}

fn role_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::role::list_roles).post(handlers::role::create_role),
        )
        .route(
            "/{role_id}",
            get(handlers::role::get_role)
                .patch(handlers::role::update_role)
                .delete(handlers::role::delete_role),
        )
        .route(
            "/{role_id}/permissions",
            put(handlers::role::replace_permissions),
        )
}

fn economy_routes() -> Router<AppState> {
    Router::new()
        .route("/leaderboard", get(handlers::economy::leaderboard))
        .route(
            "/items",
            get(handlers::economy::list_items).post(handlers::economy::create_item),
        )
        .route(
            "/items/{item_id}",
            patch(handlers::economy::update_item).delete(handlers::economy::delete_item),
        )
        .route(
            "/accounts/{discord_id}",
            get(handlers::economy::get_account),
        )
        .route(
            "/accounts/{discord_id}/transactions",
            get(handlers::economy::list_transactions),
        )
        .route(
            "/accounts/{discord_id}/adjust",
            post(handlers::economy::adjust_balance),
        )
}

fn leveling_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/config",
            get(handlers::leveling::get_config).patch(handlers::leveling::update_config),
        )
        .route("/leaderboard", get(handlers::leveling::leaderboard))
        .route(
            "/rewards",
            get(handlers::leveling::list_rewards).post(handlers::leveling::create_reward),
        )
        .route(
            "/rewards/{reward_id}",
            delete(handlers::leveling::delete_reward),
        )
}

fn welcome_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::welcome::get_config).patch(handlers::welcome::update_config),
        )
        .route("/preview", get(handlers::welcome::preview_welcome))
}

fn panel_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::welcome::list_panels).post(handlers::welcome::upsert_panel),
        )
        // GET reads the panel, PATCH rewrites its header. Both go through the
        // same upsert use case, which is why the doc calls the write a PATCH
        // rather than a separate PUT.
        .route(
            "/{panel_id}",
            get(handlers::welcome::get_panel).patch(handlers::welcome::update_panel),
        )
        .route("/{panel_id}/options", post(handlers::welcome::add_option))
        .route(
            "/{panel_id}/options/{option_id}",
            delete(handlers::welcome::delete_option),
        )
        .route(
            "/{panel_id}/publish",
            post(handlers::welcome::publish_panel),
        )
}

fn autorole_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/",
            get(handlers::autorole::list).post(handlers::autorole::create),
        )
        .route(
            "/{id}",
            patch(handlers::autorole::update).delete(handlers::autorole::delete),
        )
}

/// The CORS policy, from the configured origins.
///
/// An empty list in development allows any origin, because Vite picks its port
/// freely. Production must set `CORS_ORIGINS` — allowing any origin with
/// credentials is the kind of default that quietly becomes a vulnerability.
pub fn cors_layer(origins: &[String], is_development: bool) -> CorsLayer {
    use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};

    let base = CorsLayer::new()
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::PATCH,
            axum::http::Method::PUT,
            axum::http::Method::DELETE,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE]);

    if origins.is_empty() && is_development {
        return base.allow_origin(Any);
    }

    let parsed: Vec<axum::http::HeaderValue> = origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();

    base.allow_origin(parsed)
}
