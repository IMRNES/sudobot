//! HS256 JWTs, behind the application's `TokenService` port.

use chrono::{DateTime, Utc};
use jsonwebtoken::{
    Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode,
    errors::{Error as JwtError, ErrorKind},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::ports::{TokenClaims, TokenError, TokenPair, TokenService, TokenType};

/// The seconds of clock skew tolerated on `exp`.
///
/// Both halves of the pair are checked by processes that may not share a clock;
/// five seconds absorbs that without meaningfully extending a token's life.
const LEEWAY_SECS: u64 = 5;

/// The token as it appears on the wire.
///
/// Deliberately not the domain's `TokenClaims`: the wire format is this adapter's
/// business, and a rename in the domain must not invalidate every live session.
/// `sub` and `jti` carry UUIDs as strings because the JWT spec wants strings there.
#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    /// The user the token speaks for.
    sub: String,
    /// Issued at, seconds since the epoch.
    iat: i64,
    /// Expiry, seconds since the epoch.
    exp: i64,
    /// `access` or `refresh`; what stops one being replayed as the other.
    token_type: String,
    /// The token's own id, so an issued refresh token can be revoked by id.
    jti: String,
}

/// Mints and validates HS256 tokens.
#[derive(Clone)]
pub struct JwtTokenService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    access_ttl_secs: i64,
    refresh_ttl_secs: i64,
}

impl JwtTokenService {
    /// Builds the service from the signing secret and the two lifetimes.
    ///
    /// Both keys are derived from one secret and kept: `jsonwebtoken`'s keys hold
    /// the material, so rebuilding them per call would copy the secret on every
    /// token issued.
    pub fn new(secret: &[u8], access_ttl_secs: i64, refresh_ttl_secs: i64) -> Self {
        Self {
            encoding_key: EncodingKey::from_secret(secret),
            decoding_key: DecodingKey::from_secret(secret),
            access_ttl_secs,
            refresh_ttl_secs,
        }
    }

    /// The validation every token this service accepts must satisfy.
    fn validation() -> Validation {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = LEEWAY_SECS;
        validation.validate_exp = true;
        // No token this service mints carries an `aud`, and the default `true` here
        // rejects exactly that combination the moment an `aud` ever appears — e.g.
        // from a token minted by another service sharing the secret. `iss`/`aud`
        // scoping belongs to a deployment's gateway, not here.
        validation.validate_aud = false;
        validation
    }

    fn mint(
        &self,
        user_id: Uuid,
        claims_type: TokenType,
        ttl_secs: i64,
        now: DateTime<Utc>,
        token_id: Uuid,
    ) -> Result<(String, DateTime<Utc>), TokenError> {
        // Truncated to the second before the claims are built, because the JWT's
        // `exp` is a whole number of seconds: the caller's `access_expires_at` would
        // otherwise carry sub-second precision the token does not, and the two would
        // disagree by up to a second about when the session ends.
        let expires_at = DateTime::from_timestamp(
            (now + chrono::Duration::seconds(ttl_secs.max(0))).timestamp(),
            0,
        )
        .unwrap_or(now);

        let claims = Claims {
            sub: user_id.to_string(),
            iat: now.timestamp(),
            exp: expires_at.timestamp(),
            token_type: claims_type.as_str().to_string(),
            jti: token_id.to_string(),
        };

        let token = encode(&Header::new(Algorithm::HS256), &claims, &self.encoding_key)
            .map_err(|error| TokenError::GenerationFailed(error.to_string()))?;

        Ok((token, expires_at))
    }
}

impl TokenService for JwtTokenService {
    async fn issue_pair(&self, user_id: Uuid, now: DateTime<Utc>) -> Result<TokenPair, TokenError> {
        let access_id = Uuid::new_v4();
        let refresh_id = Uuid::new_v4();

        let (access_token, access_expires_at) = self.mint(
            user_id,
            TokenType::Access,
            self.access_ttl_secs,
            now,
            access_id,
        )?;
        let (refresh_token, refresh_expires_at) = self.mint(
            user_id,
            TokenType::Refresh,
            self.refresh_ttl_secs,
            now,
            refresh_id,
        )?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            access_expires_at,
            refresh_expires_at,
            // The refresh token's `jti`, so the session can be revoked without the
            // token itself ever being stored.
            refresh_token_id: refresh_id,
        })
    }

    async fn verify(
        &self,
        token: &str,
        expected: TokenType,
        _now: DateTime<Utc>,
    ) -> Result<TokenClaims, TokenError> {
        // `decode` checks `exp` against the system clock, not the caller's `now`.
        // The `Clock` port exists for rules a test must be able to move — a cooldown,
        // a daily reset — and those are enforced in the use cases; a token's expiry
        // is the cryptography's own contract.
        let data = decode::<Claims>(token, &self.decoding_key, &Self::validation())
            .map_err(map_jwt_error)?;

        let claims = data.claims;

        let found = TokenType::parse(&claims.token_type).ok_or_else(|| {
            TokenError::Malformed(format!("unknown token type `{}`", claims.token_type))
        })?;

        // Checked after the signature, never before: an attacker can write any
        // `token_type` they like into an unsigned payload.
        if found != expected {
            return Err(TokenError::WrongTokenType {
                expected: expected.as_str().to_string(),
                found: found.as_str().to_string(),
            });
        }

        Ok(TokenClaims {
            subject: parse_uuid(&claims.sub, "sub")?,
            issued_at: DateTime::from_timestamp(claims.iat, 0).ok_or_else(|| {
                TokenError::Malformed(format!("`iat` {} is not a time", claims.iat))
            })?,
            expires_at: DateTime::from_timestamp(claims.exp, 0).ok_or_else(|| {
                TokenError::Malformed(format!("`exp` {} is not a time", claims.exp))
            })?,
            token_type: found,
            token_id: parse_uuid(&claims.jti, "jti")?,
        })
    }
}

