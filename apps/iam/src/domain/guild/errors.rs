use thiserror::Error;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum GuildError {
    #[error("guild not found")]
    NotFound,

    #[error("guild name must be between 1 and 100 characters")]
    InvalidName,

    #[error("the bot is not present in guild {0}")]
    BotNotPresent(u64),

    #[error("you do not administer guild {0}")]
    NotGuildAdmin(u64),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl GuildError {
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::NotFound | Self::InvalidName | Self::BotNotPresent(_) | Self::NotGuildAdmin(_)
        )
    }

    /// Mapping target for the "not allowed here" case, so every handler that
    /// checks guild authority produces the same error.
    pub fn forbidden(_: ()) -> Self {
        Self::NotGuildAdmin(0)
    }
}

pub fn validate_guild_name(name: &str) -> Result<String, GuildError> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 100 {
        return Err(GuildError::InvalidName);
    }
    Ok(trimmed.to_string())
}
