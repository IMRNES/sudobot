use tracing::info;
use uuid::Uuid;

use crate::application::ports::{PanelPublishError, RolePanelPublisher};
use crate::domain::errors::{Page, PaginationParams};
use crate::domain::welcome::{
    RolePanel, RolePanelOption, RolePanelWithOptions, WelcomeError, WelcomeRepository,
    panel_not_found, parse_behavior, validate_button_style, validate_option_count,
};

/// The guild's panels, each with its option count.
///
/// The count, not the options: a list screen renders "5 buttons" and fetches the
/// panel itself when one is opened, so the buttons stay off the list query.
pub struct ListRolePanelsUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> ListRolePanelsUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<Vec<(RolePanel, usize)>, WelcomeError> {
        // One page of panels per guild: a Discord guild with more than a hundred
        // self-assign panels is not a case worth paginating the API for.
        let params = PaginationParams {
            page: 1,
            per_page: 100,
        };
        let page: Page<RolePanel> = self.welcome.list_panels(guild_id, &params).await?;

        let mut panels = Vec::with_capacity(page.data.len());
        for panel in page.data {
            let count = self.welcome.count_options(panel.id).await?;
            panels.push((panel, count));
        }

        Ok(panels)
    }
}

/// One panel, addressed by its slug.
pub struct GetRolePanelUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> GetRolePanelUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// A panel that does not exist here — or exists in another guild — is a
    /// not-found: the slug is only unique inside a guild, so the guild check is
    /// what stops one guild's panel being read through another's path.
    pub async fn execute(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<RolePanelWithOptions, WelcomeError> {
        self.welcome
            .find_panel(guild_id, slug)
            .await?
            .ok_or(WelcomeError::PanelNotFound)
    }

    /// The same panel, addressed by id, for the paths that have no slug (a button
    /// press, or the redirect after adding an option).
    ///
    /// Takes the guild and filters on it. An id alone is not a tenancy boundary:
    /// `/{guild_id}/panels/{panel_id}` authorises against the *path's* guild, so a
    /// read that ignored it would hand back a panel belonging to another guild to
    /// anyone who guessed a UUID. The write on the same path was already scoped,
    /// which made the response the only leak.
    pub async fn execute_by_id(
        &self,
        guild_id: u64,
        panel_id: Uuid,
    ) -> Result<RolePanelWithOptions, WelcomeError> {
        self.welcome
            .find_panel_by_id(panel_id)
            .await?
            .filter(|panel| panel.panel.guild_id == guild_id)
            .ok_or_else(|| panel_not_found(panel_id))
    }
}

/// Creates a panel or replaces its header.
#[derive(Debug, Clone)]
pub struct UpsertRolePanelCommand {
    pub guild_id: u64,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub channel_id: Option<u64>,
}

pub struct UpsertRolePanelUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> UpsertRolePanelUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// Idempotent by slug: the dashboard saves the header without knowing whether
    /// the panel has been created yet, and two POSTs must not produce two panels
    /// with one slug.
    pub async fn execute(
        &self,
        command: UpsertRolePanelCommand,
    ) -> Result<RolePanelWithOptions, WelcomeError> {
        let title = command.title.trim().to_string();
        if title.is_empty() {
            return Err(WelcomeError::InvalidMessage);
        }

        let existing = self
            .welcome
            .find_panel(command.guild_id, &command.slug)
            .await?;

        let panel = match existing {
            Some(existing) => {
                let updated = RolePanel {
                    title,
                    description: command.description,
                    // `None` means "leave the channel alone"; a panel without one
                    // is simply not postable yet.
                    channel_id: command.channel_id.or(existing.panel.channel_id),
                    updated_at: chrono::Utc::now(),
                    ..existing.panel
                };
                self.welcome.upsert_panel(updated).await?
            }
            None => {
                let now = chrono::Utc::now();
                let created = RolePanel {
                    id: Uuid::new_v4(),
                    guild_id: command.guild_id,
                    slug: command.slug.clone(),
                    title,
                    description: command.description,
                    channel_id: command.channel_id,
                    message_id: None,
                    is_published: false,
                    created_at: now,
                    updated_at: now,
                };
                self.welcome.upsert_panel(created).await?
            }
        };

        info!(
            guild_id = command.guild_id,
            slug = %panel.slug,
            "role panel saved"
        );

        let options: Vec<RolePanelOption> = self.welcome.list_options(panel.id).await?;
        Ok(RolePanelWithOptions { panel, options })
    }
}

