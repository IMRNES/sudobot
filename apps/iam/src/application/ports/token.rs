use chrono::{DateTime, Utc};
use std::future::Future;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum TokenError {
    #[error("token generation failed: {0}")]
    GenerationFailed(String),

    #[error("the token is malformed: {0}")]
    Malformed(String),

    #[error("the token has expired")]
    Expired,

    #[error("the token signature is not valid")]
    InvalidSignature,

    #[error("the token is not valid for this use: expected {expected}, found {found}")]
    WrongTokenType { expected: String, found: String },
}

/// The claims the application puts in a token.
///
/// A plain struct, not a serde-shaped mirror of any library's type: the wire
/// format is the adapter's concern, and `token_type` is what stops a refresh
/// token from being replayed as an access token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenClaims {
    /// The user the token speaks for.
    pub subject: Uuid,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub token_type: TokenType,
    /// Unique per token, so a refresh token can be revoked by id.
    pub token_id: Uuid,
}

/// The two audiences a token can be minted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenType {
    Access,
    Refresh,
}

impl TokenType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Refresh => "refresh",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "access" => Some(Self::Access),
            "refresh" => Some(Self::Refresh),
            _ => None,
        }
    }
}

/// A freshly minted access/refresh pair.
#[derive(Debug, Clone)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub access_expires_at: DateTime<Utc>,
    pub refresh_expires_at: DateTime<Utc>,
    /// The refresh token's `jti`, stored so it can be revoked without keeping the
    /// token itself.
    pub refresh_token_id: Uuid,
}

/// Minting and checking tokens.
pub trait TokenService: Send + Sync {
    /// Mints a matching access/refresh pair for one user.
    fn issue_pair(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<TokenPair, TokenError>> + Send;

    /// Validates a token and returns its claims.
    ///
    /// Rejects a token whose `token_type` is not the expected one, so a refresh
    /// token cannot be used to call an authenticated endpoint.
    fn verify(
        &self,
        token: &str,
        expected: TokenType,
        now: DateTime<Utc>,
    ) -> impl Future<Output = Result<TokenClaims, TokenError>> + Send;
}
