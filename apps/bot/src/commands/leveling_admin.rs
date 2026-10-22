//! `/leveling` — the administrative side of the XP system.
//!
//! Split from the member-facing `/rank`, `/level` and `/leaderboard`: those are for
//! anyone, these need `Manage Roles`, and poise propagates a parent command's
//! `default_member_permissions` to its subcommands. Putting them under one parent
//! is what makes "is this command an admin command?" answerable from the command
//! name rather than from a permission check buried in a body.

use poise::serenity_prelude as serenity;

use iam::domain::leveling::{LevelingRepository, descending, rung_for_level};

use crate::{Context, Error};

/// Administer the leveling system: the ladder, and placing members on it.
///
/// Never invoked directly — Discord refuses to run a parent command that has
/// subcommands.
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "MANAGE_ROLES",
    subcommands("ladder", "sync", "whois")
)]
pub async fn leveling(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Create this server's cosmetic level ladder and wire the reward roles up.
///
/// Idempotent: re-running reuses the roles and rewards that already exist, so it is
/// safe to run again after a partial failure or to adopt a server whose ladder was
/// built by hand.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn ladder(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    // Provisioning creates up to 21 roles and then repositions the bot's own role.
    // That is well past Discord's three-second interaction window, so the reply is
    // deferred before any of it starts.
    ctx.defer().await?;

    let report =
        crate::ladder_setup::provision(ctx.data(), ctx.serenity_context(), guild_id).await?;

    let mut embed = serenity::CreateEmbed::new()
        .title("Level ladder")
        .description(report.summary())
        .colour(if report.is_fully_functional() {
            0x2ECC71
        } else {
            0xE67E22
        });

    if let Some(highest) = &report.highest_ungrantable {
        // The one outcome that needs an administrator: Discord only lets a bot
        // assign roles below its own highest role, so a bot role left underneath
        // the ladder means every grant silently fails later. Saying so here is the
        // difference between a working ladder and a decorative one.
        embed = embed.field(
            "Action needed",
            format!(
                "I cannot assign reward roles yet: **{highest}** and everything above it sit \
                 higher than my own role. Drag my **sudobot** role above the ladder in \
                 Server Settings → Roles, then run `/leveling sync`."
            ),
            false,
        );
    }

    embed = embed.field("Ladder", render_ladder(), false);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Grant every member the ladder roles their current level already earned.
///
/// The leveling engine grants roles when a member *levels up*, so a rung added
/// after they passed it is never granted to them. This is the catch-up pass, and it
/// is the only thing that ever hands out the level-0 rung: nobody levels up *into*
/// level 0, so without a sync `Neophyte` would be a role no member ever wears.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn sync(
    ctx: Context<'_>,
    #[description = "How many members to examine (max 5000)"] limit: Option<u64>,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    // Bounded so the command cannot walk an enormous guild one API call at a time
    // inside an interaction that Discord will time out. 5000 covers well past
    // Discord's own practical member count for a community server.
    let limit = limit.unwrap_or(1_000).min(5_000) as usize;

    let (examined, grants, revocations) =
        crate::ladder_setup::sync_members(ctx.data(), guild_id, limit).await?;

    // Both halves are reported: in the replacing configuration most of the work a
    // reconcile does is taking superseded rungs off, and a summary that counted only
    // grants would make a successful run look like it found nothing to do.
    let embed = serenity::CreateEmbed::new()
        .title("Level roles reconciled")
        .description(format!(
            "Examined **{examined}** members, granted **{grants}** reward roles and removed **{revocations}** superseded ones."
        ))
        .footer(serenity::CreateEmbedFooter::new(
            "A member with no XP profile yet is not examined.",
        ))
        .colour(0x2ECC71);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Show a member's stored level, XP and the title it earns them.
///
/// Reads the database rather than Discord: the stored level is what the reward
/// decisions are made from, so this is the number that has to agree with the roles
/// the member is wearing.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn whois(
    ctx: Context<'_>,
    #[description = "Whose standing to look up"] user: serenity::User,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let profile = ctx
        .data()
        .leveling
        .find_profile(guild_id.get(), user.id.get())
        .await?;

    let Some(profile) = profile else {
        ctx.say(format!(
            "<@{}> has no XP profile in this server yet.",
            user.id
        ))
        .await?;
        return Ok(());
    };

    let config = ctx.data().leveling.find_config(guild_id.get()).await?;
    let rewards = ctx.data().leveling.list_rewards(guild_id.get()).await?;

    // The reward rows that exist, so an admin can see the difference between "the
    // ladder says level 10" and "this guild has a reward wired to level 10".
    let rung = rung_for_level(profile.level);
    let configured = rung
        .and_then(|rung| {
            rewards
                .iter()
                .find(|reward| reward.level == rung.level)
                .map(|reward| (rung, reward))
        })
        .map(|(rung, reward)| format!("<@&{}> ({})", reward.role_id, rung.name))
        .unwrap_or_else(|| "no reward role configured for this level".to_string());

    let embed = serenity::CreateEmbed::new()
        .title(user.name.clone())
        .field("Level", profile.level.to_string(), true)
        .field(
            "Rank title",
            crate::ladder_setup::title_for(profile.level),
            true,
        )
        .field("Messages", profile.message_count.to_string(), true)
        .field(
            "XP in level",
            format!(
                "{} / {}",
                profile.xp,
                config.curve.xp_for_level(profile.level)
            ),
            true,
        )
        .field("Total XP", profile.total_xp.to_string(), true)
        .field(
            "Voice minutes",
            (profile.voice_seconds / 60).to_string(),
            true,
        )
        .field("Reward role", configured, false)
        .colour(0x9B59B6);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// The ladder as text, for the `/leveling ladder` reply and for tests.
///
/// Kept as a function so the rendering has one implementation rather than being
/// assembled inline where it cannot be checked.
pub fn render_ladder() -> String {
    descending()
        .map(|rung| format!("`{:>4}`  {}", rung.level, rung.name))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use iam::domain::leveling::LADDER;

    #[test]
    fn the_rendered_ladder_reads_top_down() {
        let rendered = render_ladder();
        let lines: Vec<&str> = rendered.lines().collect();

        assert_eq!(lines.len(), LADDER.len());
        assert!(lines[0].contains("10000"), "the top rung comes first");
        assert!(lines[0].contains("IMMORTAL"));
        assert!(lines[lines.len() - 1].contains("Neophyte"));
    }

    #[test]
    fn every_rung_is_rendered_exactly_once() {
        let rendered = render_ladder();
        for rung in LADDER.iter() {
            assert_eq!(
                rendered.matches(rung.name).count(),
                1,
                "{} appears once",
                rung.name
            );
        }
    }

    #[test]
    fn the_ladder_size_matches_the_specification() {
        assert_eq!(LADDER.len(), 21);
    }
}