/// Adds a button to a panel.
#[derive(Debug, Clone)]
pub struct AddPanelOptionCommand {
    pub guild_id: u64,
    pub panel_id: Uuid,
    pub label: String,
    pub emoji: Option<String>,
    pub role_id: u64,
    pub style: i16,
    pub behavior: String,
}

pub struct AddPanelOptionUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> AddPanelOptionUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    pub async fn execute(
        &self,
        command: AddPanelOptionCommand,
    ) -> Result<RolePanelOption, WelcomeError> {
        let panel = self
            .welcome
            .find_panel_by_id(command.panel_id)
            .await?
            .filter(|panel| panel.panel.guild_id == command.guild_id)
            .ok_or(WelcomeError::PanelNotFound)?;

        // Discord rejects the whole message when a component row overflows, so the
        // limit is enforced before the button is stored rather than when the bot
        // tries to post it.
        let existing = self.welcome.count_options(panel.panel.id).await?;
        validate_option_count(existing, 1)?;

        let behavior = parse_behavior(&command.behavior)?;
        let style = validate_button_style(command.style)?;

        let position = existing as i32;
        let option = RolePanelOption {
            id: Uuid::new_v4(),
            panel_id: panel.panel.id,
            guild_id: command.guild_id,
            // Derived from the panel and the position so it is unique per button
            // and stable across restarts and re-renders; the unique index on
            // `custom_id` is what makes the button-press lookup a single query.
            custom_id: RolePanelOption::build_custom_id(panel.panel.id, existing),
            label: command.label.trim().to_string(),
            emoji: command.emoji,
            role_id: command.role_id,
            style,
            position,
            behavior,
        };

        let created = self.welcome.create_option(option).await?;

        info!(
            guild_id = command.guild_id,
            panel_id = %panel.panel.id,
            role_id = command.role_id,
            "panel option added"
        );

        Ok(created)
    }
}

/// Removes a button from a panel.
pub struct DeletePanelOptionUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> DeletePanelOptionUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// The panel id is part of the signature so the option cannot be deleted
    /// through the wrong panel's path; the two ids must agree.
    ///
    /// `guild_id` scopes the whole operation. The handler authorizes against the
    /// guild in the path, so without this check a caller holding `autorole:write`
    /// in guild A could delete a button belonging to guild B by naming B's panel and
    /// option ids under A's path.
    pub async fn execute(
        &self,
        guild_id: u64,
        panel_id: Uuid,
        option_id: Uuid,
    ) -> Result<(), WelcomeError> {
        self.welcome
            .find_panel_by_id(panel_id)
            .await?
            .filter(|panel| panel.panel.guild_id == guild_id)
            .ok_or(WelcomeError::PanelNotFound)?;

        let options = self.welcome.list_options(panel_id).await?;
        let option = options
            .into_iter()
            .find(|option| option.id == option_id)
            .ok_or(WelcomeError::OptionNotFound)?;

        self.welcome.delete_option(option.id).await?;

        info!(
            guild_id,
            panel_id = %panel_id,
            option_id = %option.id,
            "panel option deleted"
        );

        Ok(())
    }
}

/// Posts a panel to Discord and records where it landed.
///
/// The use case owns the whole operation rather than splitting it: posting and
/// recording have to agree, and a caller that could do one without the other would
/// eventually post twice. Re-publishing an already-published panel *edits* the
/// existing message, so an admin fixing a button does not leave a second copy in
/// the channel.
pub struct PublishRolePanelUseCase<W, P> {
    welcome: W,
    publisher: P,
}

impl<W: WelcomeRepository, P: RolePanelPublisher> PublishRolePanelUseCase<W, P> {
    pub fn new(welcome: W, publisher: P) -> Self {
        Self { welcome, publisher }
    }

