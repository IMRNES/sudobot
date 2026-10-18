use poise::serenity_prelude as serenity;

use iam::application::economy::{
    ClaimDailyCommand, ClaimDailyUseCase, LeaderboardEntry, PurchaseItemCommand,
    PurchaseItemUseCase, ShopItemView, ShopListingUseCase, TransferCommand, TransferUseCase,
};
use iam::application::ports::Clock;
use iam::domain::economy::{EconomyError, EconomyRepository};

use crate::{Context, Error};

/// Currency: check a wallet, claim the daily, pay someone, browse the shop.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("balance", "daily", "pay", "shop", "buy", "richest")
)]
pub async fn economy(_ctx: Context<'_>) -> Result<(), Error> {
    // Never invoked: Discord refuses to run a parent command that has subcommands,
    // but poise requires the parent function to exist.
    Ok(())
}

/// Show a member's wallet.
#[poise::command(slash_command, guild_only)]
pub async fn balance(
    ctx: Context<'_>,
    #[description = "Whose balance to show"] user: Option<serenity::User>,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let target = user.unwrap_or_else(|| ctx.author().clone());

    let account = ctx
        .data()
        .economy
        .ensure_account(guild_id.get(), target.id.get())
        .await?;

    let embed = serenity::CreateEmbed::new()
        .title(format!("{}'s wallet", target.name))
        .field("Balance", format!("{} coins", account.balance), true)
        .field("Earned", account.total_earned.to_string(), true)
        .field("Spent", account.total_spent.to_string(), true)
        .colour(0xF1C40F);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Claim today's reward.
#[poise::command(slash_command, guild_only)]
pub async fn daily(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    let user_id = ctx.author().id.get();
    ctx.defer().await?;

    let use_case = ClaimDailyUseCase::new(ctx.data().economy.clone(), ctx.data().clock);
    let outcome = use_case
        .execute(ClaimDailyCommand {
            guild_id: guild_id.get(),
            discord_user_id: user_id,
            // The interaction id makes the claim idempotent: a retried or
            // double-clicked command cannot pay twice.
            idempotency_key: Some(format!("daily:{}:{}", guild_id.get(), ctx.id())),
        })
        .await;

    match outcome {
        Ok(result) => {
            ctx.say(format!(
                "You claimed **{} coins**. Balance: {}.",
                result.credited, result.account.balance
            ))
            .await?;
        }
        Err(EconomyError::DailyNotReady) => {
            // An expected refusal, so answer with the wait rather than an error.
            let account = ctx
                .data()
                .economy
                .ensure_account(guild_id.get(), user_id)
                .await?;

            let remaining = account
                .daily_remaining(
                    ctx.data().clock.now(),
                    iam::application::economy::DAILY_COOLDOWN_HOURS,
                )
                .map(format_duration)
                .unwrap_or_else(|| "a moment".to_string());

            ctx.send(
                poise::CreateReply::default()
                    .content(format!("Already claimed. Come back in about {remaining}."))
                    .ephemeral(true),
            )
            .await?;
        }
        Err(error) => return Err(error.into()),
    }

    Ok(())
}

/// Send currency to another member.
#[poise::command(slash_command, guild_only)]
pub async fn pay(
    ctx: Context<'_>,
    #[description = "Who to pay"] user: serenity::User,
    #[description = "How much"] amount: i64,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;

    if user.bot {
        ctx.send(
            poise::CreateReply::default()
                .content("Bots do not have wallets.")
                .ephemeral(true),
        )
        .await?;
        return Ok(());
    }

    ctx.defer().await?;

    let use_case = TransferUseCase::new(ctx.data().economy.clone(), ctx.data().clock);
    let result = use_case
        .execute(TransferCommand {
            guild_id: guild_id.get(),
            from_discord_id: ctx.author().id.get(),
            to_discord_id: user.id.get(),
            amount,
            idempotency_key: Some(format!("pay:{}", ctx.id())),
        })
        .await?;

    ctx.say(format!(
        "Sent **{amount} coins** to {}. Your balance: {}.",
        user.name, result.sender.balance
    ))
    .await?;

    Ok(())
}

/// Browse the server's shop.
#[poise::command(slash_command, guild_only)]
pub async fn shop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let use_case = ShopListingUseCase::new(ctx.data().economy.clone());
    let items = use_case.execute(guild_id.get()).await?;

    if items.is_empty() {
        ctx.say("This server's shop is empty.").await?;
        return Ok(());
    }

    let embed = serenity::CreateEmbed::new()
        .title("Shop")
        .description(render_shop(&items))
        .colour(0x2ECC71);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// Buy something from the shop.
#[poise::command(slash_command, guild_only)]
pub async fn buy(
    ctx: Context<'_>,
    #[description = "The item's slug, as shown in /economy shop"]
    #[autocomplete = "autocomplete_item"]
    slug: String,
) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let use_case = PurchaseItemUseCase::new(ctx.data().economy.clone(), ctx.data().clock);
    let outcome = use_case
        .execute(PurchaseItemCommand {
            guild_id: guild_id.get(),
            discord_user_id: ctx.author().id.get(),
            slug,
            // Guards against a double-click buying twice.
            idempotency_key: Some(format!("buy:{}", ctx.id())),
        })
        .await?;

    let mut reply = format!(
        "Bought **{}** for {} coins. Balance: {}.",
        outcome.item.name, outcome.item.price, outcome.account.balance
    );

    // A shop item may represent a Discord role; granting it is the bot's job, and
    // a failure here does not undo the purchase — the ledger has already moved, and
    // an admin can grant it by hand.
    if let Some(role_id) = outcome.item.role_id {
        let role = serenity::RoleId::new(role_id);
        match guild_id.member(ctx.http(), ctx.author().id).await {
            Ok(member) => match member.add_role(ctx.http(), role).await {
                Ok(()) => reply.push_str(&format!("\n<@&{role_id}> is yours.")),
                Err(error) => {
                    tracing::warn!(%error, "the purchase went through but the role could not be granted");
                    reply.push_str(
                        "\nI could not give you the role — an administrator may need to move my role higher.",
                    );
                }
            },
            Err(error) => {
                tracing::warn!(%error, "could not read the buyer's membership");
            }
        }
    }

    ctx.say(reply).await?;
    Ok(())
}

