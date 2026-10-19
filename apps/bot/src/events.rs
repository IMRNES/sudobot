//! Gateway events that are not slash commands.
//!
//! Joins, leaves and button presses reach the client here rather than through
//! poise, because poise only dispatches `Interaction::Command` and
//! `Interaction::Autocomplete`. Everything in this module is therefore an adapter
//! over a use case, exactly like a command handler.

use poise::serenity_prelude as serenity;
use tracing::{debug, info, warn};

use iam::application::autorole::{PlanJoinAutoRolesCommand, PlanJoinAutoRolesUseCase};
use iam::application::leveling::{
    GrantMessageXpCommand, GrantMessageXpUseCase, GrantVoiceXpCommand,
};
use iam::application::welcome::{
    BuildWelcomeMessageQuery, BuildWelcomeMessageUseCase, ResolvePanelButtonQuery,
    ResolvePanelButtonUseCase, WelcomeKind,
};
use iam::domain::guild::{GuildPatch, NewGuild};
use iam::domain::welcome::OptionBehavior;

use crate::data::Data;
use crate::voice_tracker::VoiceTracker;
use iam::application::ports::Clock;
use iam::domain::guild::GuildRepository;
use iam::domain::leveling::LevelingRepository;
use iam::domain::welcome::WelcomeRepository;

/// What a listener needs to act on one event.
///
/// Bundled rather than threaded through five arguments because every handler wants
/// the same three things, and the voice tracker is stateful across events.
pub struct Listener<'a> {
    pub data: &'a Data,
    pub ctx: &'a serenity::Context,
    pub voice: &'a VoiceTracker,
}

/// Dispatches one gateway event.
///
/// Each arm is deliberately small: the interesting logic lives in the use case it
/// calls, so the same rules apply whether a role is granted by a button press or
/// by the dashboard.
pub async fn handle(
    framework: poise::FrameworkContext<'_, Data, crate::Error>,
    event: &serenity::FullEvent,
) -> Result<(), crate::Error> {
    // `user_data()` is an async fn in poise 0.7: it polls until setup has stored the
    // value, which is why it cannot be read synchronously here.
    let data = framework.user_data().await;

    let listener = Listener {
        data,
        ctx: framework.serenity_context,
        voice: data.voice(),
    };

    match event {
        serenity::FullEvent::GuildCreate { guild, .. } => {
            handle_guild_create(data, framework.serenity_context, guild).await;
        }

        serenity::FullEvent::GuildDelete { incomplete, .. } => {
            handle_guild_delete(data, incomplete.id).await;
        }

        serenity::FullEvent::GuildMemberAddition { new_member } => {
            if let Err(error) = handle_member_join(listener.data, listener.ctx, new_member).await {
                // A join must never panic the gateway task: a misconfigured welcome or
                // autorole is the guild's problem, not a reason to disconnect.
                warn!(
                    %error,
                    guild_id = new_member.guild_id.get(),
                    "member join handling failed"
                );
            }
        }

        serenity::FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
            if let Err(error) =
                handle_member_leave(listener.data, listener.ctx, *guild_id, user).await
            {
                warn!(%error, guild_id = guild_id.get(), "member leave handling failed");
            }
        }

        serenity::FullEvent::InteractionCreate {
            interaction: serenity::Interaction::Component(component),
        } => {
            if let Err(error) = handle_component(listener.data, listener.ctx, component).await {
                warn!(%error, "component interaction handling failed");
            }
        }

        // The two arms the leveling engine hangs off. Both are best-effort: a
        // failed XP grant must never take the gateway down, and the next message
        // or voice tick retries.
        serenity::FullEvent::Message { new_message } => {
            handle_message(&listener, new_message).await;
        }

        serenity::FullEvent::VoiceStateUpdate { old, new } => {
            handle_voice_state(&listener, old.as_ref(), new).await;
        }

        _ => {}
    }

    Ok(())
}

