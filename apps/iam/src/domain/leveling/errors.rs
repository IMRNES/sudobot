use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum LevelingError {
    #[error("leveling is disabled in this guild")]
    Disabled,

    #[error("profile not found")]
    ProfileNotFound,

    #[error("reward not found")]
    RewardNotFound(Uuid),

    #[error("xp bounds are invalid: min {min} is greater than max {max}")]
    InvalidXpRange { min: i32, max: i32 },

    #[error("xp bounds must be between 1 and 1000")]
    XpOutOfBounds,

    #[error("the cooldown must be between 0 and 3600 seconds")]
    InvalidCooldown,

    #[error("level must be at least 1")]
    InvalidLevel,

    #[error("a reward already exists for level {0}")]
    RewardLevelTaken(i32),

    #[error("unknown level curve `{0}`; expected mrn, linear or flat")]
    UnknownCurve(String),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl LevelingError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::Repository(_))
    }
}

/// Validates the XP-per-message range.
///
/// Both ends matter: a min above the max makes the roll panic or return nothing,
/// and a range topping out in the thousands turns a chat server into a
/// level-100-in-an-hour server.
pub fn validate_xp_range(min: i32, max: i32) -> Result<(i32, i32), LevelingError> {
    if !(1..=1000).contains(&min) || !(1..=1000).contains(&max) {
        return Err(LevelingError::XpOutOfBounds);
    }
    if min > max {
        return Err(LevelingError::InvalidXpRange { min, max });
    }
    Ok((min, max))
}

pub fn validate_cooldown(secs: i32) -> Result<i32, LevelingError> {
    if !(0..=3600).contains(&secs) {
        return Err(LevelingError::InvalidCooldown);
    }
    Ok(secs)
}

/// Rejects a reward level the ladder does not describe.
///
/// Level 0 is allowed: it is where every member starts, and the ladder's first rung
/// is a level-0 title specifically so that a new arrival has something to wear.
/// Only a negative level is refused, which is a client sending nonsense rather than
/// a rung.
pub fn validate_level(level: i32) -> Result<i32, LevelingError> {
    if level < 0 {
        return Err(LevelingError::InvalidLevel);
    }
    Ok(level)
}

/// Rejects a curve name the bot cannot compute.
pub fn parse_curve(value: &str) -> Result<super::entity::LevelCurve, LevelingError> {
    super::entity::LevelCurve::parse(value)
        .ok_or_else(|| LevelingError::UnknownCurve(value.to_string()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn rejects_inverted_range() {
        assert!(validate_xp_range(30, 10).is_err());
        assert!(validate_xp_range(15, 25).is_ok());
    }

    #[test]
    fn rejects_zero_xp_gain() {
        // A min of 0 would let members farm messages for nothing.
        assert!(validate_xp_range(0, 25).is_err());
    }
}
