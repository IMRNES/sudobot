use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A refresh token, stored as a hash.
///
/// The token itself is never persisted. A stolen database dump then yields no
/// usable sessions, and a leaked token still cannot be reverse-engineered back
/// out of the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    /// SHA-256 of the token string, hex-encoded.
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl RefreshToken {
    /// Whether this token may still be exchanged.
    ///
    /// Takes `now` rather than reading the clock, so the expiry rule is testable.
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.expires_at > now
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

/// A refresh session about to be created.
#[derive(Debug, Clone)]
pub struct NewRefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}
