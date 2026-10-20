//! Discord's guild role and channel lists, behind [`GuildDirectorySource`].
//!
//! Reads as the bot, because that is the only credential that can enumerate a
//! guild. The dashboard itself never talks to Discord: it asks this API, which
//! holds the token, so a resolved name never depends on the caller's own scopes.

use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::application::ports::guild_directory::{
    DirectoryChannel, DirectoryError, DirectoryRole, GuildDirectory, GuildDirectorySource,
};

const API_BASE: &str = "https://discord.com/api/v10";

/// Discord's edge rate-limits unidentified clients aggressively.
const USER_AGENT: &str = "DiscordBot (https://github.com/IMRNES/sudobot, 0.1.0)";

/// The role list has no page parameter and is returned whole, so it needs a
/// ceiling only to keep a pathological guild from allocating without bound.
const MAX_ROLES: usize = 250;

/// Talks to Discord's REST API as the bot to enumerate a guild.
#[derive(Clone)]
pub struct ReqwestGuildDirectory {
    http: Client,
    bot_token: Option<String>,
}

impl ReqwestGuildDirectory {
    pub fn new(bot_token: Option<String>) -> anyhow::Result<Self> {
        let http = Client::builder()
            // Two small reads; a hung one would hold a request handler open while
            // the admin stares at a spinner.
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| anyhow::anyhow!("building the discord rest client: {error}"))?;

        Ok(Self { http, bot_token })
    }

    /// One authenticated `GET`, with Discord's failures mapped to our own.
    async fn get<T: for<'de> Deserialize<'de>>(
        &self,
        token: &str,
        path: &str,
    ) -> Result<T, DirectoryError> {
        let response = self
            .http
            .get(format!("{API_BASE}{path}"))
            .header("Authorization", format!("Bot {token}"))
            .send()
            .await
            .map_err(|error| DirectoryError::Transport(error.to_string()))?;

        let status = response.status();

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(match status {
                StatusCode::TOO_MANY_REQUESTS => DirectoryError::Api(format!(
                    "rate limited by discord; retry after {}s",
                    retry_after_secs(&body)
                )),
                other => DirectoryError::Api(format!("{other}: {body}")),
            });
        }

        response
            .json::<T>()
            .await
            .map_err(|error| DirectoryError::MalformedResponse(error.to_string()))
    }
}

impl GuildDirectorySource for ReqwestGuildDirectory {
    fn is_configured(&self) -> bool {
        self.bot_token.is_some()
    }

    async fn fetch(&self, guild_id: u64) -> Result<GuildDirectory, DirectoryError> {
        // Refused here rather than requested with an empty bearer: Discord would
        // answer 401, which reads as "the token is wrong" when the real answer is
        // "this installation has no bot token".
        let Some(token) = &self.bot_token else {
            return Err(DirectoryError::NotConfigured);
        };

        // Sequential rather than concurrent: both reads are small, and two
        // simultaneous requests against one guild's routes are what earns a 429 on
        // a busy install. The pair is cached downstream, so this runs rarely.
        let roles: Vec<RolePayload> = self
            .get(token, &format!("/guilds/{guild_id}/roles"))
            .await?;

        let channels: Vec<ChannelPayload> = self
            .get(token, &format!("/guilds/{guild_id}/channels"))
            .await?;

        Ok(GuildDirectory {
            roles: roles
                .into_iter()
                .take(MAX_ROLES)
                .map(DirectoryRole::from)
                .collect(),
            channels: channels.into_iter().map(DirectoryChannel::from).collect(),
        })
    }
}

/// The subset of Discord's role object the dashboard renders.
#[derive(Debug, Deserialize)]
struct RolePayload {
    id: String,
    name: String,
    #[serde(default)]
    color: i32,
    #[serde(default)]
    position: i32,
    #[serde(default)]
    hoist: bool,
    #[serde(default)]
    mentionable: bool,
    /// Present and true for bot/integration roles, which cannot be assigned by hand.
    #[serde(default)]
    managed: bool,
}

