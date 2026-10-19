use tracing::{info, warn};
use uuid::Uuid;

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

    /// The same panel, addressed by id, for the paths that have no slug (a
    /// button press, or the redirect after adding an option).
    pub async fn execute_by_id(
        &self,
        panel_id: Uuid,
    ) -> Result<RolePanelWithOptions, WelcomeError> {
        self.welcome
            .find_panel_by_id(panel_id)
            .await?
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

/// Records where the bot posted a panel.
pub struct PublishRolePanelUseCase<W> {
    welcome: W,
}

impl<W: WelcomeRepository> PublishRolePanelUseCase<W> {
    pub fn new(welcome: W) -> Self {
        Self { welcome }
    }

    /// The panel is postable iff `panel.is_postable()` — a channel and a title.
    /// The channel and message ids come from the caller because it is the bot that
    /// talked to Discord; this use case only records the result, so a later edit
    /// can target the same message instead of posting a second one.
    pub async fn execute(
        &self,
        guild_id: u64,
        panel_id: Uuid,
        channel_id: u64,
        message_id: u64,
    ) -> Result<RolePanelWithOptions, WelcomeError> {
        let existing = self
            .welcome
            .find_panel_by_id(panel_id)
            .await?
            .filter(|panel| panel.panel.guild_id == guild_id)
            .ok_or(WelcomeError::PanelNotFound)?;

        // The recorded channel wins for the postability check below: the bot has
        // already posted somewhere, and refusing now would leave the database
        // disagreeing with Discord.
        let mut panel = existing.panel.clone();
        panel.channel_id = Some(channel_id);

        if !panel.is_postable() {
            warn!(panel_id = %panel_id, "refused to publish a panel that is not postable");
            return Err(WelcomeError::MissingChannel);
        }

        let panel = self
            .welcome
            .mark_published(panel_id, channel_id, message_id)
            .await?;

        info!(
            guild_id,
            panel_id = %panel.id,
            channel_id,
            message_id,
            "role panel published"
        );

        let options = self.welcome.list_options(panel.id).await?;
        Ok(RolePanelWithOptions { panel, options })
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
