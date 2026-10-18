use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

/// Everything that can go wrong while managing accounts.
///
/// One error enum per aggregate, converted to an HTTP status by a single `From`
/// impl in the presentation layer. The domain names the failure; only the
/// presentation layer decides it means 404.
#[derive(Debug, Error)]
pub enum UserError {
    #[error("user not found")]
    NotFound,

    #[error("a user with discord id {0} already exists")]
    DiscordIdTaken(u64),

    #[error("discord id {0} is not a valid snowflake")]
    InvalidDiscordId(u64),

    #[error("username must be between 2 and 64 characters")]
    InvalidUsername,

    #[error("a user must keep at least one identity: discord id cannot be cleared")]
    MissingIdentity,

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl UserError {
    /// True when the caller could fix the request.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::NotFound
                | Self::DiscordIdTaken(_)
                | Self::InvalidDiscordId(_)
                | Self::InvalidUsername
                | Self::MissingIdentity
        )
    }
}

impl UserError {
    pub fn not_found_for(id: Uuid) -> Self {
        tracing::debug!(%id, "user not found");
        Self::NotFound
    }
}

/// Validates a username before it is stored.
///
/// Kept in the domain, not the DTO: a slash command and the dashboard both create
/// accounts, and the rule must be the same in both.
pub fn validate_username(username: &str) -> Result<String, UserError> {
    let trimmed = username.trim();
    if trimmed.chars().count() < 2 || trimmed.chars().count() > 64 {
        return Err(UserError::InvalidUsername);
    }
    Ok(trimmed.to_string())
}
