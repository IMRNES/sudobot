use poise::serenity_prelude as serenity;

use iam::application::autorole::ListAutoRolesUseCase;
use iam::application::welcome::{
    GetRolePanelUseCase, ListRolePanelsUseCase, PublishRolePanelUseCase,
};

use crate::{Context, Error};

/// Point members at the dashboard, where the real configuration lives.
///
/// Read-only on purpose, with two exceptions: `post` writes the panel message,
/// because that is a Discord-side action only the bot can perform. Everything an
/// admin would *edit* stays on the dashboard, where it can be reviewed before it is
/// saved.
#[poise::command(slash_command, guild_only, subcommands("panels", "post", "autoroles"))]
pub async fn rolepanel(_ctx: Context<'_>) -> Result<(), Error> {
    // Never invoked: Discord refuses to run a parent command with subcommands.
    Ok(())
}

/// List this server's self-assignable role panels.
#[poise::command(slash_command, guild_only)]
pub async fn panels(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let use_case = ListRolePanelsUseCase::new(ctx.data().welcome.clone());
    let panels = use_case.execute(guild_id.get()).await?;

    if panels.is_empty() {
        ctx.say("This server has no role panels yet.").await?;
        return Ok(());
    }

    let lines = panels
        .iter()
        .map(|(panel, option_count)| {
            // The slug is what `post` takes, so it is the identifier shown here.
            let state = if panel.is_published {
                "published"
            } else {
                "draft"
            };

            format!(
                "**{}** (`{}`) — {} option{}, {state}",
                panel.title,
                panel.slug,
                option_count,
                if *option_count == 1 { "" } else { "s" }
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let embed = serenity::CreateEmbed::new()
        .title("Self-assignable roles")
        .description(lines)
        .colour(0x3498DB);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Post a panel's buttons into this channel.
///
/// This is the only way a panel reaches Discord. `PublishRolePanelUseCase` holds the
/// bot token through the API's publisher, and the same use case runs here, so the
/// message the bot writes is the one the dashboard records — and re-running this
/// edits it in place instead of posting a second copy.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn post(
    ctx: Context<'_>,
    #[description = "The panel's slug, as shown by /rolepanel panels"] slug: String,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let channel_id = ctx.channel_id();
    ctx.defer().await?;

    // Resolved by slug first: an admin knows the slug, not the UUID, and the API's
    // publish route takes an id.
    let lookup = GetRolePanelUseCase::new(ctx.data().welcome.clone());
    let panel = lookup.execute(guild_id.get(), &slug).await?;

    let use_case = PublishRolePanelUseCase::new(
        ctx.data().welcome.clone(),
        ctx.data().panel_publisher.clone(),
    );

    let published = use_case
        .execute(guild_id.get(), panel.panel.id, channel_id.get())
        .await?;

    let message = match published.panel.message_id {
        Some(message_id) => format!(
            "Posted **{}** with {} button{} in <#{}>: <https://discord.com/channels/{}/{}/{}>",
            published.panel.title,
            published.options.len(),
            if published.options.len() == 1 {
                ""
            } else {
                "s"
            },
            channel_id,
            guild_id,
            channel_id,
            message_id
        ),
        None => format!("Posted **{}**.", published.panel.title),
    };

    ctx.say(message).await?;
    Ok(())
}

/// Show one panel's buttons, for anyone who cannot see the message.
#[poise::command(slash_command, guild_only)]
pub async fn autoroles(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let use_case = ListAutoRolesUseCase::new(ctx.data().auto_roles.clone());
    let rules = use_case.execute(guild_id.get()).await?;

    if rules.is_empty() {
        ctx.say("This server grants no roles automatically.")
            .await?;
        return Ok(());
    }

    let lines = rules
        .iter()
        .map(|rule| {
            let when = match rule.trigger {
                iam::domain::autorole::AutoRoleTrigger::Join => "on join".to_string(),
                iam::domain::autorole::AutoRoleTrigger::Human => "once verified".to_string(),
                iam::domain::autorole::AutoRoleTrigger::Delay => {
                    format!("{} after joining", humanise_seconds(rule.delay_seconds))
                }
            };

            let state = if rule.is_active { "" } else { " *(paused)*" };

            format!("<@&{}> — {when}{state}", rule.role_id)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let embed = serenity::CreateEmbed::new()
        .title("Automatic roles")
        .description(lines)
        .colour(0x3498DB);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Renders a delay as something a person would say.
fn humanise_seconds(seconds: i32) -> String {
    match seconds {
        s if s >= 86_400 => format!("{} day(s)", s / 86_400),
        s if s >= 3_600 => format!("{} hour(s)", s / 3_600),
        s if s >= 60 => format!("{} minute(s)", s / 60),
        s => format!("{s} second(s)"),
    }
}
