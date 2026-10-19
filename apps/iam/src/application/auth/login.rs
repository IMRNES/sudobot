use tracing::{info, warn};
use uuid::Uuid;

use crate::application::ports::{
    Clock, DiscordOAuthClient, DiscordOAuthError, GuildMembership, TokenPair, TokenService,
};
use crate::domain::auth::{AuthError, NewRefreshToken, RefreshTokenRepository, hash_refresh_token};
use crate::domain::user::{NewUser, User, UserRepository, validate_username};

/// Everything the callback receives from Discord.
#[derive(Debug, Clone)]
pub struct DiscordLoginCommand {
    pub code: String,
}

/// What a successful login hands back.
///
/// The membership verdict travels with the result so the caller does not repeat
/// the Discord lookup when it records the membership row. The Discord access token
/// is deliberately absent: it is revoked before this returns, because nothing
/// downstream needs it any more.
#[derive(Debug, Clone)]
pub struct LoginResult {
    pub user: User,
    pub tokens: TokenPair,
    /// Always [`GuildMembership::Allowed`] — a login that fails the check is an
    /// error, not a result.
    pub membership: GuildMembership,
}

/// Trades an OAuth code for a session, gating on the guild whitelist.
///
/// Four writes happen here and they must agree: the account is created on first
/// login, refreshed on every later one, and a refresh token is stored so the
/// session can outlive the short-lived access token.
///
/// The whitelist check runs *before* the token pair is issued. Issuing first and
/// checking afterwards would hand a working session to anyone with a Discord
/// account and let a later request decide their access — the gate belongs at the
/// moment identity is established.
pub struct DiscordLoginUseCase<O, U, R, T, C> {
    oauth: O,
    users: U,
    refresh_tokens: R,
    tokens: T,
    clock: C,
}

