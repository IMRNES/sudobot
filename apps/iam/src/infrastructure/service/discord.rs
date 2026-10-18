//! Discord's OAuth2 and user endpoints, behind `DiscordOAuthClient`.

use reqwest::{Client, StatusCode};
use serde::Deserialize;
use sudobot_config::DiscordOAuthConfig;

use crate::application::ports::{
    DiscordGuildSummary, DiscordOAuthClient, DiscordOAuthError, DiscordProfile,
};

const API_BASE: &str = "https://discord.com/api";
const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const REVOKE_URL: &str = "https://discord.com/api/oauth2/token/revoke";
/// Discord's CDN renders avatars at any power-of-two size up to 4096.
const AVATAR_SIZE: u32 = 256;

/// Talks to Discord over HTTPS.
#[derive(Clone)]
pub struct ReqwestDiscordOAuthClient {
    http: Client,
    config: DiscordOAuthConfig,
}

impl ReqwestDiscordOAuthClient {
    /// Builds the client, failing early if TLS cannot be set up.
    ///
    /// Returning the failure here rather than at the first login keeps a broken
    /// build from looking like an OAuth outage.
    pub fn new(config: DiscordOAuthConfig) -> anyhow::Result<Self> {
        let http = Client::builder()
            // A hung request would hold a request handler open; the user can always
            // restart the flow, so a hard ceiling is better than an unbounded wait.
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|error| anyhow::anyhow!("building the discord http client: {error}"))?;

        Ok(Self { http, config })
    }

    /// The consent-screen URL the dashboard redirects the browser to.
    ///
    /// Not part of `DiscordOAuthClient`: the application never builds this URL —
    /// only the handler that starts a login does, and it needs the same client id
    /// and redirect URI that the exchange will use, which is why it lives here.
    pub fn authorize_url(&self, state: &str) -> String {
        self.config.authorize_url(state)
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
        let request = self.http.post(TOKEN_URL).form(&[
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret()),
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

    async fn fetch_guilds(
        &self,
        access_token: &str,
    ) -> Result<Vec<DiscordGuildSummary>, DiscordOAuthError> {
        let payload: Vec<GuildPayload> = self
            .get("/users/@me/guilds", access_token)
            .await?
            .json()
            .await
            .map_err(|error| DiscordOAuthError::MalformedResponse(error.to_string()))?;

        payload
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()
    }

    async fn revoke(&self, access_token: &str) -> Result<(), DiscordOAuthError> {
        let request = self.http.post(REVOKE_URL).form(&[
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret()),
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
struct GuildPayload {
    id: String,
    name: String,
    icon: Option<String>,
    /// Discord sends permissions as a string, for the same reason it sends ids as
    /// one.
    permissions: Option<String>,
    owner: Option<bool>,
}

impl TryFrom<GuildPayload> for DiscordGuildSummary {
    type Error = DiscordOAuthError;

    fn try_from(payload: GuildPayload) -> Result<Self, Self::Error> {
        let id = payload
            .id
            .parse()
            .map_err(|error| DiscordOAuthError::MalformedResponse(format!("guild id: {error}")))?;

        let permissions = payload
            .permissions
            .as_deref()
            .unwrap_or("0")
            .parse()
            .map_err(|error| {
                DiscordOAuthError::MalformedResponse(format!("guild permissions: {error}"))
            })?;

        Ok(Self {
            id,
            name: payload.name,
            icon_url: payload
                .icon
                .as_deref()
                .map(|hash| guild_icon_url(&payload.id, hash)),
            permissions,
            owner: payload.owner.unwrap_or(false),
        })
    }
}

/// The hash is `None` when the user has no avatar, and Discord's default avatar is
/// not ours to build — so the field stays empty rather than pointing at a 404.
fn avatar_url(discord_id: &str, hash: &str) -> String {
    format!("https://cdn.discordapp.com/avatars/{discord_id}/{hash}.png?size={AVATAR_SIZE}")
}

fn guild_icon_url(discord_id: &str, hash: &str) -> String {
    format!("https://cdn.discordapp.com/icons/{discord_id}/{hash}.png?size={AVATAR_SIZE}")
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
    fn a_guild_permission_string_becomes_a_bitfield() {
        let summary = DiscordGuildSummary::try_from(GuildPayload {
            id: "2".to_string(),
            name: "A guild".to_string(),
            icon: None,
            permissions: Some("32".to_string()),
            owner: None,
        })
        .unwrap();

        assert_eq!(summary.permissions, 32);
        assert!(!summary.owner);
    }

    #[test]
    fn a_rate_limit_body_yields_its_retry_after() {
        assert_eq!(retry_after_secs(r#"{"retry_after": 4.2}"#), 5);
        assert_eq!(retry_after_secs("not json"), 1);
    }
}