/// The name to show for a member: guild nickname, then Discord's display name,
/// then the unique username.
///
/// The ordering matches what the member is called in this server — a nickname is
/// server-specific and outranks the global display name, which in turn outranks the
/// handle they cannot change. Getting it wrong would put a stale global name on a
/// leaderboard next to people using nicknames.
/// `PartialMember` rather than `Member`: a message's payload carries the partial
/// form, which is enough — it has the nickname, and the rest comes from the author.
fn display_name_of(author: &serenity::User, nick: Option<&str>) -> String {
    nick.filter(|nick| !nick.is_empty())
        .map(str::to_owned)
        .or_else(|| author.global_name.clone().filter(|name| !name.is_empty()))
        .unwrap_or_else(|| author.name.clone())
}

/// Awards message XP.
///
/// The guild, channel and member state the use case needs are all read from the
/// event rather than fetched. The member is the one part that can be missing — a
/// direct message has no member, and a message from someone who left mid-flight
/// can arrive without one — so `None` means "no XP", not an error.
async fn handle_message(listener: &Listener<'_>, message: &serenity::Message) {
    // Bots earn nothing: a bot that replies to every message would otherwise
    // out-level the entire guild. Discord's own webhooks carry `bot: true`.
    if message.author.bot {
        return;
    }

    let Some(guild_id) = message.guild_id else {
        // A direct message. There is no guild to level in.
        return;
    };

    // `member` is present on guild messages in serenity 0.12's `Message` model.
    let role_ids = match &message.member {
        Some(member) => member.roles.iter().map(|role| role.get()).collect(),
        // Without the member's roles the exemption list cannot be consulted, and
        // awarding anyway would hand XP to a member an admin meant to exclude. An
        // empty list is the safe read: nothing is exempt, so the grant proceeds
        // only if no role exemption applies to anyone — which is the common
        // configuration.
        None => Vec::new(),
    };

    // Record who is speaking before awarding, so the dashboard has a real name to
    // show the moment a member earns their first XP. Without this the leaderboard
    // has an id and nothing else, and anything it draws next to that id is invented.
    //
    // Best effort and not awaited-for-correctness: a failure here must not cost the
    // member their XP, and the next message retries it.
    if let Err(error) = listener
        .data
        .leveling
        .record_member_name(
            guild_id.get(),
            message.author.id.get(),
            &display_name_of(
                &message.author,
                message.member.as_ref().and_then(|m| m.nick.as_deref()),
            ),
            message.author.bot,
        )
        .await
    {
        debug!(%error, "could not record a member's display name");
    }

    let use_case = GrantMessageXpUseCase::new(listener.data.leveling.clone(), listener.data.clock);

    let outcome = match use_case
        .execute(GrantMessageXpCommand {
            guild_id: guild_id.get(),
            discord_user_id: message.author.id.get(),
            channel_id: message.channel_id.get(),
            role_ids,
        })
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            warn!(
                guild_id = guild_id.get(),
                user_id = message.author.id.get(),
                %error,
                "message xp grant failed"
            );
            return;
        }
    };

    let Some(outcome) = outcome else {
        // Ordinary: leveling is off, the channel is exempt, or they are on
        // cooldown. Most messages land here.
        return;
    };

    if outcome.leveled_up() {
        let http = listener.data.http();
        let applied = crate::level_rewards::apply(&outcome, &http).await;

        info!(
            guild_id = outcome.profile.guild_id,
            user_id = outcome.profile.discord_user_id,
            level = outcome.profile.level,
            roles_applied = applied,
            "member levelled up"
        );

        announce_level_up(listener, &outcome).await;
    }
}

