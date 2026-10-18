use std::future::Future;
use thiserror::Error;

/// The subset of a Discord user the application cares about.
///
/// Deliberately not `serenity::User`: the API process does not link the gateway
/// library, and a domain that speaks Discord's full model would be a domain that
/// breaks when Discord adds a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordProfile {
    pub id: u64,
    pub username: String,
    pub global_name: Option<String>,
    /// Fully-formed avatar URL, or `None` when the user has the default avatar.
    pub avatar_url: Option<String>,
    pub email: Option<String>,
}

/// A guild the user belongs to, with what Discord says they may do there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordGuildSummary {
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    /// Discord's permission bitfield for this user in this guild.
    pub permissions: u64,
    pub owner: bool,
}

#[derive(Debug, Error)]
pub enum DiscordOAuthError {
    /// This installation has no Discord OAuth client secret.
    ///
    /// Distinct from [`InvalidCode`](Self::InvalidCode) because it is a
    /// configuration fact rather than a rejection: Discord only shows the secret
    /// once and never returns it over the API, so an installation can genuinely
    /// lack one.
    #[error("this installation has no discord oauth client secret configured")]
    NotConfigured,

    #[error("discord rejected the authorization code")]
    InvalidCode,

    #[error("discord returned an error: {0}")]
    Api(String),

    #[error("discord is unreachable: {0}")]
    Transport(String),

    #[error("discord returned a response we could not read: {0}")]
    MalformedResponse(String),

    #[error("rate limited by discord; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
}

/// Everything the login flow needs from Discord.
///
/// Four methods, because that is the whole conversation: trade the code, read the
/// profile, read the guilds, and on logout revoke what was issued.
pub trait DiscordOAuthClient: Send + Sync {
    /// Exchanges the `code` from the consent screen for an access token.
    fn exchange_code(
        &self,
        code: &str,
    ) -> impl Future<Output = Result<String, DiscordOAuthError>> + Send;

    fn fetch_profile(
        &self,
        access_token: &str,
    ) -> impl Future<Output = Result<DiscordProfile, DiscordOAuthError>> + Send;

    fn fetch_guilds(
        &self,
        access_token: &str,
    ) -> impl Future<Output = Result<Vec<DiscordGuildSummary>, DiscordOAuthError>> + Send;

    /// Tells Discord to drop an access token; best-effort on logout.
    fn revoke(
        &self,
        access_token: &str,
    ) -> impl Future<Output = Result<(), DiscordOAuthError>> + Send;
}
