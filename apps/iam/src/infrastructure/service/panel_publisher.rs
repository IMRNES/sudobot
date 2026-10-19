//! Posts role panels to Discord, behind [`RolePanelPublisher`].
//!
//! Uses the bot token, which is the only credential that can write as the bot. The
//! payload is built by [`render_panel`], so the same render runs in tests without a
//! network.

use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::application::ports::panel_publisher::{
    PanelPublishError, RolePanelPublisher, render_panel,
};
use crate::domain::welcome::RolePanelWithOptions;

const API_BASE: &str = "https://discord.com/api/v10";

/// Discord's edge rate-limits unidentified clients aggressively.
const USER_AGENT: &str = "DiscordBot (https://github.com/IMRNES/sudobot, 0.1.0)";

/// Talks to Discord's REST API as the bot.
#[derive(Clone)]
pub struct ReqwestRolePanelPublisher {
    http: Client,
    bot_token: Option<String>,
}

impl ReqwestRolePanelPublisher {
    /// Builds the client, failing early if TLS cannot be set up.
    pub fn new(bot_token: Option<String>) -> anyhow::Result<Self> {
        let http = Client::builder()
            // Posting a panel is a single small request; a hung one would hold the
            // handler open while the admin stares at a spinner.
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| anyhow::anyhow!("building the discord rest client: {error}"))?;

        Ok(Self { http, bot_token })
    }

    /// The shared failure mapping for the two write calls.
    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<MessagePayload, PanelPublishError> {
        let response = request.send().await.map_err(transport)?;
        let status = response.status();

        if status.is_success() {
            return response
                .json::<MessagePayload>()
                .await
                .map_err(|error| PanelPublishError::MalformedResponse(error.to_string()));
        }

        let body = response.text().await.unwrap_or_default();

        match status {
            StatusCode::TOO_MANY_REQUESTS => Err(PanelPublishError::RateLimited {
                retry_after_secs: retry_after_secs(&body),
            }),
            other => Err(PanelPublishError::Api(format!("{other}: {body}"))),
        }
    }
}

impl RolePanelPublisher for ReqwestRolePanelPublisher {
    fn is_configured(&self) -> bool {
        self.bot_token.is_some()
    }

    async fn post(
        &self,
        channel_id: u64,
        panel: &RolePanelWithOptions,
    ) -> Result<u64, PanelPublishError> {
        // Refused here rather than posted with an empty bearer: Discord would answer
        // 401, which reads as "the token is wrong" when the real answer is "this
        // installation has no bot token".
        let Some(token) = &self.bot_token else {
            return Err(PanelPublishError::NotConfigured);
        };

        let payload = render_panel(panel)?;

        // `wait=true` is the difference between a message id and nothing: without it
        // Discord answers 204 with no body, and the panel could never be recorded.
        let response = self
            .send(
                self.http
                    .post(format!(
                        "{API_BASE}/channels/{channel_id}/messages?wait=true"
                    ))
                    .header("Authorization", format!("Bot {token}"))
                    .json(&payload),
            )
            .await?;

        // Parsed here rather than defaulted: a message id we cannot read means the
        // panel cannot be addressed later, which is worth an error rather than a
        // panel silently recorded against message 0.
        response.id.parse().map_err(|_| {
            PanelPublishError::MalformedResponse(format!("message id {}", response.id))
        })
    }

    async fn edit(
        &self,
        channel_id: u64,
        message_id: u64,
        panel: &RolePanelWithOptions,
    ) -> Result<(), PanelPublishError> {
        let Some(token) = &self.bot_token else {
            return Err(PanelPublishError::NotConfigured);
        };

        let payload = render_panel(panel)?;

        self.send(
            self.http
                .patch(format!(
                    "{API_BASE}/channels/{channel_id}/messages/{message_id}"
                ))
                .header("Authorization", format!("Bot {token}"))
                .json(&payload),
        )
        .await?;

        Ok(())
    }
}

/// The fields of Discord's message object this module reads.
///
/// Only the id: the rest of the object is Discord's to change, and a type that
/// mirrored it would break on the next field they add.
#[derive(Debug, Deserialize)]
struct MessagePayload {
    /// A snowflake, which arrives as a string because it exceeds what a JSON number
    /// carries exactly.
    id: String,
}

/// Reads `retry_after` out of a 429 body, defaulting to Discord's own minimum.
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

fn transport(error: reqwest::Error) -> PanelPublishError {
    PanelPublishError::Transport(error.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_limit_body_yields_its_retry_after() {
        assert_eq!(retry_after_secs(r#"{"retry_after": 2.1}"#), 3);
        assert_eq!(retry_after_secs("not json"), 1);
    }

    #[test]
    fn a_client_without_a_token_is_not_configured() {
        let publisher = ReqwestRolePanelPublisher::new(None).unwrap();
        assert!(!publisher.is_configured());
    }
}