impl From<RolePayload> for DirectoryRole {
    fn from(payload: RolePayload) -> Self {
        Self {
            id: payload.id.parse().unwrap_or_default(),
            name: payload.name,
            color: payload.color,
            position: payload.position,
            hoist: payload.hoist,
            mentionable: payload.mentionable,
            managed: payload.managed,
        }
    }
}

/// The subset of Discord's channel object the dashboard renders.
#[derive(Debug, Deserialize)]
struct ChannelPayload {
    id: String,
    /// Absent for categories, which is why this is not a plain `String`.
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "type", default)]
    kind: i32,
    #[serde(default)]
    parent_id: Option<String>,
}

impl From<ChannelPayload> for DirectoryChannel {
    fn from(payload: ChannelPayload) -> Self {
        Self {
            id: payload.id.parse().unwrap_or_default(),
            // A category with no name is possible in Discord's model; naming it
            // beats dropping the row and leaving a channel's parent unresolvable.
            name: payload.name.unwrap_or_else(|| "unnamed".to_string()),
            kind: payload.kind,
            parent_id: payload.parent_id.and_then(|raw| raw.parse().ok()),
        }
    }
}

/// Reads `retry_after` out of a 429 body, falling back to one second.
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_role_payload_becomes_a_directory_role() {
        let payload: RolePayload = serde_json::from_str(
            r#"{"id":"1460146703637287115","name":"Neophyte","color":5793266,
                "position":22,"hoist":true,"mentionable":false,"managed":false}"#,
        )
        .unwrap();

        let role = DirectoryRole::from(payload);
        assert_eq!(role.id, 1460146703637287115);
        assert_eq!(role.name, "Neophyte");
        assert_eq!(role.color, 5793266);
        assert!(role.hoist);
        assert!(!role.managed);
    }

    #[test]
    fn a_role_without_optional_fields_still_parses() {
        // Discord omits `hoist` and friends on some integration roles; a missing
        // field must not sink the whole snapshot.
        let payload: RolePayload =
            serde_json::from_str(r#"{"id":"1","name":"@everyone"}"#).unwrap();

        let role = DirectoryRole::from(payload);
        assert_eq!(role.name, "@everyone");
        assert!(!role.hoist);
        assert!(!role.managed);
    }

    #[test]
    fn a_category_channel_parses_without_a_parent() {
        let payload: ChannelPayload =
            serde_json::from_str(r#"{"id":"999","name":"General","type":4}"#).unwrap();

        let channel = DirectoryChannel::from(payload);
        assert_eq!(channel.kind, 4);
        assert_eq!(channel.parent_id, None);
        assert_eq!(channel.name, "General");
    }

    #[test]
    fn a_channel_with_an_unparseable_id_does_not_panic() {
        // `unwrap_or_default` rather than a panic: a malformed id should cost this
        // one row, not the request that renders the whole page.
        let payload: ChannelPayload =
            serde_json::from_str(r#"{"id":"not-a-snowflake","name":"x","type":0}"#).unwrap();

        assert_eq!(DirectoryChannel::from(payload).id, 0);
    }

    #[test]
    fn retry_after_is_rounded_up() {
        assert_eq!(retry_after_secs(r#"{"retry_after":0.25}"#), 1);
        assert_eq!(retry_after_secs(r#"{"retry_after":2.5}"#), 3);
        // A body that is not JSON at all — a proxy's HTML error page, say.
        assert_eq!(retry_after_secs("<html>"), 1);
    }

    #[test]
    fn an_unconfigured_installation_refuses_rather_than_requesting() {
        let client = ReqwestGuildDirectory::new(None).unwrap();
        assert!(!client.is_configured());

        let outcome = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(client.fetch(1));

        assert!(matches!(outcome, Err(DirectoryError::NotConfigured)));
    }
}
