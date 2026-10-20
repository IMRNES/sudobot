//! Discord's permission overwrites, behind [`JailEnforcer`].
//!
//! The mask arithmetic lives in the domain ([`crate::domain::jail::permissions`]); this
//! module only carries the result to Discord. That split is what lets the bot and the
//! API seal a jail identically — two implementations of "which channels are denied"
//! would drift, and the visible symptom would be a jail with one readable channel
//! that only the other process wrote correctly.

use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::application::ports::jail_enforcer::{EnforcerError, JailEnforcer, SealReport};
use crate::domain::jail::permissions::ChannelPolicy;

const API_BASE: &str = "https://discord.com/api/v10";

/// Discord's edge rate-limits unidentified clients aggressively.
const USER_AGENT: &str = "DiscordBot (https://github.com/IMRNES/sudobot, 0.1.0)";

/// Sealing writes one request per channel, so the timeout is per request rather
/// than for the whole pass; a 45-channel guild is 45 short calls, not one long one.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Talks to Discord's REST API as the bot to seal and unseal a jail.
#[derive(Clone)]
pub struct ReqwestJailEnforcer {
    http: Client,
    bot_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChannelStub {
    id: String,
}

impl ReqwestJailEnforcer {
    pub fn new(bot_token: Option<String>) -> anyhow::Result<Self> {
        let http = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| anyhow::anyhow!("building the discord rest client: {error}"))?;

        Ok(Self { http, bot_token })
    }

    /// The token, or the configuration error that explains its absence.
    fn token(&self) -> Result<&str, EnforcerError> {
        self.bot_token
            .as_deref()
            .ok_or(EnforcerError::NotConfigured)
    }

