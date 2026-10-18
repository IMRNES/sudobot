use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum WelcomeError {
    #[error("welcome configuration not found")]
    ConfigNotFound,

    #[error("role panel not found")]
    PanelNotFound,

    #[error("panel option not found")]
    OptionNotFound,

    #[error("the welcome message must be between 1 and 2000 characters")]
    InvalidMessage,

    #[error("a welcome template may use {{user}}, {{mention}}, {{guild}} and {{memberCount}} only")]
    UnknownPlaceholder(String),

    #[error("a panel holds at most 25 options; this would add {} too many", .0)]
    TooManyOptions(usize),

    #[error("behaviour `{0}` is not one of toggle, add, remove or exclusive")]
    UnknownBehavior(String),

    #[error("a panel cannot be published without a channel")]
    MissingChannel,

    #[error("this option's role is not assignable by the bot in that guild")]
    RoleNotAssignable(u64),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl WelcomeError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::Repository(_))
    }
}

/// The placeholders a welcome template may contain.
pub const ALLOWED_PLACEHOLDERS: &[&str] = &["{user}", "{mention}", "{guild}", "{memberCount}"];

/// Rejects a template that references something the renderer will not substitute.
///
/// Checked at save time rather than at join time: a typo like `{membercount}`
/// would otherwise post literally in every welcome message and never be noticed.
pub fn validate_template(message: &str) -> Result<String, WelcomeError> {
    let trimmed = message.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 2000 {
        return Err(WelcomeError::InvalidMessage);
    }

    // Scan for `{...}` groups and confirm each one is a supported name.
    let mut rest = trimmed;
    while let Some(start) = rest.find('{') {
        let after = &rest[start..];
        let Some(end) = after.find('}') else {
            return Err(WelcomeError::UnknownPlaceholder(after.to_string()));
        };
        let token = &after[..=end];
        if !ALLOWED_PLACEHOLDERS.contains(&token) {
            return Err(WelcomeError::UnknownPlaceholder(token.to_string()));
        }
        rest = &after[end + 1..];
    }

    Ok(trimmed.to_string())
}

/// Confirms a panel still fits inside Discord's component limits.
pub fn validate_option_count(existing: usize, adding: usize) -> Result<(), WelcomeError> {
    let limit = super::entity::RolePanelOption::MAX_PER_PANEL;
    if existing + adding > limit {
        return Err(WelcomeError::TooManyOptions(existing + adding - limit));
    }
    Ok(())
}

pub fn parse_behavior(value: &str) -> Result<super::entity::OptionBehavior, WelcomeError> {
    super::entity::OptionBehavior::parse(value)
        .ok_or_else(|| WelcomeError::UnknownBehavior(value.to_string()))
}

/// Validates a button's numeric style.
pub fn validate_button_style(style: i16) -> Result<i16, WelcomeError> {
    if (1..=4).contains(&style) {
        Ok(style)
    } else {
        Err(WelcomeError::UnknownBehavior(format!(
            "button style {style}; expected 1-4"
        )))
    }
}

pub fn panel_not_found(id: Uuid) -> WelcomeError {
    tracing::debug!(%id, "role panel not found");
    WelcomeError::PanelNotFound
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn accepts_supported_placeholders() {
        let rendered = "{mention} joined {guild} as member #{memberCount}";
        assert!(validate_template(rendered).is_ok());
    }

    #[test]
    fn rejects_a_typo_in_placeholder_case() {
        // The classic mistake: right name, wrong case.
        assert!(validate_template("welcome {membercount}!").is_err());
    }

    #[test]
    fn rejects_an_unclosed_brace() {
        assert!(validate_template("welcome {user").is_err());
    }

    #[test]
    fn enforces_discords_option_limit() {
        assert!(validate_option_count(24, 1).is_ok());
        assert!(validate_option_count(25, 1).is_err());
    }
}
