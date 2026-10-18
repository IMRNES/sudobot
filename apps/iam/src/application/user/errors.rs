use thiserror::Error;

use crate::domain::user::UserError;

/// The failures an application-level account operation can produce.
///
/// Wraps the domain's [`UserError`] rather than replacing it — the use cases do
/// not invent new *domain* failures, and a caller that already handles
/// `UserError` keeps working through the `From` impl. It exists for the one rule
/// that is about the installation rather than about an account: the platform must
/// always keep an operator.
///
/// The alternative was to widen the frozen domain enum with a `LastSuperAdmin`
/// variant, or to bend `MissingIdentity` — which means "a user with no Discord id"
/// and would have produced a message about the wrong thing entirely — into
/// service. Neither is honest, so the variant lives here, where the rule is
/// enforced.
#[derive(Debug, Error)]
pub enum UserAdminError {
    #[error(transparent)]
    User(#[from] UserError),

    /// Demoting this account would leave the platform with nobody who can
    /// administer it.
    #[error("the platform must keep at least one super admin")]
    LastSuperAdmin,
}

impl UserAdminError {
    /// True when the caller could fix the request.
    pub fn is_client_error(&self) -> bool {
        match self {
            Self::User(error) => error.is_client_error(),
            Self::LastSuperAdmin => true,
        }
    }
}

impl From<crate::domain::errors::RepositoryError> for UserAdminError {
    fn from(error: crate::domain::errors::RepositoryError) -> Self {
        Self::User(UserError::from(error))
    }
}
