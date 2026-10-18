use anyhow::Result;

/// Settings for the Discord gateway bot.
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    token: String,
    /// When set, slash commands are registered to this guild instead of globally.
    ///
    /// Global registration can take an hour to propagate; guild registration is
    /// instant, which is what makes local iteration bearable.
    pub dev_guild_id: Option<u64>,
    /// Extra users allowed to run owner-only commands, beyond Discord's own
    /// application-owner list.
    pub owner_ids: Vec<u64>,
}

impl DiscordConfig {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            token: crate::required("DISCORD_BOT_TOKEN")?,
            dev_guild_id: parse_optional_snowflake("DISCORD_DEV_GUILD_ID")?,
            owner_ids: crate::optional("DISCORD_OWNER_IDS", "")
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| {
                    id.parse::<u64>().map_err(|error| {
                        anyhow::anyhow!(
                            "`DISCORD_OWNER_IDS` entry `{id}` is not a snowflake: {error}"
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    pub fn is_development_guild(&self) -> bool {
        self.dev_guild_id.is_some()
    }
}

/// Parses an optional snowflake, treating an empty string as "unset".
///
/// An empty variable is what a `.env` file produces when someone comments out the
/// value but leaves the key, and that should mean "not configured", not "invalid".
fn parse_optional_snowflake(key: &str) -> Result<Option<u64>> {
    let raw = crate::optional(key, "");
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    trimmed
        .parse::<u64>()
        .map(Some)
        .map_err(|error| anyhow::anyhow!("`{key}` is not a snowflake: {error} (got `{trimmed}`)"))
}
