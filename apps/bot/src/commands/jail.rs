//! `/jail` — the prisoner role, its sentences, and the ways out.
//!
//! Gated on `Manage Roles` for everything except `bail`, because a jail is a
//! moderation action: whoever can move a member out of every channel is trusted to
//! do it deliberately.
//!
//! `bail` is deliberately *not* gated. It is the prisoner's own route out, and
//! requiring a permission for it would make it reachable only by the people who do
//! not need it.

use poise::serenity_prelude as serenity;

use iam::application::jail::{
    BailQuote, JailMemberCommand, JailMemberUseCase, QuoteBailUseCase, UpdateJailConfigCommand,
    UpdateJailConfigUseCase, can_afford,
};
// The traits are not inherent methods on the SeaORM types, so a call site needs the
// trait in scope — otherwise the error points at the call, not the missing import.
use iam::application::ports::Clock as _;
use iam::domain::economy::{EconomyRepository as _, NewTransaction, TransactionReason};
use iam::domain::jail::{Imprisonment, JailRepository};

use crate::{Context, Error};

/// Where a prisoner is told they are, and when they will get out.
fn sentence_line(sentence: &Imprisonment, now: chrono::DateTime<chrono::Utc>) -> String {
    match sentence.remaining_seconds(now) {
        None => "for **an indefinite sentence** — a moderator has to release you".to_string(),
        Some(seconds) => {
            let minutes = seconds / 60;
            if minutes >= 60 {
                format!("for **{}h {}m**", minutes / 60, minutes % 60)
            } else if minutes >= 1 {
                format!("for **{minutes}m**")
            } else {
                format!("for **{seconds}s**")
            }
        }
    }
}

/// Administer the jail: configure it, sentence members, release them, inspect it.
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "MANAGE_ROLES",
    subcommands("setup", "add", "release", "list", "bail")
)]
pub async fn jail(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// Point the jail at a role and a channel.
///
/// Both are stored rather than inferred: a server can call its jail role anything,
/// and the cell is whichever channel an admin names.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn setup(
    ctx: Context<'_>,
    #[description = "Role that denies every channel"] role: serenity::Role,
    #[description = "The only channel a prisoner can still see"] channel: serenity::GuildChannel,
    #[description = "Default sentence in hours, if /jail add omits one"] default_hours: Option<u32>,
    #[description = "Price a prisoner may pay to leave early (0 disables bail)"] bail_price: Option<
        i64,
    >,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    // Capture the outgoing role before the write. If the admin points the jail at a
    // different role, the old one keeps its overwrite on every channel — a dead role
    // granting `View Channel` in a permission list nobody looks at again.
    let previous_role_id = ctx
        .data()
        .jail
        .find_config(guild_id.get())
        .await?
        .jail_role_id;

    // Applying overwrites to every channel takes longer than Discord's three-second
    // window, so the deferral happens before any of it starts.
    let use_case = UpdateJailConfigUseCase::new(ctx.data().jail.clone());
    let config = use_case
        .execute(UpdateJailConfigCommand {
            guild_id: guild_id.get(),
            jail_role_id: Some(Some(role.id.get())),
            jail_channel_id: Some(Some(channel.id.get())),
            default_sentence_seconds: default_hours
                .map(|hours| Some(i64::from(hours) * 3600))
                .or(Some(None)),
            bail_amount: bail_price.map(|price| if price > 0 { Some(price) } else { None }),
        })
        .await?;

    let http = ctx.serenity_context().http.clone();

    // Clear first, then apply. Order matters when the two roles are the same:
    // clearing after applying would delete the overwrites just written.
    if let Some(previous) = previous_role_id.filter(|previous| *previous != role.id.get()) {
        crate::jail_discord::clear_jail_permissions(
            &http,
            guild_id,
            serenity::RoleId::new(previous),
        )
        .await?;
    }

    let report =
        crate::jail_discord::apply_jail_permissions(&http, guild_id, role.id, Some(channel.id))
            .await?;

    let mut embed = serenity::CreateEmbed::new()
        .title("Jail configured")
        .colour(if report.is_clean() {
            0x2ECC71
        } else {
            0xE67E22
        })
        .field("Role", format!("<@&{}>", role.id), true)
        .field("Cell", format!("<#{}>", channel.id), true)
        .field(
            "Default sentence",
            config
                .default_sentence_seconds
                .map(|seconds| format!("{}h", seconds / 3600))
                .unwrap_or_else(|| "indefinite".to_string()),
            true,
        )
        .field(
            "Bail",
            config
                .bail_amount
                .map(|amount| format!("{amount} credits"))
                .unwrap_or_else(|| "not offered".to_string()),
            true,
        )
        .field(
            "Channels sealed",
            format!(
                "{} written, {} failed",
                report.channels_updated, report.channels_failed
            ),
            false,
        );

    if !report.is_clean() {
        embed = embed.field(
            "Action needed",
            format!(
                "{} channels refused the overwrite. My role must sit above <@&{}> and \
                 must hold **Manage Channels**.",
                report.channels_failed, role.id
            ),
            false,
        );
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Put a member in the cell.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn add(
    ctx: Context<'_>,
    #[description = "Who to jail"] member: serenity::Member,
    #[description = "Why, shown to them"] reason: Option<String>,
    #[description = "How long, in minutes; omit for the default"] minutes: Option<u32>,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let moderator = ctx.author().id;
    ctx.defer().await?;

    let use_case = JailMemberUseCase::new(ctx.data().jail.clone(), ctx.data().clock);
    let outcome = use_case
        .execute(JailMemberCommand {
            guild_id: guild_id.get(),
            discord_user_id: member.user.id.get(),
            jailed_by: moderator.get(),
            reason: reason.unwrap_or_default(),
            duration_seconds: minutes.map(|minutes| i64::from(minutes) * 60),
        })
        .await?;

    let http = ctx.serenity_context().http.clone();
    let jail_role_id = outcome
        .config
        .jail_role_id
        .ok_or("the jail has no role configured")?;

    crate::jail_discord::jail_member(
        &http,
        guild_id,
        member.user.id,
        serenity::RoleId::new(jail_role_id),
        outcome.config.jail_channel_id.map(serenity::ChannelId::new),
    )
    .await?;

    let now = ctx.data().clock.now();
    let sentence = sentence_line(&outcome.imprisonment, now);

    // Tell them in the cell, where they can still read it. Best effort: a failed
    // notification must not undo the jailing.
    if let Some(channel_id) = outcome.config.jail_channel_id {
        let _ = serenity::ChannelId::new(channel_id)
            .say(
                &http,
                format!(
                    "<@{}> you have been placed in jail {sentence}.\n**Reason:** {}",
                    member.user.id.get(),
                    outcome.imprisonment.reason
                ),
            )
            .await;
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Member jailed")
                .colour(0xE74C3C)
                .field("Member", format!("<@{}>", member.user.id), true)
                .field("Duration", sentence, true)
                .field("Reason", outcome.imprisonment.reason, false),
        ),
    )
    .await?;

    Ok(())
}

/// Let a member out early.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn release(
    ctx: Context<'_>,
    #[description = "Who to release"] member: serenity::Member,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let http = ctx.serenity_context().http.clone();
    crate::jail_sweeper::release_by_hand(&ctx.data().clone(), &http, guild_id, member.user.id)
        .await?;

    ctx.send(
        poise::CreateReply::default().content(format!("Released <@{}>.", member.user.id.get())),
    )
    .await?;

    Ok(())
}

