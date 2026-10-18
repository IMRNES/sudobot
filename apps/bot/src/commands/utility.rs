use poise::serenity_prelude as serenity;

use crate::{Context, Error};
use iam::domain::guild::GuildRepository;

/// Check that the bot is alive and can reach the database.
///
/// Reports the database round trip rather than a bare "pong": a bot that answers
/// instantly while its database is unreachable is worse than one that is visibly
/// broken, because nobody notices until an economy command fails.
#[poise::command(slash_command, guild_only)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let started = std::time::Instant::now();
    ctx.defer().await?;

    let guild_count = match ctx.data().guilds.count().await {
        Ok(count) => count.to_string(),
        Err(error) => {
            tracing::error!(%error, "could not count guilds");
            "unavailable".to_string()
        }
    };

    let elapsed = started.elapsed();
    ctx.say(format!(
        "Pong — database round trip {:.0} ms, serving {guild_count} servers.",
        elapsed.as_secs_f64() * 1000.0
    ))
    .await?;

    Ok(())
}

/// Show what this bot offers.
///
/// Rendered from the framework's own command list rather than a hand-written
/// string: a command added to `commands::all()` appears here without anyone
/// remembering to update the help text.
#[poise::command(slash_command)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    let lines = ctx
        .framework()
        .options()
        .commands
        .iter()
        .map(|command| {
            let subcommands: Vec<&str> = command
                .subcommands
                .iter()
                .map(|sub| sub.name.as_ref())
                .collect();

            // `name` is a `Cow<str>` and `description` an `Option<Cow<str>>` in
            // poise 0.7; a command with no doc comment has no description, and
            // saying so beats printing an empty line.
            let description = match &command.description {
                Some(text) => text.as_ref(),
                None => "no description",
            };

            if subcommands.is_empty() {
                format!("`/{}` — {}", command.name, description)
            } else {
                format!(
                    "`/{}` — {} *({})*",
                    command.name,
                    description,
                    subcommands.join(", ")
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let embed = serenity::CreateEmbed::new()
        .title("Commands")
        .description(lines)
        .footer(serenity::CreateEmbedFooter::new(
            "Everything else is configured from the web dashboard.",
        ))
        .colour(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// What the bot knows about this server.
#[poise::command(slash_command, guild_only)]
pub async fn serverinfo(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let guild = ctx.data().guilds.find_by_id(guild_id.get()).await?;

    let Some(guild) = guild else {
        ctx.say("I do not have this server on record yet — try again in a moment.")
            .await?;
        return Ok(());
    };

    let embed = serenity::CreateEmbed::new()
        .title(&guild.name)
        .field("Members", guild.member_count.to_string(), true)
        .field("Server ID", guild.id.to_string(), true)
        .field(
            "Dashboard",
            if guild.bot_present {
                "linked"
            } else {
                "not linked"
            },
            true,
        )
        .colour(0x5865F2);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}
