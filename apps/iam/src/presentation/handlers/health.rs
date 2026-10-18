use axum::Json;
use axum::extract::State;
use tracing::instrument;

use crate::domain::user::UserRepository;
use crate::presentation::dto::auth::HealthResponse;
use crate::presentation::state::AppState;

/// Liveness and readiness in one probe.
///
/// The database round trip is part of the answer rather than a separate `/ready`:
/// this service's only dependency is PostgreSQL, and a probe that says "ok" while
/// the pool is dead is worse than no probe at all.
#[instrument(skip_all)]
pub async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    // A lookup that is guaranteed to miss costs one indexed read and proves the
    // pool can hand out a connection and run a query. Using a real count instead
    // would make the probe's cost grow with the table.
    let database = match state.users.find_by_id(uuid::Uuid::nil()).await {
        Ok(_) => "ok",
        Err(error) => {
            tracing::error!(%error, "health check could not reach the database");
            "unreachable"
        }
    };

    Json(HealthResponse {
        status: "ok",
        database,
        version: env!("CARGO_PKG_VERSION"),
    })
}
