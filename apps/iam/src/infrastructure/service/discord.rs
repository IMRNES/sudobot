//! Discord's OAuth2 endpoints and the guild membership check, behind
//! [`DiscordOAuthClient`].
//!
//! Two different credentials live here, and the split is the point:
//! the caller's OAuth token answers "who is this", while the bot token answers
//! "do they hold the staff role". The OAuth `guilds` scope cannot answer the
//! second question — it returns a guild list with no roles in it — so the
//! whitelist check is built on the bot's own credential instead.

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use sudobot_config::{DashboardAccessConfig, DiscordOAuthConfig};

use crate::application::ports::{
    DiscordOAuthClient, DiscordOAuthError, DiscordProfile, GuildMembership,
};

const API_BASE: &str = "https://discord.com/api";
const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const REVOKE_URL: &str = "https://discord.com/api/oauth2/token/revoke";
/// Discord's CDN renders avatars at any power-of-two size up to 4096.
const AVATAR_SIZE: u32 = 256;

/// Every request carries this. Discord's edge rate-limits unidentified clients
/// aggressively, and a 429 body is not the JSON a caller expects — so an absent
/// user agent surfaces as "could not read the response" instead of "slow down".
///
/// A literal rather than `CARGO_PKG_REPOSITORY`: the crate manifest sets no
/// `repository` field, so the macro would expand to an empty string and produce
/// the malformed header `DiscordBot (, 0.1.0)`.
const USER_AGENT: &str = "DiscordBot (https://github.com/IMRNES/sudobot, 0.1.0)";

/// Talks to Discord over HTTPS.
///
/// Carries two credentials because it answers two different questions: the OAuth
/// client handles "who signed in", the bot token handles "may they open the
/// dashboard". Keeping them in one type is what lets a handler do both without
/// reaching for a second service.
#[derive(Clone)]
pub struct ReqwestDiscordOAuthClient {
    http: Client,
    config: DiscordOAuthConfig,
    /// Credentials for the membership check, absent when the dashboard has no
    /// whitelist configured.
    access: Option<DashboardAccessConfig>,
    bot_token: Option<String>,
}

impl ReqwestDiscordOAuthClient {
    /// Builds the client, failing early if TLS cannot be set up.
    ///
    /// Returning the failure here rather than at the first login keeps a broken
    /// build from looking like an OAuth outage.
    pub fn new(config: DiscordOAuthConfig) -> anyhow::Result<Self> {
        Self::with_access(config, None, None)
    }

    /// Adds the guild whitelist, so `fetch_membership` can run.
    ///
    /// Separate from [`new`](Self::new) because the two are genuinely independent:
    /// token exchange needs the OAuth client, and the whitelist needs the bot. An
    /// installation that configures only the first still serves logins, and one
    /// that configures only the second still refuses them — with a message that
    /// names the missing half rather than a generic denial.
    pub fn with_access(
        config: DiscordOAuthConfig,
        access: Option<DashboardAccessConfig>,
        bot_token: Option<String>,
    ) -> anyhow::Result<Self> {
        let http = Client::builder()
            // A hung request would hold a request handler open; the user can always
            // restart the flow, so a hard ceiling is better than an unbounded wait.
            .timeout(std::time::Duration::from_secs(10))
            // Without this, a Discord 429 with a JSON body is surfaced as a decode
            // failure while retrying, which reads as a malformed response instead of
            // "slow down".
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| anyhow::anyhow!("building the discord http client: {error}"))?;