    /// Every channel id in the guild.
    ///
    /// Only ids are needed: the policy depends on whether a channel *is* the cell,
    /// which is a comparison of ids.
    async fn channel_ids(&self, token: &str, guild_id: u64) -> Result<Vec<u64>, EnforcerError> {
        let response = self
            .http
            .get(format!("{API_BASE}/guilds/{guild_id}/channels"))
            .header("Authorization", format!("Bot {token}"))
            .send()
            .await
            .map_err(|error| EnforcerError::Transport(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(EnforcerError::Api(format!("{status}: {body}")));
        }

        let channels: Vec<ChannelStub> = response
            .json()
            .await
            .map_err(|error| EnforcerError::Api(format!("unreadable channel list: {error}")))?;

        Ok(channels
            .into_iter()
            .filter_map(|channel| channel.id.parse().ok())
            .collect())
    }

    /// Writes one overwrite, reporting "this channel refused" as a counted failure
    /// rather than aborting the pass.
    async fn put_overwrite(
        &self,
        token: &str,
        channel_id: u64,
        jail_role_id: u64,
        policy: ChannelPolicy,
    ) -> Result<(), EnforcerError> {
        let body = serde_json::json!({
            "type": 0,
            "allow": policy.allow.to_string(),
            "deny": policy.deny.to_string(),
        });

        let response = self
            .http
            .put(format!(
                "{API_BASE}/channels/{channel_id}/permissions/{jail_role_id}"
            ))
            .header("Authorization", format!("Bot {token}"))
            .json(&body)
            .send()
            .await
            .map_err(|error| EnforcerError::Transport(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(EnforcerError::Api(format!("{status}: {body}")));
        }

        Ok(())
    }
}

impl JailEnforcer for ReqwestJailEnforcer {
    fn is_configured(&self) -> bool {
        self.bot_token.is_some()
    }

    async fn seal(
        &self,
        guild_id: u64,
        jail_role_id: u64,
        prison_channel_id: Option<u64>,
    ) -> Result<SealReport, EnforcerError> {
        let token = self.token()?;
        let channels = self.channel_ids(token, guild_id).await?;

        let mut report = SealReport::default();

        for channel_id in channels {
            let policy = ChannelPolicy::for_channel(channel_id, prison_channel_id);

            match self
                .put_overwrite(token, channel_id, jail_role_id, policy)
                .await
            {
                Ok(()) => report.written += 1,
                Err(error) => {
                    // One unmanageable channel must not abandon the rest: a jail that
                    // covers 44 of 45 channels is far better than none, and the count
                    // tells the admin whether to look at permissions at all.
                    report.failed += 1;
                    tracing::warn!(
                        guild_id,
                        channel_id,
                        jail_role_id,
                        %error,
                        "could not write the jail overwrite for a channel"
                    );
                }
            }
        }

        Ok(report)
    }

    async fn unseal(&self, guild_id: u64, jail_role_id: u64) -> Result<usize, EnforcerError> {
        let token = self.token()?;
        let channels = self.channel_ids(token, guild_id).await?;

        let mut cleared = 0;

        for channel_id in channels {
            let response = self
                .http
                .delete(format!(
                    "{API_BASE}/channels/{channel_id}/permissions/{jail_role_id}"
                ))
                .header("Authorization", format!("Bot {token}"))
                .send()
                .await
                .map_err(|error| EnforcerError::Transport(error.to_string()))?;

            match response.status() {
                status if status.is_success() => cleared += 1,
                // 404 is the ordinary case: the role only ever had an overwrite where
                // a jail was actually applied, so most channels have nothing to
                // delete. Counting it as a failure would report every unseal as broken.
                StatusCode::NOT_FOUND => {}
                status => {
                    let body = response.text().await.unwrap_or_default();
                    tracing::warn!(
                        guild_id,
                        channel_id,
                        jail_role_id,
                        %status,
                        %body,
                        "could not remove the jail overwrite from a channel"
                    );
                }
            }
        }

        Ok(cleared)
    }

    async fn put_in_cell(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        jail_role_id: u64,
    ) -> Result<(), EnforcerError> {
        let token = self.token()?;

        let response = self
            .http
            .put(format!(
                "{API_BASE}/guilds/{guild_id}/members/{discord_user_id}/roles/{jail_role_id}"
            ))
            .header("Authorization", format!("Bot {token}"))
            .send()
            .await
            .map_err(|error| EnforcerError::Transport(error.to_string()))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(EnforcerError::Api(format!("{status}: {body}")));
        }

        Ok(())
    }

    async fn let_out(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        jail_role_id: u64,
    ) -> Result<(), EnforcerError> {
        let token = self.token()?;

        let response = self
            .http
            .delete(format!(
                "{API_BASE}/guilds/{guild_id}/members/{discord_user_id}/roles/{jail_role_id}"
            ))
            .header("Authorization", format!("Bot {token}"))
            .send()
            .await
            .map_err(|error| EnforcerError::Transport(error.to_string()))?;

        let status = response.status();
        match status {
            status if status.is_success() => Ok(()),
            // Already out. The caller asked for the member to be free and they are,
            // so this is success — reporting it as an error would make every
            // release-during-a-sweep look like a failure.
            StatusCode::NOT_FOUND => Ok(()),
            status => {
                let body = response.text().await.unwrap_or_default();
                Err(EnforcerError::Api(format!("{status}: {body}")))
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::jail::permissions::{JAIL_DENY, PRISON_ALLOW, PRISON_DENY};

    #[test]
    fn no_overwrite_this_enforcer_writes_allows_and_denies_the_same_bit() {
        // `PRISON_ALLOW` and `JAIL_DENY` overlap on purpose — they describe different
        // channels. The invariant that matters at the wire is per channel: whatever
        // pair this enforcer sends must be disjoint, because Discord resolves an
        // allow and a deny of the same bit by precedence rather than by intent, so an
        // overlapping pair makes the real policy whatever wins rather than what was
        // written.
        for prison in [None, Some(1_u64), Some(7)] {
            for channel_id in [1_u64, 7, 42] {
                let policy = ChannelPolicy::for_channel(channel_id, prison);
                assert_eq!(
                    policy.allow & policy.deny,
                    0,
                    "channel {channel_id} (prison {prison:?}) overlaps itself"
                );
            }
        }
    }

    #[test]
    fn the_cell_denies_only_thread_creation() {
        for channel_id in [1_u64, 2, 3] {
            let policy = ChannelPolicy::for_channel(channel_id, Some(channel_id));
            assert_eq!(policy.allow, PRISON_ALLOW);
            assert_eq!(policy.deny, PRISON_DENY);
        }
    }

    #[test]
    fn every_other_channel_is_denied_wholesale() {
        let policy = ChannelPolicy::for_channel(2, Some(1));
        assert_eq!(policy.allow, 0);
        assert_eq!(policy.deny, JAIL_DENY);
    }

    #[test]
    fn a_jail_with_no_cell_denies_everything() {
        // Degenerate but reachable: an admin clears the channel. Denying everything
        // is the right answer — better a silent cell than an open jail.
        let policy = ChannelPolicy::for_channel(5, None);
        assert_eq!(policy.allow, 0);
        assert_eq!(policy.deny, JAIL_DENY);
    }

    #[test]
    fn an_unconfigured_enforcer_reports_itself_as_such() {
        let enforcer = ReqwestJailEnforcer::new(None).unwrap();
        assert!(!enforcer.is_configured());
        assert!(matches!(
            enforcer.token(),
            Err(EnforcerError::NotConfigured)
        ));
    }

    #[test]
    fn a_configured_enforcer_hands_back_the_token() {
        let enforcer = ReqwestJailEnforcer::new(Some("abc".to_string())).unwrap();
        assert!(enforcer.is_configured());
        assert_eq!(enforcer.token().unwrap(), "abc");
    }
}
