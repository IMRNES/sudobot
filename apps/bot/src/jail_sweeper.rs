//! The sentence sweeper.
//!
//! Runs on an interval rather than sleeping per sentence, for the same reason the
//! autorole sweeper does: a sentence is a row, so a restart resumes it instead of
//! losing it. A member jailed for an hour must still be released if the bot was
//! restarted twice in between.
//!
//! Two jobs each pass:
//!
//! 1. **Release the expired.** Undo the role first, delete the row second. If the
//!    Discord call fails the row survives and the next pass retries, so a Discord
//!    outage never silently frees somebody the database thinks is still inside.
//! 2. **Reconcile the live.** A moderator can take the jail role off by hand, and a
//!    member can leave and rejoin. Both leave a row that no longer matches reality,
//!    so each pass re-applies the role to anyone missing it and drops rows for
//!    members who are no longer in the guild.

use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use tracing::{debug, info, warn};

use iam::application::jail::SweepExpiredSentencesUseCase;
use iam::application::ports::Clock;
use iam::domain::guild::GuildRepository as _;
use iam::domain::jail::JailRepository;

use crate::data::Data;
use crate::jail_discord;

/// How often sentences are checked.
///
/// The shortest sentence a moderator can set is one second, so the sweep is a
/// backstop rather than a precise timer: a member is released within this window of
/// their deadline. A minute is short enough that nobody notices and long enough that
/// the query is negligible.
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// Sentences per pass. Bounded so a mass-jailing does not produce one burst of API
/// calls; the rest wait for the next tick.
const BATCH_SIZE: u32 = 50;

/// Starts the sweep loop as a background task.
pub fn spawn(data: Arc<Data>) {
    tokio::spawn(async move {
        info!(
            interval_secs = SWEEP_INTERVAL.as_secs(),
            "jail sweeper started"
        );

        let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            if let Err(error) = sweep_once(&data).await {
                // The loop must survive a transient database failure. Logging and
                // waiting for the next tick is the whole recovery strategy.
                warn!(%error, "jail sweep failed; will retry on the next tick");
            }
        }
    });
}

/// One pass: release the expired, then reconcile the live.
async fn sweep_once(data: &Data) -> Result<(), crate::Error> {
    release_expired(data).await?;
    reconcile_live(data).await
}

/// Re-seals every configured jail.
///
/// Not part of the minute sweep: it is one Discord call per channel per guild, which
/// is far too much to repeat every tick. It runs once at startup instead, so a
/// channel created while the bot was down — or a role whose overwrites were edited by
/// hand in the Discord UI — is repaired by a restart rather than surviving until
/// somebody happens to save the form.
pub async fn reseal_all(data: &Data) -> Result<(), crate::Error> {
    let guilds = data.guilds.list_active().await?;
    let http = data.http();

    for guild in guilds {
        let config = data.jail.find_config(guild.id).await?;
        let Some(role_id) = config.jail_role_id else {
            continue;
        };

        let guild_id = serenity::GuildId::new(guild.id);

        match jail_discord::apply_jail_permissions(
            &http,
            guild_id,
            serenity::RoleId::new(role_id),
            config.jail_channel_id.map(serenity::ChannelId::new),
        )
        .await
        {
            Ok(report) => info!(
                guild_id = guild.id,
                written = report.channels_updated,
                failed = report.channels_failed,
                "re-sealed the jail at startup"
            ),
            // Best effort: a guild whose bot role sits too low is a configuration
            // problem, not a reason to refuse to start.
            Err(error) => warn!(
                guild_id = guild.id,
                %error,
                "could not re-seal the jail at startup"
            ),
        }
    }

    Ok(())
}

/// Releases every sentence whose deadline has passed.
async fn release_expired(data: &Data) -> Result<(), crate::Error> {
    let sweep = SweepExpiredSentencesUseCase::new(data.jail.clone(), data.clock);
    let due = sweep.due(BATCH_SIZE).await?;

    if due.is_empty() {
        debug!("no sentences are due for release");
        return Ok(());
    }

    debug!(count = due.len(), "releasing expired sentences");

    for sentence in due {
        let guild_id = serenity::GuildId::new(sentence.imprisonment.guild_id);
        let user_id = serenity::UserId::new(sentence.imprisonment.discord_user_id);

        // No role id means the guild's jail was un-configured while somebody was
        // still inside. The row is dropped without a Discord call: there is no role
        // to remove, and leaving the row would make the sweeper retry forever.
        let Some(role_id) = sentence.jail_role_id else {
            warn!(
                guild_id = guild_id.get(),
                user_id = user_id.get(),
                "dropping a sentence for a guild whose jail is no longer configured"
            );
            sweep.complete(guild_id.get(), user_id.get()).await?;
            continue;
        };

        let http = data.http();
        match jail_discord::release_member(&http, guild_id, user_id, serenity::RoleId::new(role_id))
            .await
        {
            Ok(()) => {
                sweep.complete(guild_id.get(), user_id.get()).await?;
                info!(
                    guild_id = guild_id.get(),
                    user_id = user_id.get(),
                    "sentence served; member released"
                );

                // Tell them, in the cell they can still see. Best effort: the release
                // must not be undone by a failed courtesy message.
                if let Some(config) = jail_config(data, guild_id).await
                    && let Some(channel_id) = config.jail_channel_id
                {
                    let _ = serenity::ChannelId::new(channel_id)
                        .say(
                            &http,
                            format!(
                                "<@{}> your sentence has been served. Welcome back.",
                                user_id.get()
                            ),
                        )
                        .await;
                }
            }
            Err(error) => {
                // The row stays, so the next pass retries. Losing the release would
                // be worse than a duplicate attempt.
                warn!(
                    guild_id = guild_id.get(),
                    user_id = user_id.get(),
                    %error,
                    "could not release a member whose sentence expired; will retry"
                );
            }
        }
    }

    Ok(())
}