    /// Posts (or edits) the panel and stores the resulting ids.
    ///
    /// The channel comes from the request rather than the stored panel so an admin
    /// can move a panel to another channel; whatever they pass becomes the recorded
    /// channel, so the next publish edits there.
    pub async fn execute(
        &self,
        guild_id: u64,
        panel_id: Uuid,
        channel_id: u64,
    ) -> Result<RolePanelWithOptions, WelcomeError> {
        let existing = self
            .welcome
            .find_panel_by_id(panel_id)
            .await?
            .filter(|panel| panel.panel.guild_id == guild_id)
            .ok_or(WelcomeError::PanelNotFound)?;

        // The buttons are rendered with the panel, so a publish always posts what
        // the database currently holds rather than what the browser last saw.
        let options = self.welcome.list_options(panel_id).await?;
        let rendered = RolePanelWithOptions {
            panel: existing.panel.clone(),
            options,
        };

        // Refused before Discord sees it: an empty panel would post a message that
        // does nothing, and the API would report success.
        if !rendered.is_publishable() {
            return Err(if rendered.options.is_empty() {
                WelcomeError::EmptyPanel
            } else {
                WelcomeError::TooManyOptions(
                    rendered.options.len() - RolePanelOption::MAX_PER_PANEL,
                )
            });
        }

        let message_id = match existing.panel.message_id {
            // Already posted: rewrite in place. `None` from Discord means the message
            // is gone, in which case the edit fails and the caller sees it rather
            // than silently ending up with no panel.
            Some(message_id) => {
                self.publisher
                    .edit(channel_id, message_id, &rendered)
                    .await
                    .map_err(publish_error)?;
                message_id
            }
            None => self
                .publisher
                .post(channel_id, &rendered)
                .await
                .map_err(publish_error)?,
        };

        let panel = self
            .welcome
            .mark_published(panel_id, channel_id, message_id)
            .await?;

        info!(
            guild_id,
            panel_id = %panel.id,
            channel_id,
            message_id,
            options = rendered.options.len(),
            "role panel published"
        );

        let options = self.welcome.list_options(panel.id).await?;
        Ok(RolePanelWithOptions { panel, options })
    }
}

/// A publish failure is the installation's problem unless the panel itself is at
/// fault, so the two are separated: an unconfigured bot token is `Unavailable`
/// (retrying will not help), and an unpublishable panel is the panel's own error.
fn publish_error(error: PanelPublishError) -> WelcomeError {
    match error {
        PanelPublishError::Invalid(inner) => inner,
        other => WelcomeError::Publisher(other.to_string()),
    }
}

/// A button press, resolved to the panel and option it belongs to.
#[derive(Debug, Clone)]
pub struct ResolvePanelButtonQuery {
    pub custom_id: String,
}

/// What the bot needs to act on a press.
#[derive(Debug, Clone)]
pub struct PanelButton {
    pub panel_id: Uuid,
    pub guild_id: u64,
    pub option: RolePanelOption,
}

/// Looks up the button a member pressed.
pub struct ResolvePanelButtonUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> ResolvePanelButtonUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// Not-found is `Ok(None)` rather than an error: a button on a panel that was
    /// deleted, or one still sitting in an old message, is ordinary, and the
    /// caller answers it with "this panel is no longer available" instead of
    /// logging a failure for every click. `Err` is reserved for the database
    /// actually failing.
    pub async fn execute(
        &self,
        query: ResolvePanelButtonQuery,
    ) -> Result<Option<PanelButton>, WelcomeError> {
        let Some(option) = self
            .welcome
            .find_option_by_custom_id(&query.custom_id)
            .await?
        else {
            return Ok(None);
        };

        // The panel is read so the guild comes from the panel rather than from the
        // interaction, which is what lets the caller verify the press happened in
        // the guild it is about to change roles in.
        let Some(panel) = self.welcome.find_panel_by_id(option.panel_id).await? else {
            return Ok(None);
        };

        Ok(Some(PanelButton {
            panel_id: panel.panel.id,
            guild_id: panel.panel.guild_id,
            option,
        }))
    }
}