/// Tracks voice time and awards the XP it is worth.
///
/// Discord sends a voice state update whenever anything about a member's voice
/// presence changes — mute, deafen, streaming — not just on join and leave. The
/// tracker reduces that stream to "joined at T" and "left at T", so a member who
/// toggles their mute ten times is still one session.
async fn handle_voice_state(
    listener: &Listener<'_>,
    old: Option<&serenity::VoiceState>,
    new: &serenity::VoiceState,
) {
    // A voice state for a user outside a guild (a DM call) has nothing to level in.
    let Some(guild_id) = new.guild_id else {
        return;
    };

    // A bot in a voice channel would earn XP for sitting there. Discord's music
    // bots live in voice permanently.
    if new
        .member
        .as_ref()
        .map(|member| member.user.bot)
        .unwrap_or(true)
    {
        return;
    }

    let user_id = new.user_id;

    // The member stopped being in a voice channel: close the session and pay for it.
    if new.channel_id.is_none() {
        let Some(seconds) =
            listener
                .voice
                .close(guild_id.get(), user_id.get(), listener.data.clock.now())
        else {
            return;
        };

        award_voice_xp(listener, guild_id.get(), user_id.get(), seconds).await;
        return;
    }

    // The member moved between channels: pay for the session that just ended and
    // start a new one, so crossing channels cannot be used to reset a long session
    // into free time.
    if let Some(old) = old
        && old.channel_id != new.channel_id
        && let Some(seconds) =
            listener
                .voice
                .close(guild_id.get(), user_id.get(), listener.data.clock.now())
    {
        award_voice_xp(listener, guild_id.get(), user_id.get(), seconds).await;
    }

    // A state update for a member already being tracked in this channel (a mute, a
    // deafen) must not restart the clock: that would erase the time already served.
    listener.voice.open(
        guild_id.get(),
        user_id.get(),
        new.channel_id.map(|channel| channel.get()),
        listener.data.clock.now(),
    );
}

/// Grants voice XP and applies whatever it earned.
async fn award_voice_xp(listener: &Listener<'_>, guild_id: u64, user_id: u64, seconds: i64) {
    let use_case = iam::application::leveling::GrantVoiceXpUseCase::new(
        listener.data.leveling.clone(),
        listener.data.clock,
    );

    let outcome = match use_case
        .execute(GrantVoiceXpCommand {
            guild_id,
            discord_user_id: user_id,
            seconds,
        })
        .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            warn!(guild_id, user_id, %error, "voice xp grant failed");
            return;
        }
    };

    let Some(outcome) = outcome else {
        debug!(guild_id, user_id, seconds, "voice time earned no xp");
        return;
    };

    if outcome.leveled_up() {
        let http = listener.data.http();
        let applied = crate::level_rewards::apply(&outcome, &http).await;

        info!(
            guild_id,
            user_id,
            seconds,
            level = outcome.profile.level,
            roles_applied = applied,
            "member levelled up from voice time"
        );

        announce_level_up(listener, &outcome).await;
    }
}

/// Posts the level-up announcement, if the guild configured a channel.
///
/// Best effort and silent when no channel is set: most guilds either announce or
/// do not, and asking for one is the dashboard's job, not a chat message's.
async fn announce_level_up(
    listener: &Listener<'_>,
    outcome: &iam::domain::leveling::LevelUpOutcome,
) {
    let config = match listener
        .data
        .leveling
        .find_config(outcome.profile.guild_id)
        .await
    {
        Ok(config) => config,
        Err(error) => {
            debug!(%error, "could not read the leveling config to announce");
            return;
        }
    };

    let Some(channel_id) = config.announce_channel_id else {
        return;
    };

    let rung = iam::domain::leveling::rung_for_level(outcome.profile.level)
        .map(|rung| rung.name.to_string())
        .unwrap_or_else(|| format!("level {}", outcome.profile.level));

    let channel = serenity::ChannelId::new(channel_id);
    if let Err(error) = channel
        .say(
            listener.ctx,
            format!(
                "<@{}> reached level {} — {}",
                outcome.profile.discord_user_id, outcome.profile.level, rung
            ),
        )
        .await
    {
        warn!(%error, channel_id, "could not post the level-up announcement");
    }
}