/// Re-applies the jail role to anyone who should have it, and drops stale rows.
///
/// This is what makes "leave and rejoin" not an escape route: Discord hands the bot
/// the join, but the member may rejoin before or after any event the bot sees, so
/// the sweeper is what actually guarantees the role is back on.
async fn reconcile_live(data: &Data) -> Result<(), crate::Error> {
    let guilds = data.guilds.list_active().await?;

    for guild in guilds {
        let config = data.jail.find_config(guild.id).await?;
        let Some(role_id) = config.jail_role_id else {
            continue;
        };

        let sentences = data.jail.list_for_guild(guild.id).await?;
        if sentences.is_empty() {
            continue;
        }

        let guild_id = serenity::GuildId::new(guild.id);
        let role = serenity::RoleId::new(role_id);
        let http = data.http();

        for sentence in sentences {
            let user_id = serenity::UserId::new(sentence.discord_user_id);

            match jail_discord::wears_jail_role(&http, guild_id, user_id, role).await {
                Ok(true) => {}
                Ok(false) => {
                    // Either the role was removed by hand, or they left and came
                    // back. Re-applying is correct in both cases: the sentence is
                    // still live, so the role belongs on them.
                    match jail_discord::jail_member(
                        &http,
                        guild_id,
                        user_id,
                        role,
                        config.jail_channel_id.map(serenity::ChannelId::new),
                    )
                    .await
                    {
                        Ok(_) => info!(
                            guild_id = guild.id,
                            user_id = sentence.discord_user_id,
                            "re-applied the jail role to a member whose sentence is still live"
                        ),
                        Err(error) => warn!(
                            guild_id = guild.id,
                            user_id = sentence.discord_user_id,
                            %error,
                            "could not re-apply the jail role"
                        ),
                    }
                }
                Err(error) => {
                    warn!(
                        guild_id = guild.id,
                        user_id = sentence.discord_user_id,
                        %error,
                        "could not read a member while reconciling the jail"
                    );
                }
            }
        }
    }

    Ok(())
}

/// Releases a member now, by a moderator's hand or by paying bail.
///
/// Shared by `/jail release` and `/jail bail` because the two differ only in who
/// asked and whether money moved: the Discord work and the row deletion are
/// identical, and a divergence between them is how a paid release ends up leaving the
/// role on.
pub async fn release_by_hand(
    data: &Data,
    http: &serenity::Http,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
) -> Result<(), crate::Error> {
    let config = data.jail.find_config(guild_id.get()).await?;

    // No configured role means there is nothing to take off, but the row still has to
    // go — otherwise the member stays "sentenced" forever with no way to observe it.
    if let Some(role_id) = config.jail_role_id {
        jail_discord::release_member(http, guild_id, user_id, serenity::RoleId::new(role_id))
            .await?;
    }

    data.jail.delete(guild_id.get(), user_id.get()).await?;

    info!(
        guild_id = guild_id.get(),
        user_id = user_id.get(),
        "member released by hand"
    );

    Ok(())
}

/// The guild's jail config, or `None` if it cannot be read.
async fn jail_config(
    data: &Data,
    guild_id: serenity::GuildId,
) -> Option<iam::domain::jail::JailConfig> {
    match data.jail.find_config(guild_id.get()).await {
        Ok(config) => Some(config),
        Err(error) => {
            warn!(guild_id = guild_id.get(), %error, "could not read the jail config");
            None
        }
    }
}

/// Exposed for the join handler: re-jail a returning member immediately rather than
/// waiting up to a minute for the next sweep.
pub async fn rejail_on_rejoin(
    data: &Data,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
) -> Result<bool, crate::Error> {
    let Some(sentence) = data.jail.find(guild_id.get(), user_id.get()).await? else {
        return Ok(false);
    };

    // A sentence that has already run out is not re-applied; the sweeper will drop
    // the row on its next pass.
    let now = data.clock.now();
    if sentence.is_expired_at(now) {
        return Ok(false);
    }

    let config = data.jail.find_config(guild_id.get()).await?;
    let Some(role_id) = config.jail_role_id else {
        return Ok(false);
    };

    let http = data.http();
    jail_discord::jail_member(
        &http,
        guild_id,
        user_id,
        serenity::RoleId::new(role_id),
        config.jail_channel_id.map(serenity::ChannelId::new),
    )
    .await?;

    info!(
        guild_id = guild_id.get(),
        user_id = user_id.get(),
        "re-jailed a member who rejoined mid-sentence"
    );

    Ok(true)
}
