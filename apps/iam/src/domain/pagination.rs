//! Re-export of the pagination vocabulary.
//!
//! `PaginationParams` and `Page` are used by every aggregate, so they live in
//! `domain::errors` next to `RepositoryError` — a repository's contract is
//! "return a page or a repository error", and splitting the two across modules
//! only made the signatures longer.

pub use super::errors::{Page, PaginationParams, not_found_for, validate_snowflake};