/// Records a guild the bot has been added to, so the dashboard can offer it.
pub async fn handle_guild_create(data: &Data, ctx: &serenity::Context, guild: &serenity::Guild) {
    // The member count is present on the create payload; the owner is not always,
    // hence the `Option`.
    let new_guild = NewGuild {
        id: guild.id.get(),
        name: guild.name.clone(),
        icon_url: guild.icon_url(),
        owner_discord_id: Some(guild.owner_id.get()),
        member_count: i32::try_from(guild.member_count).unwrap_or(i32::MAX),
    };

    match data.guilds.upsert(new_guild).await {
        Ok(recorded) => {
            info!(guild_id = recorded.id, name = %recorded.name, "joined a guild");

            // The dashboard has nothing to show until the system roles exist, so
            // they are seeded here rather than left to the first login.
            if let Err(error) = crate::bootstrap_guild::seed_system_roles(data, recorded.id).await {
                warn!(%error, guild_id = recorded.id, "could not seed the guild's system roles");
            }
        }
        Err(error) => {
            warn!(%error, guild_id = guild.id.get(), "could not record a joined guild");
        }
    }

    // The guild's channel and role lists are read straight off the payload rather
    // than fetched later: they are the only place we see them without another API
    // call, and the role-panel editor needs the role names to render its picker.
    ctx.cache.guilds();
}

/// Marks a guild the bot has left as inactive.
///
/// The row is kept rather than deleted: audit logs, economy history and leveling
/// profiles all reference it, and a rejoin should find its data intact.
pub async fn handle_guild_delete(data: &Data, guild_id: serenity::GuildId) {
    match data
        .guilds
        .update(guild_id.get(), GuildPatch::bot_left())
        .await
    {
        Ok(_) => info!(guild_id = guild_id.get(), "left a guild"),
        // A guild the bot left that it never recorded is not worth an error.
        Err(error) => {
            tracing::debug!(%error, guild_id = guild_id.get(), "could not mark a guild as left");
        }
    }
}

/// Greets a new member and queues their automatic roles.
pub async fn handle_member_join(
    data: &Data,
    ctx: &serenity::Context,
    member: &serenity::Member,
) -> Result<(), crate::Error> {
    let guild_id = member.guild_id.get();
    let is_bot = member.user.bot;

    // 1. Automatic roles. `plan` never fails the join: a misconfigured rule must
    //    not stop the member from arriving.
    let plan = PlanJoinAutoRolesUseCase::new(data.auto_roles.clone(), data.clock);
    let plan = plan
        .execute(PlanJoinAutoRolesCommand {
            guild_id,
            discord_user_id: member.user.id.get(),
            is_bot,
            joined_at: data.clock.now(),
        })
        .await?;

    if !plan.immediate.is_empty() {
        let roles: Vec<serenity::RoleId> = plan
            .immediate
            .iter()
            .map(|role_id| serenity::RoleId::new(*role_id))
            .collect();

        // Best effort: a role above the bot's own highest role cannot be granted,
        // and that is a server configuration problem the admin has to fix. The
        // deferred grants retry; immediate ones are logged and dropped.
        if let Err(error) = member.add_roles(ctx, &roles).await {
            warn!(
                %error,
                guild_id,
                user_id = member.user.id.get(),
                "could not grant the immediate automatic roles"
            );
        }
    }

    // 2. The level-0 rung.
    //
    //    Nobody levels up *into* level 0, so the leveling engine's level-change path
    //    can never grant it. Without this a new arrival has no title until they
    //    reach level 3, which makes the ladder's first rung dead weight — and it is
    //    supposed to be the badge every member wears from day one.
    //
    //    Best effort, and silent when the guild has not provisioned the ladder: an
    //    unconfigured guild is normal, not an error.
    grant_starting_role(data, ctx, member).await;

    // 3. The welcome message.
    let guild = data.guilds.find_by_id(guild_id).await?;
    let (guild_name, member_count) = match &guild {
        Some(guild) => (guild.name.clone(), i64::from(guild.member_count)),
        None => ("this server".to_string(), 0),
    };

    let build = BuildWelcomeMessageUseCase::new(data.welcome.clone());
    let message = build
        .execute(BuildWelcomeMessageQuery {
            guild_id,
            guild_name,
            member_count,
            discord_user_id: member.user.id.get(),
            username: member.user.name.clone(),
            kind: WelcomeKind::Welcome,
        })
        .await?;

    if let Some(message) = message {
        let channel = serenity::ChannelId::new(message.channel_id);

        // Built with serenity's own builder rather than poise's `CreateReply`:
        // this path is outside a command, where `CreateReply` does not apply.
        // Mirror the reference bot's shape: the greeting as message content, and the
        // server's card as a rich embed underneath. The colour, when set, is the
        // embed's colour bar; `None` leaves Discord's default.
        let mut embed = serenity::CreateEmbed::new();
        let mut has_embed = false;

        if let Some(card) = &message.embed {
            if let Some(author) = &card.author {
                embed = embed.author(serenity::CreateEmbedAuthor::new(author));
                has_embed = true;
            }
            if let Some(title) = &card.title {
                embed = embed.title(title);
                has_embed = true;
            }
            if let Some(description) = &card.description {
                embed = embed.description(description);
                has_embed = true;
            }
            if let Some(url) = &card.image_url {
                embed = embed.image(url);
                has_embed = true;
            }
            if let Some(url) = &card.thumbnail_url {
                embed = embed.thumbnail(url);
                has_embed = true;
            }
        }

        if let Some(colour) = message.embed_color {
            embed = embed.colour(colour as u32);
        }

        let mut builder = serenity::CreateMessage::new().content(&message.content);
        if has_embed || message.embed_color.is_some() {
            builder = builder.embed(embed);
        }

        if let Err(error) = channel.send_message(ctx, builder).await {
            warn!(%error, guild_id, "could not post the welcome message");
        }
    }

    Ok(())
}

