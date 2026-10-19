//! Posting a role panel to a Discord channel.
//!
//! The panel lives in the database; the message members click lives in Discord.
//! Something has to bridge the two, and it cannot be the browser — a dashboard
//! visitor has no authority to post as the bot. So the API posts with the bot
//! token, which is the only credential that owns the message.
//!
//! The trait is separate from [`super::DiscordOAuthClient`] because the two use
//! different credentials and answer different questions: OAuth asks "who is this",
//! the bot token asks "may I write here". An installation whose OAuth client is
//! unconfigured can still post panels, and vice versa.

use std::future::Future;
use thiserror::Error;

use crate::domain::welcome::{RolePanelWithOptions, WelcomeError};

#[derive(Debug, Error)]
pub enum PanelPublishError {
    /// No bot token, so nothing here can reach a channel.
    #[error("this installation has no discord bot token configured")]
    NotConfigured,

    #[error("discord refused the message: {0}")]
    Api(String),

    #[error("discord is unreachable: {0}")]
    Transport(String),

    #[error("discord returned a response we could not read: {0}")]
    MalformedResponse(String),

    #[error("rate limited by discord; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    /// The panel itself cannot be rendered — no options, or more than 25.
    ///
    /// Carried here rather than checked twice so the caller gets one answer: a
    /// panel the bot refuses to draw is a client error on the panel, not a
    /// Discord failure.
    #[error(transparent)]
    Invalid(#[from] WelcomeError),
}

/// Renders and posts role panels.
pub trait RolePanelPublisher: Send + Sync {
    /// Whether a bot token is configured.
    fn is_configured(&self) -> bool;

    /// Posts the panel to `channel_id`, returning the new message's id.
    fn post(
        &self,
        channel_id: u64,
        panel: &RolePanelWithOptions,
    ) -> impl Future<Output = Result<u64, PanelPublishError>> + Send;

    /// Rewrites an existing panel message in place.
    ///
    /// Publishing twice must not leave two messages in the channel, so a panel that
    /// already has a message id is edited rather than posted again.
    fn edit(
        &self,
        channel_id: u64,
        message_id: u64,
        panel: &RolePanelWithOptions,
    ) -> impl Future<Output = Result<(), PanelPublishError>> + Send;
}

/// The wire shape Discord expects, built from a domain panel.
///
/// Exposed so the render can be tested without a network: the button rows are the
/// part that is easy to get wrong — Discord rejects more than five buttons per row
/// and more than five rows per message.
pub fn render_panel(panel: &RolePanelWithOptions) -> Result<serde_json::Value, WelcomeError> {
    use crate::domain::welcome::RolePanelOption;

    // The domain decides what Discord will accept, so this render cannot drift from
    // the use case that calls it.
    if !panel.is_publishable() {
        return Err(if panel.options.is_empty() {
            WelcomeError::EmptyPanel
        } else {
            WelcomeError::TooManyOptions(panel.options.len() - RolePanelOption::MAX_PER_PANEL)
        });
    }

    let mut embeds = Vec::new();
    if !panel.panel.title.trim().is_empty() {
        let mut embed = serde_json::json!({
            "title": panel.panel.title,
            "color": 0x3498DB,
        });

        if let Some(description) = panel
            .panel
            .description
            .as_deref()
            .map(str::trim)
            .filter(|text| !text.is_empty())
        {
            embed["description"] = serde_json::Value::String(description.to_string());
        }

        embeds.push(embed);
    }

    let components = panel
        .options
        .chunks(RolePanelOption::MAX_PER_ROW)
        .map(|row| {
            let buttons = row
                .iter()
                .map(|option| {
                    let mut button = serde_json::json!({
                        "type": 2,
                        // Clamped rather than trusted: a row in the database with a
                        // style outside 1..=4 makes Discord reject the whole message,
                        // and "grey button" is a better outcome than "nothing posted".
                        "style": clamp_style(option.style),
                        "label": option.label,
                        "custom_id": option.custom_id,
                    });

                    if let Some(emoji) = option
                        .emoji
                        .as_deref()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                    {
                        button["emoji"] = serde_json::json!({ "name": emoji });
                    }

                    button
                })
                .collect::<Vec<_>>();

            serde_json::json!({ "type": 1, "components": buttons })
        })
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "embeds": embeds,
        "components": components,
        // A role panel is not a conversation starter; mentioning everyone in it
        // would ping the server every time an admin fixes a typo.
        "allowed_mentions": { "parse": [] },
    }))
}

/// Discord's button styles are 1 through 4; anything else is a bug in the database.
fn clamp_style(style: i16) -> i16 {
    if (1..=4).contains(&style) { style } else { 2 }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::welcome::{OptionBehavior, RolePanel, RolePanelOption};
    use chrono::Utc;
    use uuid::Uuid;

    fn panel_with(options: usize) -> RolePanelWithOptions {
        let panel_id = Uuid::nil();
        let guild_id = 1;

        RolePanelWithOptions {
            panel: RolePanel {
                id: panel_id,
                guild_id,
                slug: "self-roles".to_string(),
                title: "Pick your roles".to_string(),
                description: Some("Choose what you want.".to_string()),
                channel_id: Some(2),
                message_id: None,
                is_published: false,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            options: (0..options)
                .map(|index| RolePanelOption {
                    id: Uuid::new_v4(),
                    panel_id,
                    guild_id,
                    custom_id: RolePanelOption::build_custom_id(panel_id, index),
                    label: format!("Role {index}"),
                    emoji: None,
                    role_id: 100 + index as u64,
                    style: 2,
                    position: index as i32,
                    behavior: OptionBehavior::Toggle,
                })
                .collect(),
        }
    }

    #[test]
    fn a_full_panel_becomes_five_rows_of_five() {
        let rendered = render_panel(&panel_with(25)).unwrap();
        let rows = rendered["components"].as_array().unwrap();

        assert_eq!(rows.len(), 5);
        for row in rows {
            assert_eq!(row["components"].as_array().unwrap().len(), 5);
        }
    }

    #[test]
    fn a_partial_row_is_its_own_row() {
        let rendered = render_panel(&panel_with(7)).unwrap();
        let rows = rendered["components"].as_array().unwrap();

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["components"].as_array().unwrap().len(), 5);
        assert_eq!(rows[1]["components"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_panel_without_buttons_is_refused() {
        assert!(render_panel(&panel_with(0)).is_err());
    }

    #[test]
    fn more_than_five_rows_is_refused() {
        assert!(render_panel(&panel_with(26)).is_err());
    }

    /// A style outside Discord's range must not take the whole message down.
    #[test]
    fn an_out_of_range_style_falls_back_to_grey() {
        let mut panel = panel_with(1);
        panel.options[0].style = 99;

        let rendered = render_panel(&panel).unwrap();
        assert_eq!(rendered["components"][0]["components"][0]["style"], 2);
    }

    #[test]
    fn the_custom_id_matches_what_a_press_echoes_back() {
        let panel = panel_with(3);
        let rendered = render_panel(&panel).unwrap();

        for (index, option) in panel.options.iter().enumerate() {
            assert_eq!(
                rendered["components"][0]["components"][index]["custom_id"],
                option.custom_id
            );
        }
    }
}
