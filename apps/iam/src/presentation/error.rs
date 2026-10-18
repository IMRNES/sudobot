use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use tracing::{error, warn};

use crate::domain::auth::AuthError;
use crate::domain::autorole::AutoRoleError;
use crate::domain::economy::EconomyError;
use crate::domain::errors::RepositoryError;
use crate::domain::guild::GuildError;
use crate::domain::guild_member::GuildMemberError;
use crate::domain::leveling::LevelingError;
use crate::domain::rbac::RbacError;
use crate::domain::user::UserError;
use crate::domain::welcome::WelcomeError;

/// The API's entire error vocabulary.
///
/// Six shapes, mapped to status codes in one place. Every domain error enum
/// converts into this through a `From` impl below, which is what keeps a handler
/// able to write `?` and still produce a correct status without matching.
#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    /// Authenticated, but this token may no longer be used. The client should
    /// refresh and retry, which is why it is not simply a 401.
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    /// The caller is out of attempts. Kept distinct from `Forbidden` so clients
    /// can back off rather than re-authenticate.
    TooManyRequests,
    /// The feature is not available on this installation — a configuration gap on
    /// the server, not a fault in the request. Distinct from `Internal` because the
    /// message is actionable ("login is not configured") and safe to show, and
    /// distinct from `Forbidden` because retrying or re-authenticating will not
    /// help.
    Unavailable(String),
    Internal(String),
}

/// The JSON body every error produces.
///
/// `code` is present only when a client must branch on the reason (a refreshable
/// token, a lost stock race). Human-readable text alone is not a contract.
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

impl AppError {
    /// A stable, machine-readable tag for the cases clients act on.
    fn code(&self) -> Option<&'static str> {
        match self {
            Self::Unauthorized(reason) if reason == TOKEN_EXPIRED => Some(TOKEN_EXPIRED),
            Self::Conflict(_) => Some("conflict"),
            Self::Unavailable(_) => Some(NOT_CONFIGURED),
            Self::TooManyRequests => Some("rate_limited"),
            Self::Forbidden(reason) if reason == PERMISSION_DENIED => Some(PERMISSION_DENIED),
            _ => None,
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A message safe to send to a client.
    ///
    /// Internal errors deliberately reveal nothing: the detail is logged with the
    /// trace id instead, and a client that needs it can quote the id.
    fn message(&self) -> String {
        match self {
            Self::BadRequest(message)
            | Self::Unauthorized(message)
            | Self::Forbidden(message)
            | Self::NotFound(message)
            | Self::Conflict(message)
            | Self::Unavailable(message) => message.clone(),
            Self::TooManyRequests => "too many requests; try again shortly".to_string(),
            Self::Internal(_) => "an internal error occurred".to_string(),
        }
    }
}

/// Marks a 401 the client should answer by refreshing its access token.
pub const TOKEN_EXPIRED: &str = "token_expired";
/// Marks a 403 the client should render as "you lack permission X".
pub const PERMISSION_DENIED: &str = "permission_denied";
/// Marks a 503 the client should render as "this installation cannot do that yet".
pub const NOT_CONFIGURED: &str = "not_configured";

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Expected refusals are logged at `warn`; genuine faults at `error`, with
        // the detail that the response deliberately withholds.
        match &self {
            Self::Internal(detail) => error!(detail, "request failed with an internal error"),
            other => warn!(status = status.as_u16(), reason = %other.message(), "request refused"),
        }

        let body = ErrorBody {
            error: self.message(),
            code: self.code(),
        };

        (status, Json(body)).into_response()
    }
}

impl From<RepositoryError> for AppError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::NotFound { entity } => Self::NotFound(format!("{entity} not found")),
            RepositoryError::Conflict(message) => Self::Conflict(message),
            RepositoryError::InvalidData { field, value } => {
                Self::Internal(format!("stored value for `{field}` is invalid: {value}"))
            }
            RepositoryError::Database(detail) => Self::Internal(detail),
        }
    }
}