/// Grants a new member the ladder's level-0 rung.
///
/// The one reward the level-change path can never deliver, because nobody levels up
/// *into* level 0. A guild that has not provisioned the ladder has no level-0
/// reward, and this is a no-op — an unconfigured server is the normal case, not an
/// error worth a log line above debug.
async fn grant_starting_role(data: &Data, ctx: &serenity::Context, member: &serenity::Member) {
    // Bots do not level and must not collect the guild's badges.
    if member.user.bot {
        return;
    }

    let guild_id = member.guild_id.get();

    let rewards = match data.leveling.list_rewards(guild_id).await {
        Ok(rewards) => rewards,
        Err(error) => {
            debug!(%error, guild_id, "could not read rewards for the starting rung");
            return;
        }
    };

    let Some(starting) = rewards.iter().find(|reward| reward.level == 0) else {
        debug!(
            guild_id,
            "no level-0 reward configured; skipping the starting role"
        );
        return;
    };

    // Already wearing it — a rejoin, or the welcome path running twice.
    if member
        .roles
        .contains(&serenity::RoleId::new(starting.role_id))
    {
        return;
    }

    if let Err(error) = member
        .add_role(ctx, serenity::RoleId::new(starting.role_id))
        .await
    {
        // The usual cause is the same one `/leveling ladder` warns about: a reward
        // role positioned above the bot's own role.
        warn!(
            %error,
            guild_id,
            user_id = member.user.id.get(),
            role_id = starting.role_id,
            "could not grant the starting level role"
        );
    }
}

/// Says goodbye.
pub async fn handle_member_leave(
    data: &Data,
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
    user: &serenity::User,
) -> Result<(), crate::Error> {
    // A bot leaving is not worth a farewell message.
    if user.bot {
        return Ok(());
    }

    let guild = data.guilds.find_by_id(guild_id.get()).await?;
    let (guild_name, member_count) = match &guild {
        Some(guild) => (guild.name.clone(), i64::from(guild.member_count)),
        None => ("this server".to_string(), 0),
    };

    let build = BuildWelcomeMessageUseCase::new(data.welcome.clone());
    let message = build
        .execute(BuildWelcomeMessageQuery {
            guild_id: guild_id.get(),
            guild_name,
            member_count,
            discord_user_id: user.id.get(),
            username: user.name.clone(),
            kind: WelcomeKind::Goodbye,
        })
        .await?;

    if let Some(message) = message {
        let channel = serenity::ChannelId::new(message.channel_id);
        if let Err(error) = channel.say(ctx, &message.content).await {
            warn!(%error, guild_id = guild_id.get(), "could not post the goodbye message");
        }
    }

    Ok(())
}