/// The richest members of this server.
///
/// Named `richest` rather than `leaderboard` because the top-level `/leaderboard`
/// is the XP one; two commands cannot share a name, and the XP board is the one
/// members reach for first.
#[poise::command(slash_command, guild_only)]
pub async fn richest(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = super::require_guild(ctx)?;
    ctx.defer().await?;

    let use_case = ShopListingUseCase::new(ctx.data().economy.clone());
    let leaders = use_case.leaderboard(guild_id.get(), 10).await?;

    if leaders.is_empty() {
        ctx.say("Nobody has earned anything yet.").await?;
        return Ok(());
    }

    ctx.say(render_leaderboard(&leaders)).await?;
    Ok(())
}

/// Offers the guild's shop item slugs as the member types.
async fn autocomplete_item(
    ctx: Context<'_>,
    partial: &str,
) -> serenity::CreateAutocompleteResponse {
    let guild_id = match ctx.guild_id() {
        Some(guild_id) => guild_id,
        None => return serenity::CreateAutocompleteResponse::new(),
    };

    let use_case = ShopListingUseCase::new(ctx.data().economy.clone());
    let Ok(items) = use_case.execute(guild_id.get()).await else {
        // A failure to list items must not fail the keystroke; an empty suggestion
        // list degrades to the member typing the slug.
        return serenity::CreateAutocompleteResponse::new();
    };

    let partial = partial.to_lowercase();
    let choices = items
        .into_iter()
        .filter(|item| item.is_active && item.slug.to_lowercase().starts_with(&partial))
        .take(25)
        .map(|item| {
            serenity::AutocompleteChoice::new(
                format!("{} — {} coins", item.name, item.price),
                item.slug,
            )
        })
        .collect();

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Formats the shop as text rather than one embed field per item: a shop with
/// thirty entries would blow past Discord's 25-field limit, and a list reads fine.
fn render_shop(items: &[ShopItemView]) -> String {
    items
        .iter()
        .map(|item| {
            let stock = match item.stock {
                Some(0) => " *(sold out)*".to_string(),
                Some(remaining) => format!(" — {remaining} left"),
                None => String::new(),
            };

            let description = item.description.as_deref().unwrap_or("");

            format!(
                "**{}** — {} coins{stock}\n`{}`{}",
                item.name,
                item.price,
                item.slug,
                if description.is_empty() {
                    String::new()
                } else {
                    format!("\n{description}")
                }
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn render_leaderboard(leaders: &[LeaderboardEntry]) -> String {
    leaders
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            format!(
                "**{}.** <@{}> — {} coins",
                index + 1,
                entry.discord_user_id,
                entry.balance
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Renders a cooldown as something a person would say.
fn format_duration(remaining: chrono::Duration) -> String {
    let minutes = remaining.num_minutes();
    match minutes {
        m if m >= 60 => format!("{}h {}m", m / 60, m % 60),
        m if m > 0 => format!("{m} minutes"),
        _ => "a moment".to_string(),
    }
}