impl<O, U, R, T, C> DiscordLoginUseCase<O, U, R, T, C>
where
    O: DiscordOAuthClient,
    U: UserRepository,
    R: RefreshTokenRepository,
    T: TokenService,
    C: Clock,
{
    pub fn new(oauth: O, users: U, refresh_tokens: R, tokens: T, clock: C) -> Self {
        Self {
            oauth,
            users,
            refresh_tokens,
            tokens,
            clock,
        }
    }

    /// Completes the login: code → Discord profile → whitelist → account → tokens.
    pub async fn execute(&self, command: DiscordLoginCommand) -> Result<LoginResult, AuthError> {
        let now = self.clock.now();

        // 1. Exchange the code. Discord rejects a reused or expired one, which is
        //    the normal browser-refresh failure and not our fault to log loudly.
        let access_token = self
            .oauth
            .exchange_code(&command.code)
            .await
            .map_err(map_oauth_error)?;

        let profile = self
            .oauth
            .fetch_profile(&access_token)
            .await
            .map_err(map_oauth_error)?;

        // 2. Gate on the guild whitelist, while the identity is fresh and *before*
        //    any local state is written or any session token is minted.
        let membership = self
            .oauth
            .fetch_membership(profile.id)
            .await
            .map_err(map_oauth_error)?;

        // The token has now done its job — the dashboard never needs the caller's
        // Discord token, because roles are read with the bot's. Revoking here
        // rather than in the caller means no code path can forget, and the caller's
        // error return below cannot leave it alive either.
        if let Err(error) = self.oauth.revoke(&access_token).await {
            warn!(%error, "could not revoke the discord access token after login");
        }

        if let GuildMembership::RoleMissing { .. } | GuildMembership::NotAMember = membership {
            // Logged with the distinction intact: "not in the guild" and "in the
            // guild without the role" need different fixes, and Discord's response
            // alone does not make the difference visible to an operator.
            warn!(
                discord_id = profile.id,
                verdict = ?membership,
                "login refused: the account holds no whitelisted dashboard role"
            );
            return Err(AuthError::NotWhitelisted);
        }

        // 3. Find or create the account. Discord's id is the identity; everything
        //    else on the profile can change under us.
        let user = self.upsert_user(&profile).await?;

        // 4. Mint the pair and persist the refresh half as a hash.
        let tokens = self
            .tokens
            .issue_pair(user.id, now)
            .await
            .map_err(|error| {
                AuthError::Repository(crate::domain::errors::RepositoryError::database(format!(
                    "token issuance failed: {error}"
                )))
            })?;

        self.store_refresh_token(user.id, &tokens).await?;

        info!(user_id = %user.id, discord_id = user.discord_id, "user signed in");

        Ok(LoginResult {
            user,
            tokens,
            membership,
        })
    }

    /// Tells Discord to drop the access token a login used.
    ///
    /// Best-effort and deliberately separate from `execute`: a failure here means
    /// the token stays valid at Discord until it expires, which is worth a log line
    /// and nothing more. Reported rather than swallowed so the caller can decide.
    pub async fn revoke_discord_token(&self, access_token: &str) -> Result<(), AuthError> {
        self.oauth
            .revoke(access_token)
            .await
            .map_err(map_oauth_error)
            .map_err(|error| {
                warn!(%error, "could not revoke the discord access token");
                error
            })
    }

    /// Creates the account on first login, refreshes it on every later one.
    async fn upsert_user(
        &self,
        profile: &crate::application::ports::DiscordProfile,
    ) -> Result<User, AuthError> {
        let username = validate_username(&profile.username).map_err(|error| {
            AuthError::Repository(crate::domain::errors::RepositoryError::invalid_data(
                "username",
                error.to_string(),
            ))
        })?;

        match self.users.find_by_discord_id(profile.id).await? {
            Some(existing) => {
                let patch = crate::domain::user::UserPatch {
                    username: Some(username),
                    global_name: profile.global_name.clone(),
                    avatar_url: profile.avatar_url.clone(),
                    email: profile.email.clone(),
                };

                if patch.is_empty() {
                    return Ok(existing);
                }

                // A profile change is not an error worth waking anyone for, but it
                // is worth a line: it explains why a name changed in the UI.
                info!(user_id = %existing.id, "refreshing profile from discord login");
                self.users
                    .update(existing.id, patch)
                    .await
                    .map_err(AuthError::from)
            }
            None => {
                let new_user = NewUser {
                    id: Uuid::new_v4(),
                    discord_id: profile.id,
                    username,
                    global_name: profile.global_name.clone(),
                    avatar_url: profile.avatar_url.clone(),
                    email: profile.email.clone(),
                };

                match self.users.create(new_user).await {
                    Ok(created) => {
                        info!(user_id = %created.id, "account created from discord login");
                        Ok(created)
                    }
                    // Two logins racing on first sign-in both see "not found" and
                    // both insert. The unique index rejects the loser, and the
                    // right answer is the winner's row, not an error in the
                    // browser of whoever clicked second.
                    Err(crate::domain::errors::RepositoryError::Conflict(_)) => {
                        warn!(discord_id = profile.id, "duplicate first login; re-reading");
                        self.users
                            .find_by_discord_id(profile.id)
                            .await?
                            .ok_or(AuthError::UserNotFound)
                    }
                    Err(error) => Err(AuthError::from(error)),
                }
            }
        }
    }

    async fn store_refresh_token(
        &self,
        user_id: Uuid,
        tokens: &TokenPair,
    ) -> Result<(), AuthError> {
        self.refresh_tokens
            .create(NewRefreshToken {
                id: tokens.refresh_token_id,
                user_id,
                token_hash: hash_refresh_token(&tokens.refresh_token),
                expires_at: tokens.refresh_expires_at,
            })
            .await?;

        Ok(())
    }
}

/// Translates a transport-level Discord failure into a domain one.
///
/// `InvalidCode` gets its own variant because the login screen handles it
/// differently from "Discord is down": one restarts the flow, the other retries.
fn map_oauth_error(error: DiscordOAuthError) -> AuthError {
    match error {
        DiscordOAuthError::NotConfigured => AuthError::LoginNotConfigured,
        DiscordOAuthError::InvalidCode => AuthError::InvalidOAuthCode,
        DiscordOAuthError::Api(message) => AuthError::DiscordUnavailable(message),
        DiscordOAuthError::Transport(message) => AuthError::DiscordUnavailable(message),
        DiscordOAuthError::MalformedResponse(message) => {
            AuthError::DiscordUnavailable(format!("unreadable response: {message}"))
        }
        DiscordOAuthError::RateLimited { retry_after_secs } => {
            AuthError::DiscordUnavailable(format!("rate limited for {retry_after_secs}s"))
        }
    }
}