/// Handles a role-panel button press.
///
/// Acknowledged ephemerally before any work happens, because Discord invalidates
/// an interaction that has not been answered within three seconds and a database
/// round trip plus a role edit can exceed that on a slow day.
pub async fn handle_component(
    data: &Data,
    ctx: &serenity::Context,
    component: &serenity::ComponentInteraction,
) -> Result<(), crate::Error> {
    let resolve = ResolvePanelButtonUseCase::new(data.welcome.clone());
    let button = resolve
        .execute(ResolvePanelButtonQuery {
            custom_id: component.data.custom_id.clone(),
        })
        .await?;

    let Some(button) = button else {
        // An old button on a panel that has since been deleted. Ordinary, and the
        // member deserves an answer rather than silence.
        component
            .create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("That role panel is no longer available.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    };

    let guild_id = serenity::GuildId::new(button.guild_id);
    let role_id = serenity::RoleId::new(button.option.role_id);
    let user_id = component.user.id;

    // The member object is on the interaction when it came from a guild; fetching
    // it otherwise costs a request we can usually avoid.
    let member = match component.member.clone() {
        Some(member) => Some(member),
        None => guild_id.member(ctx, user_id).await.ok(),
    };

    let Some(member) = member else {
        component
            .create_response(
                ctx,
                serenity::CreateInteractionResponse::Message(
                    serenity::CreateInteractionResponseMessage::new()
                        .content("I could not read your membership in this server.")
                        .ephemeral(true),
                ),
            )
            .await?;
        return Ok(());
    };

    // What the press should do, decided by the option's behaviour and whether the
    // member already holds the role.
    let holds_role = member.roles.contains(&role_id);
    let should_add = match button.option.behavior {
        OptionBehavior::Toggle => !holds_role,
        OptionBehavior::Add => true,
        OptionBehavior::Remove => false,
        // Exclusive options replace their siblings; the removal happens below and
        // the role itself is always added.
        OptionBehavior::Exclusive => true,
    };

    let outcome = if should_add {
        apply_exclusive_removals(data, ctx, &member, &button).await;
        member.add_role(ctx, role_id).await
    } else {
        member.remove_role(ctx, role_id).await
    };

    let content = match outcome {
        Ok(()) if should_add => format!("Gave you <@&{}>.", role_id.get()),
        Ok(()) => format!("Removed <@&{}>.", role_id.get()),
        Err(error) => {
            // The realistic cause is a role positioned above the bot's own highest
            // role, which only a server admin can fix — so say so.
            warn!(%error, guild_id = button.guild_id, "could not change a member's role");
            "I could not change that role. An administrator may need to move my role higher."
                .to_string()
        }
    };

    component
        .create_response(
            ctx,
            serenity::CreateInteractionResponse::Message(
                serenity::CreateInteractionResponseMessage::new()
                    .content(content)
                    .ephemeral(true),
            ),
        )
        .await?;

    Ok(())
}

/// Removes a member's other options in an exclusive group.
///
/// Failures are ignored on purpose: the goal is that the member ends up with
/// exactly one role from the group, and the add that follows is the part that
/// matters. A leftover sibling is a cosmetic problem, not a reason to refuse.
async fn apply_exclusive_removals(
    data: &Data,
    ctx: &serenity::Context,
    member: &serenity::Member,
    chosen: &iam::application::welcome::PanelButton,
) {
    if chosen.option.behavior != OptionBehavior::Exclusive {
        return;
    }

    let siblings = match data.welcome.list_options(chosen.panel_id).await {
        Ok(options) => options,
        Err(error) => {
            tracing::debug!(%error, "could not read a panel's options");
            return;
        }
    };

    for sibling in siblings {
        if sibling.behavior != OptionBehavior::Exclusive || sibling.id == chosen.option.id {
            continue;
        }

        let role_id = serenity::RoleId::new(sibling.role_id);
        if !member.roles.contains(&role_id) {
            continue;
        }

        if let Err(error) = member.remove_role(ctx, role_id).await {
            tracing::debug!(%error, role_id = sibling.role_id, "could not remove an exclusive sibling");
        }
    }
}
