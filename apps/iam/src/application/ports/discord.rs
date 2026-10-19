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

/// Whether a member holds one of the roles that grant dashboard access.
///
/// The dashboard is single-tenant: one guild, and the operator decides who may
/// open it by giving them a Discord role. That makes the role the only fact worth
/// fetching, which is why this type carries no nickname, no permissions bitfield
/// and no guild list — none of them change the answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuildMembership {
    /// In the guild, holding at least one of the whitelisted roles.
    Allowed { username: String },
    /// In the guild, but with none of the whitelisted roles.
    RoleMissing { username: String },
    /// Not in the guild at all — Discord answers 404 for the member lookup.
    NotAMember,
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
/// Three methods, because that is the whole conversation with a whitelist
/// dashboard: trade the code, read the profile, and ask whether that member holds
/// an allowed role. The guild list is gone on purpose — see [`GuildMembership`].
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

    /// Resolves the caller's role membership in the one configured guild.
    ///
    /// Uses the bot token, not the caller's OAuth token: the `guilds` scope
    /// returns no roles at all, so a whitelist check cannot be built on it. The
    /// bot is already in the guild and may read members, which is the capability
    /// this question actually needs.
    fn fetch_membership(
        &self,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<GuildMembership, DiscordOAuthError>> + Send;

    /// Tells Discord to drop an access token; best-effort on logout.
    fn revoke(
        &self,
        access_token: &str,
    ) -> impl Future<Output = Result<(), DiscordOAuthError>> + Send;
}
