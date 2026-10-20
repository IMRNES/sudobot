//! The Discord bot process.
//!
//! A separate binary from the API on purpose: the bot holds a gateway connection
//! and must keep running while the API is redeployed, and a panic in a slash
//! command must not take the dashboard down with it. The two processes share one
//! PostgreSQL database, and that is the whole integration surface between them.

mod autorole_sweeper;
mod bootstrap_guild;
mod commands;
mod data;
mod database;
mod events;
mod jail_discord;
mod jail_sweeper;
mod ladder_icons;
mod ladder_setup;
mod level_rewards;
mod voice_tracker;

use anyhow::Result;
// anyhow's `Context` trait is what gives `.context(..)` on a `Result`; aliased so it
// does not collide with the command `Context<'a>` defined below.
use anyhow::Context as _;
use poise::serenity_prelude as serenity;
use std::sync::Arc;

/// The error type every command returns.
///
/// This exact alias is what poise's bounds expect, and it lets `?` work across
/// serenity, SeaORM and the domain's own error types.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// The per-command context, aliased because it appears in every signature.
pub type Context<'a> = poise::Context<'a, data::Data, Error>;

#[tokio::main]
async fn main() -> Result<()> {
    let config = sudobot_config::load_or_exit();
    config.init_tracing()?;
    config.discord.token();

    tracing::info!(
        environment = config.environment.as_str(),
        dev_guild = ?config.discord.dev_guild_id,
        "starting the bot"
    );

    let db = database::connect(&config.database).await?;
    let data = Arc::new(data::Data::new(
        db,
        config.discord.dev_guild_id,
        config.discord.provision_guild_id,
        config.discord.token(),
    ));

    // GUILD_MEMBERS and MESSAGE_CONTENT are privileged: they must also be enabled
    // in the Developer Portal, or the gateway closes with a 4014 and the bot looks
    // like it started and immediately died.
    //
    // GUILD_VOICE_STATES is what the leveling engine's voice half sits on: without
    // it Discord never sends a voice state update, and voice XP silently stays at
    // zero while the command that reports it still answers.
    let intents = serenity::GatewayIntents::GUILD_MESSAGES
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MEMBERS
        | serenity::GatewayIntents::GUILD_VOICE_STATES;

    let setup_data = Arc::clone(&data);
    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: commands::all(),
            // The single dispatch path for every gateway event. `events::handle`
            // owns joins, leaves, button presses, messages and voice states, and it
            // receives the same `Arc<Data>` the commands do through
            // `FrameworkContext::user_data`.
            //
            // Nothing else is registered as an event handler on the client: poise's
            // framework *and* serenity's handler list are both driven for each
            // event, so registering `Data` as a handler as well made every join run
            // twice and post the welcome message twice.
            event_handler: |framework_ctx, event| Box::pin(events::handle(framework_ctx, event)),
            on_error: |error| {
                Box::pin(async move {
                    // "Unknown command" and permission refusals are ordinary
                    // traffic, so this reports rather than panics.
                    if let Err(report) = poise::builtins::on_error(error).await {
                        tracing::error!(error = %report, "failed to report a command error");
                    }
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, ready, framework| {
            let data = Arc::clone(&setup_data);
            Box::pin(async move {
                tracing::info!(bot = %ready.user.name, id = %ready.user.id, "connected to discord");

                // Global registration can take an hour to propagate, which makes
                // iteration painful; a dev guild id switches to instant,
                // guild-scoped registration.
                match data.dev_guild_id {
                    Some(guild_id) => {
                        poise::builtins::register_in_guild(
                            ctx,
                            &framework.options().commands,
                            serenity::GuildId::new(guild_id),
                        )
                        .await?;
                        tracing::info!(guild_id, "slash commands registered to the dev guild");
                    }
                    None => {
                        poise::builtins::register_globally(ctx, &framework.options().commands)
                            .await?;
                        tracing::info!("slash commands registered globally");
                    }
                }

                autorole_sweeper::spawn(Arc::clone(&data));
                jail_sweeper::spawn(Arc::clone(&data));

                // Repair the jail's overwrites once, at startup. A channel created
                // while the bot was down, or a hand-edited overwrite, would otherwise
                // stay wrong until somebody re-saved the form.
                if let Err(error) = jail_sweeper::reseal_all(&data).await {
                    tracing::warn!(%error, "re-sealing the jail at startup failed");
                }

                // The ladder is part of the deployment, not a manual step: a fresh
                // host running the leveling engine with no reward roles would
                // compute level-ups and grant nothing. Provisioning here means
                // deploying a commit produces the same guild state every time.
                //
                // Runs after the slash commands register so the run is announced
                // before it starts doing work, and best-effort: a guild the bot has
                // been removed from, or a missing `Manage Roles`, is reported and
                // the bot keeps serving. A ladder that failed to build is a
                // degraded feature, not a reason to refuse to start.
                if let Some(guild_id) = data.provision_guild_id() {
                    let guild_id = serenity::GuildId::new(guild_id);
                    match ladder_setup::provision(&data, ctx, guild_id).await {
                        Ok(report) => {
                            tracing::info!(
                                guild_id = guild_id.get(),
                                summary = %report.summary(),
                                fully_functional = report.is_fully_functional(),
                                "level ladder provisioned"
                            );

                            if let Some(highest) = &report.highest_ungrantable {
                                tracing::warn!(
                                    guild_id = guild_id.get(),
                                    rung = %highest,
                                    "reward roles above the bot's own role cannot be assigned; \
                                     raise the bot role above the ladder in Server Settings"
                                );
                            }

                            // A rung added after members passed it is never granted by
                            // the level-change path, so every deploy reconciles. Level
                            // 0 depends on this: nobody levels up *into* level 0.
                            match ladder_setup::sync_members(&data, guild_id, 5_000).await {
                                Ok((examined, grants, revocations)) => tracing::info!(
                                    guild_id = guild_id.get(),
                                    examined,
                                    grants,
                                    revocations,
                                    "level reward roles reconciled"
                                ),
                                Err(error) => {
                                    tracing::warn!(%error, "could not reconcile level reward roles")
                                }
                            }
                        }
                        Err(error) => {
                            tracing::warn!(%error, "could not provision the level ladder")
                        }
                    }
                }

                Ok((*data).clone())
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(config.discord.token(), intents)
        .framework(framework)
        .await
        .context("building the discord client")?;

    client.start().await.context("running the discord client")
}
