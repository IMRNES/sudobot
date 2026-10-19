use chrono::{DateTime, Utc};

/// Welcome and goodbye configuration for one guild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WelcomeConfig {
    pub guild_id: u64,

    pub welcome_enabled: bool,
    pub welcome_channel_id: Option<u64>,
    /// Template containing `{user}`, `{mention}`, `{guild}` and `{memberCount}`.
    pub welcome_message: Option<String>,
    /// Decimal RGB, as Discord's embed colours are.
    pub welcome_embed_color: Option<i32>,
    pub welcome_dm_enabled: bool,

    pub goodbye_enabled: bool,
    pub goodbye_channel_id: Option<u64>,
    pub goodbye_message: Option<String>,
    pub goodbye_embed_color: Option<i32>,

    pub updated_at: DateTime<Utc>,
}

impl WelcomeConfig {
    pub fn default_for(guild_id: u64) -> Self {
        Self {
            guild_id,
            welcome_enabled: false,
            welcome_channel_id: None,
            welcome_message: None,
            welcome_embed_color: None,
            welcome_dm_enabled: false,
            goodbye_enabled: false,
            goodbye_channel_id: None,
            goodbye_message: None,
            goodbye_embed_color: None,
            updated_at: Utc::now(),
        }
    }

    /// Whether a welcome can actually be delivered.
    ///
    /// Enabled with no channel is a configuration mistake, and reporting it here
    /// keeps the bot from failing silently on every join.
    pub fn welcomes_in_channel(&self) -> bool {
        self.welcome_enabled && self.welcome_channel_id.is_some()
    }

    pub fn says_goodbye_in_channel(&self) -> bool {
        self.goodbye_enabled && self.goodbye_channel_id.is_some()
    }

    /// Renders a template, substituting the supported placeholders.
    ///
    /// A plain string replace rather than a template engine: the placeholder set
    /// is small, fixed, and must not be able to execute anything.
    pub fn render(template: &str, context: &TemplateContext) -> String {
        template
            .replace("{user}", &context.username)
            .replace("{mention}", &format!("<@{}>", context.discord_user_id))
            .replace("{guild}", &context.guild_name)
            .replace("{memberCount}", &context.member_count.to_string())
    }
}

/// The values a welcome template may reference.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    pub discord_user_id: u64,
    pub username: String,
    pub guild_name: String,
    pub member_count: i64,
}

/// A button-role panel: the message members self-assign roles from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolePanel {
    pub id: uuid::Uuid,
    pub guild_id: u64,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub channel_id: Option<u64>,
    /// The posted message, once published, so edits can target it in place.
    pub message_id: Option<u64>,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl RolePanel {
    /// Whether the panel is ready to be posted or edited.
    pub fn is_postable(&self) -> bool {
        self.channel_id.is_some() && !self.title.trim().is_empty()
    }
}

/// How pressing a button changes the member's roles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionBehavior {
    /// Press to add, press again to remove.
    Toggle,
    /// Press only ever adds.
    Add,
    /// Press only ever removes.
    Remove,
    /// Pressing this option removes its siblings — a radio group.
    Exclusive,
}

impl OptionBehavior {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "toggle" => Some(Self::Toggle),
            "add" => Some(Self::Add),
            "remove" => Some(Self::Remove),
            "exclusive" => Some(Self::Exclusive),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Toggle => "toggle",
            Self::Add => "add",
            Self::Remove => "remove",
            Self::Exclusive => "exclusive",
        }
    }
}

/// One button on a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolePanelOption {
    pub id: uuid::Uuid,
    pub panel_id: uuid::Uuid,
    pub guild_id: u64,
    /// Stable id Discord echoes back on press; the lookup key for a click.
    pub custom_id: String,
    pub label: String,
    pub emoji: Option<String>,
    pub role_id: u64,
    /// Discord button style: 1 primary, 2 secondary, 3 success, 4 danger.
    pub style: i16,
    pub position: i32,
    pub behavior: OptionBehavior,
}

impl RolePanelOption {
    /// The five rows of five Discord allows, in order.
    ///
    /// Encoding this here rather than at render time keeps the builder in the bot
    /// from having to know Discord's limit.
    pub const MAX_PER_ROW: usize = 5;
    pub const MAX_ROWS: usize = 5;
    pub const MAX_PER_PANEL: usize = Self::MAX_PER_ROW * Self::MAX_ROWS;

    pub fn build_custom_id(panel_id: uuid::Uuid, index: usize) -> String {
        // Discord caps custom ids at 100 characters; a UUID plus a index is 38.
        format!("rolepanel:{panel_id}:{index}")
    }
}

/// A panel with its options, as the bot needs it to render buttons.
#[derive(Debug, Clone)]
pub struct RolePanelWithOptions {
    pub panel: RolePanel,
    pub options: Vec<RolePanelOption>,
}

impl RolePanelWithOptions {
    /// Whether the panel fits in one Discord message.
    pub fn fits_in_one_message(&self) -> bool {
        self.options.len() <= RolePanelOption::MAX_PER_PANEL
    }

    /// Whether this panel can be turned into a Discord message at all.
    ///
    /// Two ways it cannot: no buttons, in which case the message would render but
    /// do nothing when clicked, and more than Discord's 25-component limit. Both
    /// the use case and the wire render ask this, so they cannot disagree about
    /// what is publishable.
    pub fn is_publishable(&self) -> bool {
        !self.options.is_empty() && self.fits_in_one_message()
    }
}