impl From<AuthError> for AppError {
    fn from(error: AuthError) -> Self {
        match error {
            AuthError::InvalidOAuthCode
            | AuthError::StateMismatch
            | AuthError::StateExpired
            | AuthError::ProfileUnavailable => Self::Unauthorized(error.to_string()),

            AuthError::InvalidAccessToken => Self::Unauthorized(TOKEN_EXPIRED.to_string()),
            AuthError::AccessTokenExpired => Self::Unauthorized(TOKEN_EXPIRED.to_string()),

            // A dead refresh token means "sign in again", not "try again".
            AuthError::InvalidRefreshToken
            | AuthError::RefreshTokenExpired
            | AuthError::RefreshTokenReused => Self::Unauthorized(error.to_string()),

            // A server-side configuration gap, not a bad request: the caller did
            // nothing wrong and cannot fix it by retrying. 503 is the honest
            // status — the feature is unavailable, and it is not this request's
            // fault.
            AuthError::LoginNotConfigured => Self::Unavailable(error.to_string()),

            AuthError::UserNotFound => Self::NotFound(error.to_string()),
            AuthError::NotAGuildMember(_) => Self::Forbidden(error.to_string()),
            AuthError::DiscordUnavailable(detail) => Self::Internal(detail),
            AuthError::Repository(inner) => inner.into(),
        }
    }
}

impl From<UserError> for AppError {
    fn from(error: UserError) -> Self {
        match error {
            UserError::NotFound => Self::NotFound(error.to_string()),
            UserError::DiscordIdTaken(_)
            | UserError::InvalidDiscordId(_)
            | UserError::InvalidUsername
            | UserError::MissingIdentity => Self::BadRequest(error.to_string()),
            UserError::Repository(inner) => inner.into(),
        }
    }
}

impl From<GuildError> for AppError {
    fn from(error: GuildError) -> Self {
        match error {
            GuildError::NotFound => Self::NotFound(error.to_string()),
            GuildError::InvalidName => Self::BadRequest(error.to_string()),
            GuildError::BotNotPresent(_) | GuildError::NotGuildAdmin(_) => {
                Self::Forbidden(error.to_string())
            }
            GuildError::Repository(inner) => inner.into(),
        }
    }
}

impl From<GuildMemberError> for AppError {
    fn from(error: GuildMemberError) -> Self {
        match error {
            GuildMemberError::NotFound => Self::NotFound(error.to_string()),
            GuildMemberError::CannotDemoteOwner
            | GuildMemberError::CannotChangeSelf
            | GuildMemberError::WouldRemoveLastHolder(_) => Self::Forbidden(error.to_string()),
            GuildMemberError::Repository(inner) => inner.into(),
        }
    }
}

impl From<RbacError> for AppError {
    fn from(error: RbacError) -> Self {
        match error {
            RbacError::RoleNotFound => Self::NotFound(error.to_string()),
            // A rbac denial is the one 403 clients render with the permission name.
            RbacError::PermissionDenied(_) => Self::Forbidden(PERMISSION_DENIED.to_string()),
            RbacError::UnknownPermission(_) => Self::BadRequest(error.to_string()),
            RbacError::SystemRoleImmutable(_)
            | RbacError::RoleInUse(_)
            | RbacError::WouldRemoveLastAdmin(_) => Self::Conflict(error.to_string()),
            RbacError::Repository(inner) => inner.into(),
        }
    }
}

