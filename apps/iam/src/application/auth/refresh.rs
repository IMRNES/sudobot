use tracing::{info, warn};
use uuid::Uuid;

use crate::application::ports::{Clock, TokenError, TokenService, TokenType};
use crate::domain::auth::{AuthError, RefreshTokenRepository, hash_refresh_token};

/// A refresh request: the long-lived token plus nothing else.
///
/// The user is identified by the token itself, never by a parameter, so a caller
/// cannot refresh somebody else's session.
#[derive(Debug, Clone)]
pub struct RefreshCommand {
    pub refresh_token: String,
}

/// Rotates a refresh token into a fresh pair.
///
/// Rotation, not reuse: the presented token is revoked as part of the exchange.
/// A refresh token that gets replayed therefore fails the second time, which is
/// the signal that it was stolen.
pub struct RefreshUseCase<R, T, C> {
    refresh_tokens: R,
    tokens: T,
    clock: C,
}

impl<R, T, C> RefreshUseCase<R, T, C>
where
    R: RefreshTokenRepository,
    T: TokenService,
    C: Clock,
{
    pub fn new(refresh_tokens: R, tokens: T, clock: C) -> Self {
        Self {
            refresh_tokens,
            tokens,
            clock,
        }
    }

    pub async fn execute(
        &self,
        command: RefreshCommand,
    ) -> Result<crate::application::ports::TokenPair, AuthError> {
        let now = self.clock.now();

        // 1. The signature check comes first: it is cheap and rejects anything
        //    that is not a token this service minted.
        let claims = self
            .tokens
            .verify(&command.refresh_token, TokenType::Refresh, now)
            .await
            .map_err(map_token_error)?;

        // 2. Then the stored record, which is what makes revocation possible.
        let hash = hash_refresh_token(&command.refresh_token);
        let stored = self
            .refresh_tokens
            .find_by_hash(&hash)
            .await?
            .ok_or(AuthError::InvalidRefreshToken)?;

        if stored.is_revoked() {
            // A revoked token being presented means either a replay of a stolen
            // token or a client that lost the rotation race. Both are worth a
            // warning; the safe answer is to refuse and make them sign in again.
            warn!(user_id = %stored.user_id, "a revoked refresh token was presented");
            return Err(AuthError::RefreshTokenReused);
        }

        if !stored.is_usable(now) {
            return Err(AuthError::RefreshTokenExpired);
        }

        if stored.user_id != claims.subject {
            warn!(
                token_user = %stored.user_id,
                claim_user = %claims.subject,
                "refresh token subject does not match its stored owner"
            );
            return Err(AuthError::InvalidRefreshToken);
        }

        // 3. Rotate.
        let pair = self
            .tokens
            .issue_pair(claims.subject, now)
            .await
            .map_err(map_token_error)?;

        self.refresh_tokens.revoke(stored.id, now).await?;
        self.refresh_tokens
            .create(crate::domain::auth::NewRefreshToken {
                id: pair.refresh_token_id,
                user_id: claims.subject,
                token_hash: hash_refresh_token(&pair.refresh_token),
                expires_at: pair.refresh_expires_at,
            })
            .await?;

        info!(user_id = %claims.subject, "refresh token rotated");
        Ok(pair)
    }
}

/// Ends a single session, or every session for a user.
#[derive(Debug, Clone)]
pub struct LogoutCommand {
    pub refresh_token: Option<String>,
    pub user_id: Uuid,
    /// When true, revokes every live token rather than just the presented one.
    pub everywhere: bool,
}

pub struct LogoutUseCase<R, C> {
    refresh_tokens: R,
    clock: C,
}

impl<R, C> LogoutUseCase<R, C>
where
    R: RefreshTokenRepository,
    C: Clock,
{
    pub fn new(refresh_tokens: R, clock: C) -> Self {
        Self {
            refresh_tokens,
            clock,
        }
    }

    /// Returns how many sessions were ended.
    pub async fn execute(&self, command: LogoutCommand) -> Result<u64, AuthError> {
        let now = self.clock.now();

        let revoked = if command.everywhere {
            self.refresh_tokens
                .revoke_all_for_user(command.user_id, now)
                .await?
        } else {
            match command.refresh_token {
                Some(token) => {
                    let hash = hash_refresh_token(&token);
                    match self.refresh_tokens.find_by_hash(&hash).await? {
                        // Revoking an unknown token is not an error: the caller
                        // wanted it dead and it is dead. Reporting failure would
                        // only make clients retry.
                        Some(stored) if stored.user_id == command.user_id => {
                            self.refresh_tokens.revoke(stored.id, now).await?;
                            1
                        }
                        Some(_) => {
                            warn!(user_id = %command.user_id, "logout attempted with another user's token");
                            0
                        }
                        None => 0,
                    }
                }
                None => {
                    self.refresh_tokens
                        .revoke_all_for_user(command.user_id, now)
                        .await?
                }
            }
        };

        info!(user_id = %command.user_id, revoked, "sessions ended");
        Ok(revoked)
    }
}

/// Maps a token failure onto the auth error the client sees.
fn map_token_error(error: TokenError) -> AuthError {
    match error {
        TokenError::Expired => AuthError::RefreshTokenExpired,
        TokenError::InvalidSignature | TokenError::Malformed(_) => AuthError::InvalidRefreshToken,
        TokenError::WrongTokenType { .. } => AuthError::InvalidRefreshToken,
        TokenError::GenerationFailed(message) => {
            AuthError::Repository(crate::domain::errors::RepositoryError::database(message))
        }
    }
}
