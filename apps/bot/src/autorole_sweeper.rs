//! The delayed-autorole sweeper.
//!
//! Runs on an interval rather than sleeping per grant. A scheduled grant is a row
//! in `auto_role_pending`, so a bot restart resumes the queue instead of losing it —
//! which is the entire point of a delayed autorole as an anti-raid measure: the
//! member who joins, grabs the role and leaves again must not keep it.

use std::sync::Arc;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use tracing::{debug, info, warn};

use iam::application::autorole::SweepPendingGrantsUseCase;

use crate::data::Data;

/// How often the queue is checked.
///
/// A minute is well below the shortest meaningful delay (people configure hours),
/// and the query is one indexed range scan, so the cost is negligible.
const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

/// How many grants one pass will attempt.
///
/// Bounded so a guild that configures a delay for thousands of members does not
/// produce one enormous burst of API calls.
const BATCH_SIZE: u32 = 50;

/// Starts the sweep loop as a background task.
pub fn spawn(data: Arc<Data>) {
    tokio::spawn(async move {
        info!(
            interval_secs = SWEEP_INTERVAL.as_secs(),
            "autorole sweeper started"
        );

        let mut ticker = tokio::time::interval(SWEEP_INTERVAL);
        // Skip missed ticks rather than firing them back to back after a stall: a
        // burst of catch-up sweeps would hammer the API for no benefit, since the
        // pending rows are still there.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;

            if let Err(error) = sweep_once(&data).await {
                // The loop must survive a transient database failure; logging and
                // waiting for the next tick is the whole recovery strategy.
                warn!(%error, "autorole sweep failed; will retry on the next tick");
            }
        }
    });
}

/// One pass over the due grants.
async fn sweep_once(data: &Data) -> Result<(), crate::Error> {
    let sweep = SweepPendingGrantsUseCase::new(data.auto_roles.clone(), data.clock);
    let grants = sweep.execute(BATCH_SIZE).await?;

    if grants.is_empty() {
        debug!("no automatic roles are due");
        return Ok(());
    }

    debug!(count = grants.len(), "applying due automatic roles");

    for grant in grants {
        // The http client extracted per grant: the serenity context is not passed
        // into this task, and building the client is cheap (the underlying reqwest
        // pool is shared).
        let http = data.http();
        let guild_id = serenity::GuildId::new(grant.guild_id);
        let role_id = serenity::RoleId::new(grant.role_id);
        let user_id = serenity::UserId::new(grant.discord_user_id);

        // The member is fetched before the grant because that is the only path
        // serenity exposes for adding a role, and because it answers the question
        // that matters: did they stay long enough to deserve it?
        let member = match guild_id.member(&http, user_id).await {
            Ok(member) => member,
            Err(error) => {
                // "Unknown member" means they left during the delay — the exact
                // behaviour a delayed autorole exists to catch, so the grant is
                // completed (removed) rather than retried. Any other error is a
                // real failure and goes through the backoff path.
                if is_unknown_member(&error) {
                    debug!(
                        guild_id = grant.guild_id,
                        user_id = grant.discord_user_id,
                        "member left before their delayed role was due; discarding the grant"
                    );
                    sweep.complete(grant.id).await?;
                    continue;
                }

                let dropped = sweep.fail(&grant, &error.to_string()).await?;
                warn!(guild_id = grant.guild_id, %error, dropped, "could not read a member for a grant");
                continue;
            }
        };

        match member.add_role(&http, role_id).await {
            Ok(()) => {
                sweep.complete(grant.id).await?;
                debug!(
                    guild_id = grant.guild_id,
                    user_id = grant.discord_user_id,
                    role_id = grant.role_id,
                    "granted a delayed automatic role"
                );
            }
            Err(error) => {
                // `fail` reschedules with backoff and reports whether the grant was
                // dropped after exhausting its attempts, so a permanently stuck row
                // is logged once instead of every minute forever.
                let dropped = sweep.fail(&grant, &error.to_string()).await?;

                if dropped {
                    warn!(
                        guild_id = grant.guild_id,
                        user_id = grant.discord_user_id,
                        role_id = grant.role_id,
                        %error,
                        "giving up on an automatic role after repeated failures"
                    );
                } else {
                    warn!(
                        guild_id = grant.guild_id,
                        role_id = grant.role_id,
                        %error,
                        "an automatic role grant failed; it will be retried"
                    );
                }
            }
        }
    }

    Ok(())
}

/// Whether a serenity error means "that member is not in this guild".
///
/// Matched on the message rather than a typed variant because serenity surfaces
/// Discord's JSON error code as a string here; the alternative would be depending
/// on an internal enum that is not public.
fn is_unknown_member(error: &serenity::Error) -> bool {
    let rendered = error.to_string();
    rendered.contains("Unknown Member") || rendered.contains("10007")
}
