use thiserror::Error;
use uuid::Uuid;

/// What can go wrong inside a repository, in terms the domain understands.
///
/// Deliberately small: a repository either could not find the row, could not
/// write it because of a constraint, or hit an infrastructure failure. Anything
/// else is a bug the domain has no business modelling. Adapters translate their
/// own error types into these three (`DbErr` → `RepositoryError` in
/// `infrastructure`), which is the only place that knows a database exists.
#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("{entity} not found")]
    NotFound { entity: &'static str },

    /// A uniqueness or foreign-key constraint rejected the write. The message is
    /// written for a human reading an API response, not for a log line.
    #[error("{0}")]
    Conflict(String),

    /// The storage layer failed in a way the caller cannot act on. Logged with
    /// its source; surfaced to clients as an opaque 500.
    #[error("database error: {0}")]
    Database(String),

    /// Storage returned data that cannot represent a domain value — a role slug
    /// outside the known set, a negative balance where none is allowed. Distinct
    /// from `Database` because it means the data is wrong, not the connection.
    #[error("invalid stored value for {field}: {value}")]
    InvalidData { field: &'static str, value: String },
}

impl RepositoryError {
    pub fn not_found(entity: &'static str) -> Self {
        Self::NotFound { entity }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::Database(message.into())
    }

    pub fn invalid_data(field: &'static str, value: impl std::fmt::Display) -> Self {
        Self::InvalidData {
            field,
            value: value.to_string(),
        }
    }

    /// True when the caller could fix the request rather than retry it.
    ///
    /// Handlers use this to choose between 4xx and 5xx without matching on every
    /// variant at every call site.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::NotFound { .. } | Self::Conflict(_) | Self::InvalidData { .. }
        )
    }
}

/// A pagination request, expressed in domain terms.
///
/// The domain does not know that HTTP query strings exist, so the presentation
/// layer converts `PaginationQuery` into this before calling a use case.
#[derive(Debug, Clone)]
pub struct PaginationParams {
    pub page: u32,
    pub per_page: u32,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: 1,
            per_page: 20,
        }
    }
}

impl PaginationParams {
    /// Rows to skip. Saturating because a client can ask for page 4 billion and
    /// an overflow panic is not an acceptable answer to a bad query string.
    pub fn offset(&self) -> u64 {
        u64::from(self.page.saturating_sub(1)) * u64::from(self.per_page)
    }
}

/// One page of results plus the metadata a client needs to ask for the next one.
#[derive(Debug, Clone)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub page: u32,
    pub per_page: u32,
    pub total: u64,
}

impl<T> Page<T> {
    pub fn new(data: Vec<T>, params: &PaginationParams, total: u64) -> Self {
        Self {
            data,
            page: params.page,
            per_page: params.per_page,
            total,
        }
    }

    pub fn total_pages(&self) -> u32 {
        if self.per_page == 0 {
            return 0;
        }
        self.total.div_ceil(u64::from(self.per_page)) as u32
    }

    /// Maps the page's items while preserving the metadata.
    ///
    /// This is how a repository returns ORM models and a use case hands the
    /// handler domain entities without rebuilding the pagination bookkeeping.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            data: self.data.into_iter().map(f).collect(),
            page: self.page,
            per_page: self.per_page,
            total: self.total,
        }
    }
}

/// Snowflake identifiers that must be non-zero to be meaningful.
///
/// Discord never issues id 0, so a zero arriving from storage or a request is a
/// bug worth catching at the boundary rather than passing along.
pub fn validate_snowflake(value: u64, field: &'static str) -> Result<u64, RepositoryError> {
    if value == 0 {
        return Err(RepositoryError::invalid_data(field, value));
    }
    Ok(value)
}

/// Helper for adapters that need to report which row was missing.
pub fn not_found_for(entity: &'static str, id: Uuid) -> RepositoryError {
    tracing::debug!(%id, entity, "repository lookup miss");
    RepositoryError::not_found(entity)
}