        Ok(Self {
            http,
            config,
            access,
            bot_token,
        })
    }

    /// The consent-screen URL the dashboard redirects the browser to.
    ///
    /// Not part of `DiscordOAuthClient`: the application never builds this URL —
    /// only the handler that starts a login does, and it needs the same client id
    /// and redirect URI that the exchange will use, which is why it lives here.
    pub fn authorize_url(&self, state: &str) -> String {
        self.config.authorize_url(state)
    }

    /// Whether this installation can complete a consent-code exchange.
    ///
    /// The consent screen needs only the client id, so a half-configured
    /// installation can build a URL that is guaranteed to fail. Callers check this
    /// first so the browser is refused here instead of by Discord.
    pub fn is_configured(&self) -> bool {
        self.config.is_configured()
    }

    /// Whether the guild whitelist is configured, so a login can be gated.
    ///
    /// Needs both halves — the guild to check and a token able to read its
    /// members — because either one alone cannot answer the question.
    pub fn has_access_config(&self) -> bool {
        self.access.is_some() && self.bot_token.is_some()
    }

    /// The token endpoint, exposed for diagnostics and tests.
    pub fn token_endpoint(&self) -> &'static str {
        self.config.token_endpoint()
    }

    /// The shared failure mapping for the two token-endpoint calls.
    async fn token_response(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<TokenResponse, DiscordOAuthError> {
        let response = request.send().await.map_err(transport)?;
        let status = response.status();

        if status.is_success() {
            // A 200 whose body is not the shape we expect is Discord changing the
            // response, not a user error — hence `MalformedResponse`.
            return response
                .json::<TokenResponse>()
                .await
                .map_err(|error| DiscordOAuthError::MalformedResponse(error.to_string()));
        }

        let body = response.text().await.unwrap_or_default();

        match status {
            // A code that is expired, reused, or issued to another client. This is
            // the browser-refresh case, so it gets its own variant the login screen
            // can act on.
            StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED => {
                Err(DiscordOAuthError::InvalidCode)
            }
            StatusCode::TOO_MANY_REQUESTS => Err(DiscordOAuthError::RateLimited {
                retry_after_secs: retry_after_secs(&body),
            }),
            other => Err(DiscordOAuthError::Api(format!("{other}: {body}"))),
        }
    }

    /// A `GET` with the caller's bearer token.
    async fn get(
        &self,
        path: &str,
        access_token: &str,
    ) -> Result<reqwest::Response, DiscordOAuthError> {
        let response = self
            .http
            .get(format!("{API_BASE}{path}"))
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(transport)?;

        let status = response.status();

        match status {
            status if status.is_success() => Ok(response),
            StatusCode::UNAUTHORIZED => Err(DiscordOAuthError::Api(
                "the access token was rejected".to_string(),
            )),
            StatusCode::TOO_MANY_REQUESTS => {
                let body = response.text().await.unwrap_or_default();
                Err(DiscordOAuthError::RateLimited {
                    retry_after_secs: retry_after_secs(&body),
                })
            }
            other => {
                let body = response.text().await.unwrap_or_default();
                Err(DiscordOAuthError::Api(format!("{other}: {body}")))
            }
        }
    }
}

impl DiscordOAuthClient for ReqwestDiscordOAuthClient {
    async fn exchange_code(&self, code: &str) -> Result<String, DiscordOAuthError> {
        // Refused here rather than posted with an empty secret: Discord would
        // answer `invalid_client`, which reads as "your credentials are wrong"
        // when the real answer is "this installation has none configured".
        let Some(client_secret) = self.config.client_secret() else {
            return Err(DiscordOAuthError::NotConfigured);
        };

        let request = self.http.post(TOKEN_URL).form(&[
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", client_secret),
            ("grant_type", "authorization_code"),
            ("code", code),
            // Must match the value the consent screen was opened with, byte for
            // byte, or Discord rejects the exchange.
            ("redirect_uri", self.config.redirect_uri.as_str()),
        ]);

        Ok(self.token_response(request).await?.access_token)
    }

    async fn fetch_profile(&self, access_token: &str) -> Result<DiscordProfile, DiscordOAuthError> {
        let payload: UserPayload = self
            .get("/users/@me", access_token)
            .await?
            .json()
            .await
            .map_err(|error| DiscordOAuthError::MalformedResponse(error.to_string()))?;

        Ok(payload.into())
    }

