use tracing::{debug, info};

use crate::domain::welcome::{WelcomeConfig, WelcomeError, WelcomeRepository, validate_template};

/// The guild's welcome and goodbye configuration.
///
/// The repository answers with defaults for a guild that has never saved any, so
/// "not configured" and "configured to nothing" are the same readable response
/// rather than a 404 the client has to special-case.
pub struct GetWelcomeConfigUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> GetWelcomeConfigUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<WelcomeConfig, WelcomeError> {
        Ok(self.welcome.find_config(guild_id).await?)
    }
}

/// A patch to a guild's welcome configuration.
///
/// Every field is doubly optional, and both layers mean something:
///
/// * the **outer** `Option` is "is this field part of the patch at all" — `None`
///   leaves the stored value alone, which is what lets the dashboard send one
///   toggle without resending the message beside it;
/// * the **inner** `Option` is "set it to null" — `Some(None)` clears a channel
///   or a colour.
///
/// That is the one genuinely surprising shape in this file. It exists because
/// Discord snowflakes are not nullable in a form: a client cannot send a channel
/// it does not have, so clearing one has to be said explicitly, and `null` is the
/// only JSON that can say it without inventing a sentinel id.
#[derive(Debug, Clone)]
pub struct UpdateWelcomeConfigCommand {
    pub guild_id: u64,
    pub welcome_enabled: Option<bool>,
    pub welcome_channel_id: Option<Option<u64>>,
    pub welcome_message: Option<String>,
    pub welcome_embed_color: Option<Option<i32>>,
    /// The shared embed card. `Some(None)` clears a field, `None` leaves it alone.
    pub embed_title: Option<Option<String>>,
    pub embed_description: Option<Option<String>>,
    pub embed_author: Option<Option<String>>,
    pub embed_image_url: Option<Option<String>>,
    pub embed_thumbnail_url: Option<Option<String>>,
    pub welcome_dm_enabled: Option<bool>,
    pub goodbye_enabled: Option<bool>,
    pub goodbye_channel_id: Option<Option<u64>>,
    pub goodbye_message: Option<String>,
    pub goodbye_embed_color: Option<Option<i32>>,
}

pub struct UpdateWelcomeConfigUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> UpdateWelcomeConfigUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// Merges the patch onto the stored configuration and saves it.
    ///
    /// Read-modify-write rather than a partial UPDATE because the templates must
    /// be validated as a whole against the merged result: saving a message that
    /// references a placeholder the renderer does not know would otherwise post
    /// literally in every welcome.
    pub async fn execute(
        &self,
        command: UpdateWelcomeConfigCommand,
    ) -> Result<WelcomeConfig, WelcomeError> {
        let mut config = self.welcome.find_config(command.guild_id).await?;

        apply(command.welcome_enabled, &mut config.welcome_enabled);
        apply(command.welcome_dm_enabled, &mut config.welcome_dm_enabled);
        apply(command.goodbye_enabled, &mut config.goodbye_enabled);

        // `Some(inner)` assigns the inner value, including its `None`; a bare
        // `None` on the outer leaves the stored channel untouched.
        if let Some(channel_id) = command.welcome_channel_id {
            config.welcome_channel_id = channel_id;
        }
        if let Some(channel_id) = command.goodbye_channel_id {
            config.goodbye_channel_id = channel_id;
        }
        if let Some(colour) = command.welcome_embed_color {
            config.welcome_embed_color = colour;
        }

        // The card is shared, so it is applied the same way whichever message kind
        // the caller was editing.
        if let Some(title) = command.embed_title {
            config.embed_title = title;
        }
        if let Some(description) = command.embed_description {
            config.embed_description = description;
        }
        if let Some(author) = command.embed_author {
            config.embed_author = author;
        }
        if let Some(image) = command.embed_image_url {
            config.embed_image_url = image;
        }
        if let Some(thumbnail) = command.embed_thumbnail_url {
            config.embed_thumbnail_url = thumbnail;
        }
        if let Some(colour) = command.goodbye_embed_color {
            config.goodbye_embed_color = colour;
        }

        // A message being set is validated; one being cleared (an empty string is
        // rejected by the domain's own length rule) is not a template at all.
        if let Some(message) = &command.welcome_message {
            config.welcome_message = Some(validate_template(message)?);
        }
        if let Some(message) = &command.goodbye_message {
            config.goodbye_message = Some(validate_template(message)?);
        }

        let stored = self.welcome.upsert_config(config).await?;

        if stored.welcome_enabled && stored.welcome_channel_id.is_none() {
            // Enabled without a channel is a configuration that silently does
            // nothing; saying so here saves the "why did nobody get welcomed?"
            // support ticket.
            debug!(
                guild_id = command.guild_id,
                "welcomes are enabled but no channel is set"
            );
        }

        info!(
            guild_id = command.guild_id,
            welcome_enabled = stored.welcome_enabled,
            goodbye_enabled = stored.goodbye_enabled,
            "welcome configuration updated"
        );

        Ok(stored)
    }
}

/// Assigns a patch field when it is present.
fn apply<T>(value: Option<T>, slot: &mut T) {
    if let Some(value) = value {
        *slot = value;
    }
}
