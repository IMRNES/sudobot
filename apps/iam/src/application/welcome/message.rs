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
    /// The server's branding card, rendered into a Discord embed alongside
    /// `content`. Empty when the guild has not configured one, in which case the
    /// message stays a plain post — a card with no fields would be a grey box.
    pub embed: Option<WelcomeEmbed>,
    /// Whether the same text should also go to the member's direct messages.
    pub send_dm: bool,
}

/// The rich embed that accompanies a welcome or goodbye.
///
/// Mirrors Discord's embed shape rather than serenity's builder: the application
/// layer must not depend on the gateway library, and the bot already translates
/// `embed_color` the same way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeEmbed {
    pub title: Option<String>,
    pub description: Option<String>,
    /// Shown as the small grey line above the title.
    pub author: Option<String>,
    /// The large image under the description.
    pub image_url: Option<String>,
    /// The small image in the corner.
    pub thumbnail_url: Option<String>,
}

impl WelcomeEmbed {
    /// Whether anything would actually render.
    ///
    /// A card carrying only a thumbnail is worth posting; one carrying nothing at
    /// all is not, and Discord rejects an embed with no visible content.
    pub fn is_visible(&self) -> bool {
        self.title.is_some()
            || self.description.is_some()
            || self.author.is_some()
            || self.image_url.is_some()
            || self.thumbnail_url.is_some()
    }
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

        // The card is built from the stored branding. Building it here rather than
        // in the bot means the dashboard preview and the posted message cannot
        // drift apart, and a guild with no card configured simply gets no embed.
        let embed = WelcomeEmbed {
            title: config.embed_title.clone(),
            description: config.embed_description.clone(),
            author: config.embed_author.clone(),
            image_url: config.embed_image_url.clone(),
            thumbnail_url: config.embed_thumbnail_url.clone(),
        };
        let embed = embed.is_visible().then_some(embed);

        info!(
            guild_id = query.guild_id,
            channel_id,
            kind = ?query.kind,
            has_embed = embed.is_some(),
            "welcome message built"
        );

        Ok(Some(WelcomeMessage {
            channel_id,
            content,
            embed_color,
            embed,
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
