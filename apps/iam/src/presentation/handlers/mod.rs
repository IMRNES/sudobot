//! One module per feature area, mirroring `dto/`.
//!
//! A handler's job is three lines of work done properly: authorize, convert the
//! request into a command, call the use case, render the response. Anything longer
//! is a sign a rule belongs in the domain.

pub mod auth;
pub mod autorole;
pub mod economy;
pub mod guild;
pub mod health;
pub mod jail;
pub mod leveling;
pub mod member;
pub mod role;
pub mod welcome;

use uuid::Uuid;

use crate::presentation::error::AppError;
use crate::presentation::middleware::GuildContext;
use crate::presentation::state::AppState;

/// Resolves the caller's authority in a guild and checks one permission.
///
/// Every guild-scoped handler starts with this call, so the pattern is visible at
/// the top of each body rather than hidden in a layer that a reader has to go and
/// find. The alternative — a middleware that parses the path — would have to
/// re-parse the guild id and could not skip the routes that are not guild-scoped.
pub async fn authorize(
    state: &AppState,
    user_id: Uuid,
    guild_id: u64,
    permission: &str,
) -> Result<GuildContext, AppError> {
    let context =
        crate::presentation::middleware::load_guild_context(state, user_id, guild_id).await?;
    crate::presentation::middleware::ensure_permission(&context, permission)?;
    Ok(context)
}

/// Converts the paginator's HTTP query into the domain's own params type.
///
/// The presentation layer owns this translation so that the domain never imports
/// `paginator-axum` — the domain's `PaginationParams` is its own type, and this is
/// the one place the two meet.
pub fn to_pagination(
    query: &paginator_rs::PaginationParams,
) -> crate::domain::errors::PaginationParams {
    crate::domain::errors::PaginationParams {
        page: query.page,
        per_page: query.per_page,
    }
}

/// Parses a path segment that must be a UUID.
///
/// A bad id in a path is a client error, not a server fault, and saying so keeps
/// a typo in a URL from looking like an outage.
pub fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(raw.trim())
        .map_err(|_| AppError::BadRequest(format!("{field} is not a valid id")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_uuid() {
        let raw = "6f9619ff-8b86-d011-b42d-00cf4fc964ff";
        assert_eq!(parse_uuid(raw, "id").unwrap().to_string(), raw);
    }

    #[test]
    fn a_bad_uuid_is_a_client_error() {
        assert!(matches!(
            parse_uuid("not-a-uuid", "roleId"),
            Err(AppError::BadRequest(_))
        ));
    }
}
