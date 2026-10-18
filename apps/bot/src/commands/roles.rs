use poise::serenity_prelude as serenity;

use iam::application::autorole::ListAutoRolesUseCase;
use iam::application::welcome::ListRolePanelsUseCase;

use crate::{Context, Error};

/// Point members at the dashboard, where the real configuration lives.
///
/// Read-only on purpose. These commands exist so a member can discover what the
/// server offers without an admin having to tell them; every button an admin would
/// press belongs on the dashboard, where it can be reviewed before it is saved.
#[poise::command(slash_command, guild_only, subcommands("panels", "autoroles"))]
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

    let published: Vec<_> = panels
        .into_iter()
        .filter(|(panel, _)| panel.is_published)
        .collect();

    if published.is_empty() {
        ctx.say("This server has no role panels yet.").await?;
        return Ok(());
    }

    let lines = published
        .iter()
        .map(|(panel, option_count)| {
            let channel = match panel.channel_id {
                Some(channel_id) => format!("<#{channel_id}>"),
                None => "an unset channel".to_string(),
            };

            format!(
                "**{}** — {} option{} in {channel}",
                panel.title,
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
