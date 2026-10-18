//! Shared pagination helpers.
//!
//! The domain and `paginator-rs` both have a `PaginationParams`, with different
//! fields and different validation. Converting in one place keeps every repository
//! from inventing its own interpretation of "page 0" or "per_page 500".

use paginator_rs::PaginationParams as PaginatorParams;
use paginator_sea_orm::PaginateSeaOrm;
use sea_orm::{ConnectionTrait, EntityTrait, FromQueryResult, Select};
use serde::Serialize;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};

/// The largest page a client may ask for.
///
/// `paginator-rs` clamps to 100 internally, but doing it here too means the
/// repository's own arithmetic (offsets, totals) uses the same number the query
/// does, rather than one value for the count and another for the rows.
const MAX_PER_PAGE: u32 = 100;

/// Converts the domain's pagination request into the one the paginator takes.
pub fn to_paginator_params(params: &PaginationParams) -> PaginatorParams {
    PaginatorParams::new(params.page, params.per_page.min(MAX_PER_PAGE))
}

/// Runs a paginated query and maps the rows into domain entities.
///
/// `entity` and `conflict` are the error-context strings the caller's other
/// queries use, so a failure here reads like a failure anywhere else in that
/// repository.
pub async fn paginate_models<C, E, T, F>(
    select: Select<E>,
    db: &C,
    params: &PaginationParams,
    entity: &'static str,
    conflict: &'static str,
    map: F,
) -> Result<Page<T>, RepositoryError>
where
    C: ConnectionTrait,
    E: EntityTrait,
    E::Model: Serialize + Send + Sync + FromQueryResult,
    F: FnMut(E::Model) -> T,
{
    let request = to_paginator_params(params);
    let response = select.paginate_with(db, &request).await.map_err(|error| {
        // The paginator's own error carries the driver's message; it is not a
        // `DbErr` any more, so it is classified here rather than in `map_db_err`.
        let message = error.to_string();
        if message.contains("Unique") {
            RepositoryError::conflict(conflict)
        } else {
            tracing::error!(entity, error = %message, "paginated query failed");
            RepositoryError::database(message)
        }
    })?;

    let total = u64::from(response.meta.total.unwrap_or(response.data.len() as u32));

    Ok(Page::new(
        response.data.into_iter().map(map).collect(),
        params,
        total,
    ))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn per_page_is_clamped_to_what_the_database_will_serve() {
        let params = PaginationParams {
            page: 1,
            per_page: 5_000,
        };
        assert_eq!(to_paginator_params(&params).per_page, MAX_PER_PAGE);
    }

    #[test]
    fn page_zero_is_treated_as_the_first_page() {
        // `PaginationParams::new` clamps; without that, `offset()` would underflow.
        let params = PaginationParams {
            page: 0,
            per_page: 20,
        };
        assert_eq!(to_paginator_params(&params).page, 1);
    }
}