    async fn fetch_membership(
        &self,
        discord_user_id: u64,
    ) -> Result<GuildMembership, DiscordOAuthError> {
        // Refused with a distinct error rather than treated as "no roles": an
        // installation that never configured the whitelist would otherwise refuse
        // every user with "you are not staff", which is indistinguishable from a
        // genuinely missing role and so never gets investigated.
        let (Some(access), Some(bot_token)) = (&self.access, &self.bot_token) else {
            return Err(DiscordOAuthError::NotConfigured);
        };

        let response = self
            .http
            .get(format!(
                "{API_BASE}/guilds/{}/members/{discord_user_id}",
                access.guild_id
            ))
            .header("Authorization", format!("Bot {bot_token}"))
            .send()
            .await
            .map_err(transport)?;

        let status = response.status();

        // 404 is the answer, not a failure: Discord uses it to say "no such member
        // in this guild", which is exactly the deny case. Reading it as an error
        // would turn every non-member's sign-in into a 500.
        if status == StatusCode::NOT_FOUND {
            return Ok(GuildMembership::NotAMember);
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DiscordOAuthError::Api(format!("{status}: {body}")));
        }

        // The body is decoded into a wire type local to this module, then reduced
        // to a domain answer. `GuildMembership` deliberately does not carry a
        // nickname or a permissions bitfield: neither changes the decision, and a
        // type that mirrors Discord's member object would break on the next field
        // Discord adds.
        let payload: MemberPayload = response
            .json()
            .await
            .map_err(|error| DiscordOAuthError::MalformedResponse(error.to_string()))?;

        Ok(payload.into_membership(access, discord_user_id))
    }

    async fn revoke(&self, access_token: &str) -> Result<(), DiscordOAuthError> {
        // Best-effort by contract, and revocation cannot run without the secret.
        // A token that stays valid until it expires is the caller's accepted
        // outcome here, so this is not an error worth surfacing.
        let Some(client_secret) = self.config.client_secret() else {
            return Ok(());
        };

        let request = self.http.post(REVOKE_URL).form(&[
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", client_secret),
            ("token", access_token),
        ]);

        // Fire and forget at the token endpoint's level of strictness: logout must
        // succeed even when Discord is unreachable, and the application logs the
        // failure without failing the request.
        let response = request.send().await.map_err(transport)?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(DiscordOAuthError::Api(format!(
                "revocation returned {}",
                response.status()
            )))
        }
    }
}

/// `expires_in` and `refresh_token` are part of Discord's response but not of
/// anything this service does with it: the token is used once and revoked.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct UserPayload {
    /// A snowflake, which arrives as a string because it exceeds what a JSON number
    /// can carry exactly.
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
    email: Option<String>,
}

impl From<UserPayload> for DiscordProfile {
    fn from(payload: UserPayload) -> Self {
        let avatar_url = payload
            .avatar
            .as_deref()
            .map(|hash| avatar_url(&payload.id, hash));

        Self {
            // Parsed here rather than in the use case: a malformed id from Discord is
            // a protocol problem, and `0` keeps the profile usable for the parts that
            // do not need it. The login's `validate_username` runs on the username,
            // which is the field that actually has to be sound.
            id: payload.id.parse().unwrap_or_default(),
            username: payload.username,
            global_name: payload.global_name,
            avatar_url,
            email: payload.email,
        }
    }
}

#[derive(Debug, Deserialize)]
struct MemberPayload {
    /// Discord sends snowflakes as strings, because they exceed what a JSON number
    /// carries exactly.
    roles: Vec<String>,
    user: MemberUserPayload,
}

#[derive(Debug, Deserialize)]
struct MemberUserPayload {
    username: String,
}

impl MemberPayload {
    /// Reduces Discord's member object to the one answer the dashboard needs.
    ///
    /// The allowlist is checked inside [`DashboardAccessConfig::permits`] and takes
    /// precedence, so an operator account gets in even when its role is missing —
    /// which is the situation this configuration exists for.
    fn into_membership(
        self,
        access: &DashboardAccessConfig,
        discord_user_id: u64,
    ) -> GuildMembership {
        let roles: Vec<u64> = self
            .roles
            .iter()
            .filter_map(|role| role.parse::<u64>().ok())
            .collect();

        let username = self.user.username;

        if access.permits(discord_user_id, Some(&roles)) {
            GuildMembership::Allowed { username }
        } else {
            GuildMembership::RoleMissing { username }
        }
    }
}

/// The hash is `None` when the user has no avatar, and Discord's default avatar is
/// not ours to build — so the field stays empty rather than pointing at a 404.
fn avatar_url(discord_id: &str, hash: &str) -> String {
    format!("https://cdn.discordapp.com/avatars/{discord_id}/{hash}.png?size={AVATAR_SIZE}")
}