/// Everyone currently in the cell.
#[poise::command(slash_command, guild_only, default_member_permissions = "MANAGE_ROLES")]
pub async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let sentences = ctx.data().jail.list_for_guild(guild_id.get()).await?;
    if sentences.is_empty() {
        ctx.send(poise::CreateReply::default().content("The jail is empty.".to_string()))
            .await?;
        return Ok(());
    }

    let now = ctx.data().clock.now();
    let mut embed = serenity::CreateEmbed::new()
        .title("Current sentences")
        .colour(0xE67E22)
        .description(
            sentences
                .iter()
                .map(|sentence| {
                    format!(
                        "<@{}> — {} · {}",
                        sentence.discord_user_id,
                        sentence_line(sentence, now),
                        sentence.reason
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );

    // A description is capped at 4096 characters; 25 sentences of this shape is well
    // inside it, and saying so beats Discord rejecting the whole embed.
    if sentences.len() > 25 {
        embed = embed.footer(serenity::CreateEmbedFooter::new(format!(
            "{} sentences; showing the newest 25",
            sentences.len()
        )));
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Buy your way out of jail.
///
/// The one jail subcommand a prisoner can run, and the reason the sentence is a
/// number rather than a sentence-less role: there is a second way out that does not
/// need a moderator to be awake.
#[poise::command(slash_command, guild_only)]
pub async fn bail(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let user_id = ctx.author().id;
    ctx.defer().await?;

    let quote_use_case = QuoteBailUseCase::new(ctx.data().jail.clone(), ctx.data().clock);
    let quote: Option<BailQuote> = quote_use_case
        .execute(guild_id.get(), user_id.get())
        .await?;

    let Some(quote) = quote else {
        ctx.send(
            poise::CreateReply::default().content(
                "There is no bail to pay: either this server does not offer it, or your \
                 sentence has already ended."
                    .to_string(),
            ),
        )
        .await?;
        return Ok(());
    };

    let account = ctx
        .data()
        .economy
        .find_account(guild_id.get(), user_id.get())
        .await?
        .ok_or("You do not have an account in this server yet.")?;

    if !can_afford(account.balance, quote.amount) {
        ctx.send(poise::CreateReply::default().content(format!(
            "Bail is **{} credits** and you have **{}**. Not enough yet.",
            quote.amount, account.balance
        )))
        .await?;
        return Ok(());
    }

    // Charge first, release second. A failed release after a successful charge is
    // recoverable by a moderator; a release after a failed charge is free jailbreak.
    let transaction = NewTransaction {
        id: uuid::Uuid::new_v4(),
        guild_id: guild_id.get(),
        discord_user_id: user_id.get(),
        amount: -quote.amount,
        // `Purchase`, not `AdminDeduct`: bail is a payment for a thing, and only an
        // explicit admin deduction is allowed to overdraw. Using the admin path here
        // would let a prisoner with 3 credits buy a 500-credit bail and walk out with
        // a negative balance.
        reason: TransactionReason::Purchase,
        actor_discord_id: Some(user_id.get()),
        note: Some("paid bail to leave jail".to_string()),
        // A prisoner double-clicking the command must not be charged twice.
        idempotency_key: Some(format!(
            "bail:{}:{}:{}",
            guild_id.get(),
            user_id.get(),
            ctx.id()
        )),
    };

    ctx.data().economy.apply_transaction(transaction).await?;

    let http = ctx.serenity_context().http.clone();
    crate::jail_sweeper::release_by_hand(&ctx.data().clone(), &http, guild_id, user_id).await?;

    ctx.send(poise::CreateReply::default().content(format!(
        "You paid **{} credits** in bail and are free to go.",
        quote.amount
    )))
    .await?;

    Ok(())
}
