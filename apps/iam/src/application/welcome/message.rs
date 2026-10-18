use tracing::{debug, info};

use crate::domain::welcome::{TemplateContext, WelcomeConfig, WelcomeError, WelcomeRepository};

/// Which of the two member-lifecycle messages is being built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WelcomeKind {
    Welcome,
    Goodbye,
}

/// Everything the renderer needs about the join or leave it is announcing.
#[derive(Debug, Clone)]
pub struct BuildWelcomeMessageQuery {
    pub guild_id: u64,
    pub guild_name: String,
    pub member_count: i64,
    pub discord_user_id: u64,
    pub username: String,
    pub kind: WelcomeKind,
}

/// A message ready to be posted.
#[derive(Debug, Clone)]
pub struct WelcomeMessage {
    pub channel_id: u64,
    pub content: String,
    pub embed_color: Option<i32>,
    /// Whether the same text should also go to the member's direct messages.
    pub send_dm: bool,
}

/// The message the bot posts when somebody joins (or leaves).
///
/// This is the join path: it runs on every member event in every guild, so it does
/// the smallest possible amount of work and it never fails a join — a
/// misconfiguration is reported as "nothing to send", not as an error the gateway
/// has to swallow.
pub struct BuildWelcomeMessageUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> BuildWelcomeMessageUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// `None` means "nothing to send": not enabled, or enabled with no channel.
    pub async fn execute(
        &self,
        query: BuildWelcomeMessageQuery,
    ) -> Result<Option<WelcomeMessage>, WelcomeError> {
        let config = self.welcome.find_config(query.guild_id).await?;

        let (enabled, channel_id, template, embed_color, send_dm) = match query.kind {
            WelcomeKind::Welcome => (
                config.welcome_enabled,
                config.welcome_channel_id,
                config.welcome_message.as_deref(),
                config.welcome_embed_color,
                config.welcome_dm_enabled,
            ),
            WelcomeKind::Goodbye => (
                config.goodbye_enabled,
                config.goodbye_channel_id,
                config.goodbye_message.as_deref(),
                config.goodbye_embed_color,
                // Direct messages are a welcome affordance only: a goodbye DM
                // would be sent to somebody who has already left the guild.
                false,
            ),
        };

        if !enabled {
            debug!(guild_id = query.guild_id, "no message is configured");
            return Ok(None);
        }

        // Enabled with no channel is a configuration mistake, and returning
        // `None` keeps it from being an error on every single join.
        let Some(channel_id) = channel_id else {
            debug!(
                guild_id = query.guild_id,
                "a message is enabled but no channel is set"
            );
            return Ok(None);
        };

        let context = TemplateContext {
            discord_user_id: query.discord_user_id,
            username: query.username.clone(),
            guild_name: query.guild_name.clone(),
            member_count: query.member_count,
        };

        // A guild that enabled welcomes without writing a message would otherwise
        // post nothing at all; the default is what the dashboard shows as the
        // placeholder text, so it is the least surprising thing to send.
        let template = match template {
            Some(template) => template.to_string(),
            None => default_template(query.kind).to_string(),
        };

        let content = WelcomeConfig::render(&template, &context);

        info!(
            guild_id = query.guild_id,
            channel_id,
            kind = ?query.kind,
            "welcome message built"
        );

        Ok(Some(WelcomeMessage {
            channel_id,
            content,
            embed_color,
            send_dm,
        }))
    }
}

/// The message used when a guild enabled the feature but never wrote one.
///
/// Rendered through the same `render` as a stored template, so the placeholders
/// mean exactly what they mean everywhere else.
fn default_template(kind: WelcomeKind) -> &'static str {
    match kind {
        WelcomeKind::Welcome => "Welcome {mention} to {guild}! You are member #{memberCount}.",
        // A goodbye has nobody to greet, and `{mention}` would ping somebody who
        // has already gone.
        WelcomeKind::Goodbye => "{user} has left {guild}.",
    }
}