/// Reads `retry_after` out of a 429 body, defaulting to Discord's own minimum.
///
/// The field is documented as seconds and is sometimes fractional; anything
/// unparseable falls back to one second so a caller always gets a usable answer.
fn retry_after_secs(body: &str) -> u64 {
    #[derive(Deserialize)]
    struct RateLimit {
        retry_after: Option<f64>,
    }

    serde_json::from_str::<RateLimit>(body)
        .ok()
        .and_then(|limit| limit.retry_after)
        .filter(|seconds| *seconds >= 0.0)
        .map(|seconds| seconds.ceil() as u64)
        .unwrap_or(1)
}

fn transport(error: reqwest::Error) -> DiscordOAuthError {
    DiscordOAuthError::Transport(error.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn an_avatar_hash_becomes_a_cdn_url() {
        assert_eq!(
            avatar_url("80351110224678912", "8342729096ea3675442027381ff50dfe"),
            "https://cdn.discordapp.com/avatars/80351110224678912/8342729096ea3675442027381ff50dfe.png?size=256"
        );
    }

    #[test]
    fn a_profile_without_an_avatar_has_no_url() {
        let profile = DiscordProfile::from(UserPayload {
            id: "1".to_string(),
            username: "someone".to_string(),
            global_name: Some("Someone".to_string()),
            avatar: None,
            email: None,
        });

        assert_eq!(profile.avatar_url, None);
        assert_eq!(profile.id, 1);
    }

    #[test]
    fn a_member_holding_a_whitelisted_role_is_allowed() {
        let access = DashboardAccessConfig {
            guild_id: 1,
            role_ids: vec![42],
            allowlist_user_ids: vec![],
        };

        let membership = MemberPayload {
            roles: vec!["7".to_string(), "42".to_string()],
            user: MemberUserPayload {
                username: "someone".to_string(),
            },
        }
        .into_membership(&access, 500);

        assert_eq!(
            membership,
            GuildMembership::Allowed {
                username: "someone".to_string()
            }
        );
    }

    /// Sorted roles are not guaranteed to be numeric strings of equal length, and a
    /// snowflake round-trips through a string. A member whose only role is not
    /// whitelisted must be reported as such — not silently allowed.
    #[test]
    fn a_member_without_a_whitelisted_role_is_distinguished() {
        let access = DashboardAccessConfig {
            guild_id: 1,
            role_ids: vec![42],
            allowlist_user_ids: vec![],
        };

        let membership = MemberPayload {
            roles: vec!["1451210604114346059".to_string()],
            user: MemberUserPayload {
                username: "someone".to_string(),
            },
        }
        .into_membership(&access, 500);

        assert!(matches!(membership, GuildMembership::RoleMissing { .. }));
    }

    /// Discord reports guild roles as strings, and one unparseable entry must not
    /// discard the ones around it.
    #[test]
    fn an_unparseable_role_does_not_hide_a_whitelisted_one() {
        let access = DashboardAccessConfig {
            guild_id: 1,
            role_ids: vec![42],
            allowlist_user_ids: vec![],
        };

        let membership = MemberPayload {
            roles: vec!["not-a-snowflake".to_string(), "42".to_string()],
            user: MemberUserPayload {
                username: "someone".to_string(),
            },
        }
        .into_membership(&access, 500);

        assert!(matches!(membership, GuildMembership::Allowed { .. }));
    }

    #[test]
    fn an_allowlisted_user_is_allowed_without_any_role() {
        let access = DashboardAccessConfig {
            guild_id: 1,
            role_ids: vec![42],
            allowlist_user_ids: vec![500],
        };

        let membership = MemberPayload {
            roles: vec![],
            user: MemberUserPayload {
                username: "operator".to_string(),
            },
        }
        .into_membership(&access, 500);

        assert!(matches!(membership, GuildMembership::Allowed { .. }));
    }

    #[test]
    fn a_rate_limit_body_yields_its_retry_after() {
        assert_eq!(retry_after_secs(r#"{"retry_after": 4.2}"#), 5);
        assert_eq!(retry_after_secs("not json"), 1);
    }
}