impl From<EconomyError> for AppError {
    fn from(error: EconomyError) -> Self {
        match error {
            EconomyError::AccountNotFound => Self::NotFound(error.to_string()),
            EconomyError::ItemNotFound(_) => Self::NotFound(error.to_string()),
            EconomyError::InsufficientFunds { .. } => Self::BadRequest(error.to_string()),
            EconomyError::DailyNotReady => Self::Conflict(error.to_string()),
            EconomyError::NonPositiveAmount => Self::BadRequest(error.to_string()),
            EconomyError::SelfTransfer => Self::BadRequest(error.to_string()),
            EconomyError::SoldOut | EconomyError::StockRaceLost => {
                Self::Conflict(error.to_string())
            }
            EconomyError::NotForSale => Self::BadRequest(error.to_string()),
            EconomyError::Repository(inner) => inner.into(),
        }
    }
}

impl From<LevelingError> for AppError {
    fn from(error: LevelingError) -> Self {
        match error {
            LevelingError::ProfileNotFound => Self::NotFound(error.to_string()),
            LevelingError::RewardNotFound(_) => Self::NotFound(error.to_string()),
            LevelingError::Disabled => Self::Conflict(error.to_string()),
            LevelingError::InvalidXpRange { .. }
            | LevelingError::XpOutOfBounds
            | LevelingError::InvalidCooldown
            | LevelingError::InvalidLevel
            | LevelingError::UnknownCurve(_) => Self::BadRequest(error.to_string()),
            LevelingError::RewardLevelTaken(_) => Self::Conflict(error.to_string()),
            LevelingError::Repository(inner) => inner.into(),
        }
    }
}

impl From<WelcomeError> for AppError {
    fn from(error: WelcomeError) -> Self {
        match error {
            WelcomeError::ConfigNotFound => Self::NotFound(error.to_string()),
            WelcomeError::PanelNotFound => Self::NotFound(error.to_string()),
            WelcomeError::OptionNotFound => Self::NotFound(error.to_string()),
            WelcomeError::InvalidMessage
            | WelcomeError::UnknownPlaceholder(_)
            | WelcomeError::TooManyOptions(_)
            | WelcomeError::UnknownBehavior(_)
            | WelcomeError::MissingChannel => Self::BadRequest(error.to_string()),
            WelcomeError::RoleNotAssignable(_) => Self::Conflict(error.to_string()),
            WelcomeError::Repository(inner) => inner.into(),
        }
    }
}

impl From<AutoRoleError> for AppError {
    fn from(error: AutoRoleError) -> Self {
        match error {
            AutoRoleError::NotFound(_) => Self::NotFound(error.to_string()),
            AutoRoleError::UnknownTrigger(_)
            | AutoRoleError::InvalidDelay
            | AutoRoleError::GrantNotDue
            | AutoRoleError::GrantExhausted => Self::BadRequest(error.to_string()),
            AutoRoleError::RoleAlreadyConfigured(_) => Self::Conflict(error.to_string()),
            AutoRoleError::Repository(inner) => inner.into(),
        }
    }
}

impl From<crate::application::ports::TokenError> for AppError {
    fn from(error: crate::application::ports::TokenError) -> Self {
        match error {
            crate::application::ports::TokenError::Expired => {
                Self::Unauthorized(TOKEN_EXPIRED.to_string())
            }
            crate::application::ports::TokenError::Malformed(detail) => Self::Unauthorized(detail),
            crate::application::ports::TokenError::InvalidSignature => {
                Self::Unauthorized("the token signature is not valid".to_string())
            }
            crate::application::ports::TokenError::WrongTokenType { expected, .. } => {
                Self::Unauthorized(format!("a {expected} token is required"))
            }
            crate::application::ports::TokenError::GenerationFailed(detail) => {
                Self::Internal(detail)
            }
        }
    }
}

/// Turns a validation failure into a 400 carrying every problem at once.
///
/// One message per field rather than the first failure: a form that reports one
/// error at a time makes the user submit five times, and they blame the form.
pub fn validation_failed(issues: &[(String, String)]) -> AppError {
    let rendered = issues
        .iter()
        .map(|(field, message)| format!("{field}: {message}"))
        .collect::<Vec<_>>()
        .join("; ");

    AppError::BadRequest(rendered)
}
