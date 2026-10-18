use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum AutoRoleError {
    #[error("auto role not found")]
    NotFound(Uuid),

    #[error("trigger `{0}` is not one of join, delay or human")]
    UnknownTrigger(String),

    #[error("a delayed rule must wait between 1 second and 7 days")]
    InvalidDelay,

    #[error("this guild already grants role {0} automatically")]
    RoleAlreadyConfigured(u64),

    #[error("the pending grant is no longer due")]
    GrantNotDue,

    #[error("the pending grant exhausted its retries")]
    GrantExhausted,

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl AutoRoleError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::Repository(_))
    }
}

pub fn parse_trigger(value: &str) -> Result<super::entity::AutoRoleTrigger, AutoRoleError> {
    super::entity::AutoRoleTrigger::parse(value)
        .ok_or_else(|| AutoRoleError::UnknownTrigger(value.to_string()))
}

/// Validates the wait before a delayed grant.
///
/// A delay of zero is not an error — several rules treat it as "now" — but the
/// upper bound keeps the pending table meaningful.
pub fn validate_delay(
    _trigger: super::entity::AutoRoleTrigger,
    delay_seconds: i32,
) -> Result<(), AutoRoleError> {
    // Zero is allowed and means "grant now"; the upper bound is what keeps the
    // pending queue meaningful.
    if !(0..=super::entity::AutoRole::MAX_DELAY_SECONDS).contains(&delay_seconds) {
        return Err(AutoRoleError::InvalidDelay);
    }

    Ok(())
}

/// Guards the sweep against granting something that is not due yet.
pub fn ensure_due(
    due_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AutoRoleError> {
    if due_at > now {
        return Err(AutoRoleError::GrantNotDue);
    }
    Ok(())
}
