use poise::serenity_prelude as serenity;

use iam::application::leveling::{LeaderboardRow, LevelLeaderboardUseCase};

use crate::{Context, Error};
use iam::domain::leveling::LevelingRepository;

/// Show a member's level, XP and rank.
#[poise::command(slash_command, guild_only)]
pub async fn rank(
    ctx: Context<'_>,
    #[description = "Whose card to show"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let target = user.unwrap_or_else(|| ctx.author().clone());
    ctx.defer().await?;

    let profile = ctx
        .data()
        .leveling
        .ensure_profile(guild_id.get(), target.id.get())
        .await?;

    let config = ctx.data().leveling.find_config(guild_id.get()).await?;
    let rank = LevelLeaderboardUseCase::new(ctx.data().leveling.clone())
        .rank_of(guild_id.get(), target.id.get())
        .await?;

    let embed = serenity::CreateEmbed::new()
        .title(format!("{} — level {}", target.name, profile.level))
        .field("Rank", format!("#{rank}"), true)
        .field(
            "XP",
            format!(
                "{} / {}",
                profile.xp,
                config.curve.xp_for_level(profile.level)
            ),
            true,
        )
        .field("Messages", profile.message_count.to_string(), true)
        .description(progress_bar(profile.progress(config.curve)))
        .colour(0x9B59B6);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Show your own level at a glance.
///
/// Duplicates `/rank`'s body rather than calling it: a `#[poise::command]` function
/// is a command registration, not a helper — poise's macro rewrites its signature,
/// so it cannot be invoked as an ordinary async fn.
#[poise::command(slash_command, guild_only)]
pub async fn level(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let target = ctx.author().clone();
    ctx.defer().await?;

    let profile = ctx
        .data()
        .leveling
        .ensure_profile(guild_id.get(), target.id.get())
        .await?;

    let config = ctx.data().leveling.find_config(guild_id.get()).await?;
    let position = LevelLeaderboardUseCase::new(ctx.data().leveling.clone())
        .rank_of(guild_id.get(), target.id.get())
        .await?;

    let embed = serenity::CreateEmbed::new()
        .title(format!("{} — level {}", target.name, profile.level))
        .field("Rank", format!("#{position}"), true)
        .field(
            "XP",
            format!(
                "{} / {}",
                profile.xp,
                config.curve.xp_for_level(profile.level)
            ),
            true,
        )
        .field("Messages", profile.message_count.to_string(), true)
        .description(progress_bar(profile.progress(config.curve)))
        .colour(0x9B59B6);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// The server's XP leaderboard.
#[poise::command(slash_command, guild_only)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let use_case = LevelLeaderboardUseCase::new(ctx.data().leveling.clone());
    let page = use_case
        .execute(
            guild_id.get(),
            &iam::domain::errors::PaginationParams {
                page: 1,
                per_page: 10,
            },
        )
        .await?;

    if page.data.is_empty() {
        ctx.say("Nobody has earned XP yet.").await?;
        return Ok(());
    }

    // Rendered on the leveling config's curve so the numbers match what `/rank`
    // shows; fetching it once here rather than per row is the point.
    let config = ctx.data().leveling.find_config(guild_id.get()).await?;

    let lines = page
        .data
        .iter()
        .map(render_row)
        .collect::<Vec<_>>()
        .join("\n");

    let embed = serenity::CreateEmbed::new()
        .title("Leaderboard")
        .description(lines)
        .footer(serenity::CreateEmbedFooter::new(format!(
            "Curve: {}",
            config.curve.as_str()
        )))
        .colour(0x9B59B6);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

fn render_row(row: &LeaderboardRow) -> String {
    format!(
        "**{}.** <@{}> — level {} ({} xp)",
        row.rank, row.discord_user_id, row.level, row.total_xp
    )
}

/// A ten-cell progress bar.
///
/// Text rather than an image: it costs no request, and Discord renders it
/// identically on every platform.
fn progress_bar(progress: f64) -> String {
    let filled = (progress * 10.0).round().clamp(0.0, 10.0) as usize;
    format!(
        "{}{}  {:.0}%",
        "▰".repeat(filled),
        "▱".repeat(10 - filled),
        progress * 100.0
    )
}
