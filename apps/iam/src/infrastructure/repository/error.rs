//! The one place a `DbErr` becomes a `RepositoryError`.
//!
//! Every adapter goes through [`map_db_err`], so a unique violation reads the same
//! in every response body, and an unexpected failure is logged exactly once, at
//! the boundary where the driver's detail is still available.

use sea_orm::DbErr;
use sea_orm::error::SqlErr;

use crate::domain::errors::RepositoryError;

/// Translates a driver error into the domain's vocabulary.
///
/// `conflict` is what a human should read in an API response when a constraint
/// rejected the write — "a guild with that slug already exists" beats
/// `duplicate key value violates unique constraint "uq_guild_roles_guild_slug"`.
pub fn map_db_err(entity: &'static str, conflict: &'static str, error: DbErr) -> RepositoryError {
    match &error {
        DbErr::RecordNotFound(_) => RepositoryError::not_found(entity),
        DbErr::RecordNotInserted | DbErr::RecordNotUpdated => RepositoryError::conflict(conflict),
        _ => match error.sql_err() {
            Some(SqlErr::UniqueConstraintViolation(_)) => RepositoryError::conflict(conflict),
            Some(SqlErr::ForeignKeyConstraintViolation(_)) => RepositoryError::conflict(conflict),
            // `sql_err` only classifies those two, and nothing else the driver can
            // return is something a caller could act on — so it is a 500, and the
            // operator needs the detail in the log because the client will not get it.
            _ => {
                tracing::error!(entity, error = %error, "database operation failed");
                RepositoryError::database(error.to_string())
            }
        },
    }
}

/// [`map_db_err`] for a lookup that the caller already knows can miss.
///
/// `find_by_id` returning nothing is not a failure, so the repository answers with
/// `None`; this exists only so the miss is not silently swallowed when a write
/// depends on it.
pub fn not_found(entity: &'static str) -> RepositoryError {
    RepositoryError::not_found(entity)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn record_not_found_becomes_not_found() {
        let mapped = map_db_err(
            "guild",
            "already exists",
            DbErr::RecordNotFound("no row".to_owned()),
        );
        assert!(matches!(
            mapped,
            RepositoryError::NotFound { entity: "guild" }
        ));
    }

    #[test]
    fn a_record_not_updated_is_a_conflict_not_a_500() {
        // The guarded stock decrement relies on this: zero rows changed is a
        // business outcome, and an unguarded update that matched nothing is a bug
        // the caller can still act on by retrying against fresh state.
        let mapped = map_db_err("guild", "already exists", DbErr::RecordNotUpdated);
        assert!(matches!(mapped, RepositoryError::Conflict(_)));
        assert!(mapped.is_client_error());
    }

    #[test]
    fn an_unclassifiable_driver_error_becomes_a_database_error() {
        let mapped = map_db_err(
            "guild",
            "already exists",
            DbErr::Custom("the connection went away".to_owned()),
        );
        assert!(matches!(mapped, RepositoryError::Database(_)));
        assert!(!mapped.is_client_error());
    }
}