/// Maps the library's error kinds onto the two the application distinguishes.
fn map_jwt_error(error: JwtError) -> TokenError {
    match error.kind() {
        ErrorKind::ExpiredSignature => TokenError::Expired,
        // A bad signature is not a malformed token to the caller — it is a token
        // that was not issued here — but the domain keeps no separate variant for
        // it, and the application maps both to "sign in again".
        ErrorKind::InvalidSignature => TokenError::InvalidSignature,
        ErrorKind::InvalidToken
        | ErrorKind::Base64(_)
        | ErrorKind::Json(_)
        | ErrorKind::Utf8(_) => TokenError::Malformed(error.to_string()),
        ErrorKind::InvalidAlgorithm => {
            TokenError::Malformed(format!("unexpected signing algorithm: {error}"))
        }
        ErrorKind::MissingRequiredClaim(claim) => {
            TokenError::Malformed(format!("missing claim `{claim}`"))
        }
        // `ErrorKind` is `#[non_exhaustive]`: a future variant must still produce a
        // rejection rather than a panic on an unexpected token.
        _ => TokenError::Malformed(error.to_string()),
    }
}

fn parse_uuid(value: &str, claim: &str) -> Result<Uuid, TokenError> {
    Uuid::parse_str(value)
        .map_err(|error| TokenError::Malformed(format!("`{claim}` is not a uuid: {error}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    const SECRET: &[u8] = b"0123456789012345678901234567890123456789";

    fn service() -> JwtTokenService {
        JwtTokenService::new(SECRET, 900, 2_592_000)
    }

    #[tokio::test]
    async fn a_pair_round_trips_with_its_claims() {
        let service = service();
        let user_id = Uuid::new_v4();
        let now = Utc::now();

        let pair = service.issue_pair(user_id, now).await.unwrap();

        let access = service
            .verify(&pair.access_token, TokenType::Access, now)
            .await
            .unwrap();
        assert_eq!(access.subject, user_id);
        assert_eq!(access.token_type, TokenType::Access);
        assert_eq!(access.expires_at, pair.access_expires_at);

        let refresh = service
            .verify(&pair.refresh_token, TokenType::Refresh, now)
            .await
            .unwrap();
        assert_eq!(refresh.token_id, pair.refresh_token_id);
        assert_eq!(refresh.expires_at, pair.refresh_expires_at);
    }

    #[tokio::test]
    async fn a_refresh_token_is_not_an_access_token() {
        let service = service();
        let now = Utc::now();
        let pair = service.issue_pair(Uuid::new_v4(), now).await.unwrap();

        // The whole point of the `token_type` claim: a long-lived token must not be
        // usable against an endpoint that expects a short-lived one.
        let error = service
            .verify(&pair.refresh_token, TokenType::Access, now)
            .await
            .unwrap_err();

        assert!(matches!(error, TokenError::WrongTokenType { .. }));
    }

    #[tokio::test]
    async fn a_token_signed_with_another_secret_is_rejected() {
        let other = JwtTokenService::new(b"a-completely-different-secret-value!!", 900, 900);
        let now = Utc::now();
        let pair = other.issue_pair(Uuid::new_v4(), now).await.unwrap();

        let error = service()
            .verify(&pair.access_token, TokenType::Access, now)
            .await
            .unwrap_err();

        assert!(matches!(error, TokenError::InvalidSignature));
    }

    #[tokio::test]
    async fn garbage_is_malformed_rather_than_invalid() {
        let error = service()
            .verify("not.a.token", TokenType::Access, Utc::now())
            .await
            .unwrap_err();

        assert!(matches!(error, TokenError::Malformed(_)));
    }

    #[tokio::test]
    async fn an_expired_token_reports_expiry() {
        // Issued two hours ago with a 900 second lifetime, so it is past its `exp`
        // plus the five second leeway.
        let service = service();
        let issued = Utc::now() - chrono::Duration::hours(2);
        let pair = service.issue_pair(Uuid::new_v4(), issued).await.unwrap();

        let error = service
            .verify(&pair.access_token, TokenType::Access, Utc::now())
            .await
            .unwrap_err();

        assert!(matches!(error, TokenError::Expired));
    }
}
