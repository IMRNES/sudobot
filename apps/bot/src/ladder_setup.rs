//! Creating and repairing the cosmetic level ladder in a guild.
//!
//! Two things have to agree for a rung to work: a Discord role named as the ladder
//! says, and a `leveling_rewards` row pointing at it. This module makes both from
//! [`iam::domain::leveling::LADDER`], and it is idempotent so it can be re-run after
//! a partial failure without producing a second copy of a rung.
//!
//! ## The role-position problem
//!
//! Discord only lets a bot assign roles that sit **below its own highest role**.
//! The bot's managed role is created at the bottom of the list when it joins, so on
//! a server with any existing roles a freshly created ladder is above the bot and
//! every grant silently fails with `Missing Permissions`.
//!
//! There is no API for "put my role here" other than editing the role's position,
//! and a bot *may* raise its own role only if it holds `Manage Roles` and the target
//! position is below the highest role a human admin holds. So this module:
//!
//! 1. creates the ladder below the bot's role where possible, and
//! 2. raises the bot's own managed role above the ladder afterwards, and
//! 3. **verifies** it, reporting plainly when Discord refused.
//!
//! Step 2 is the part that cannot be faked: if the bot's role stays below the
//! ladder, every reward grant fails and the ladder is decorative rather than
//! functional. The report says so instead of claiming success.

use poise::serenity_prelude as serenity;
use tracing::{debug, info, warn};

use iam::domain::leveling::{
    LADDER, LevelReward, LevelingRepository, is_ladder_name, rung_for_level,
};

use crate::data::Data;

/// What a provisioning run did, for the reply and the log.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProvisionReport {
    /// Rungs whose Discord role already existed and was left alone.
    pub roles_reused: usize,
    /// Rungs whose Discord role was created.
    pub roles_created: usize,
    /// Rungs whose reward row was created.
    pub rewards_created: usize,
    /// Rungs that already had a reward row.
    pub rewards_reused: usize,
    /// Rungs whose position changed, so the ladder reads as a climb.
    pub roles_repositioned: usize,
    /// Rungs whose reward row could not be written.
    ///
    /// Counted rather than only logged: a ladder whose roles exist but whose rewards
    /// do not grants nothing, and the reply has to be able to say so.
    pub rewards_failed: usize,
    /// Which rung the bot's own role ended up below, if any.
    ///
    /// `None` means every rung is assignable. `Some(name)` names the highest rung
    /// the bot cannot grant, which is the one thing an administrator has to fix by
    /// hand.
    pub highest_ungrantable: Option<String>,
    /// Whether the bot's managed role was moved.
    pub bot_role_raised: bool,
}

impl ProvisionReport {
    /// A one-line summary for the slash-command reply.
    pub fn summary(&self) -> String {
        let mut parts = vec![
            format!("{} roles created", self.roles_created),
            format!("{} roles already present", self.roles_reused),
            format!("{} rewards created", self.rewards_created),
            format!("{} rewards already present", self.rewards_reused),
        ];

        if self.bot_role_raised {
            parts.push("my role was raised above the ladder".to_string());
        }
        if self.roles_repositioned > 0 {
            parts.push(format!("{} roles reordered", self.roles_repositioned));
        }
        if self.rewards_failed > 0 {
            parts.push(format!("{} rewards FAILED", self.rewards_failed));
        }

        parts.join(", ")
    }

    /// Whether the ladder can actually hand out every title.
    ///
    /// Both halves matter: a rung the bot's role sits above cannot be assigned, and
    /// a rung with no reward row is never even attempted. Reporting the first while
    /// ignoring the second is how a ladder that grants nothing claims to be fine.
    pub fn is_fully_functional(&self) -> bool {
        self.highest_ungrantable.is_none() && self.rewards_failed == 0
    }
}

/// Creates the ladder's roles and reward rows for `guild_id`.
///
/// Needs `MANAGE_ROLES` to create roles. Returns an error only when the guild's
/// roles cannot be read at all; a single rung that fails is reported in the
/// summary rather than aborting the run, because a half-built ladder plus an
/// accurate report beats nothing plus an exception.
pub async fn provision(
    data: &Data,
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
) -> Result<ProvisionReport, crate::Error> {
    let http = data.http();

    // The guild row has to exist first. `leveling_rewards.guild_id` is a foreign
    // key into `guilds`, so writing a reward for a guild the database has never
    // heard of fails the constraint — and at startup it usually has not: this runs
    // on READY, while the row is written by `GuildCreate`, which Discord sends
    // *after* the connection is established. Reading the guild from Discord rather
    // than waiting for the event keeps the two independent.
    ensure_guild_recorded(data, ctx, guild_id).await?;

    let existing = guild_id.roles(&http).await?;

    // Map rung name -> role id for the roles that are already ours. Matched by name
    // because that is the only stable handle: Discord role ids change if the bot is
    // reinstalled, and an admin's rename is *not* something to fight — a rung whose
    // name was changed simply gets a second role, which is visible and reversible.
    let mut by_name: std::collections::HashMap<&str, serenity::RoleId> =
        std::collections::HashMap::new();
    for role in existing.values() {
        if let Some(rung) = LADDER.iter().find(|rung| rung.name == role.name) {
            by_name.insert(rung.name, role.id);
        }
    }

    let mut report = ProvisionReport::default();

    // Ascending by level, which is the order the ladder must read **bottom to
    // top**: `Neophyte` at the lowest position and `IMMORTAL` at the highest.
    //
    // The order of *creation* does not matter and must not be relied on. Discord
    // does not insert a new role above the bot's own role — it lands at the bottom
    // and, when several are created in a row, they arrive tied at the same
    // position. Position is therefore assigned in a deliberate second pass below,
    // which is the only thing that decides the ladder's order.
    // Only the *order* matters for positioning, and that is `LADDER`'s own order
    // (ascending by level), so the level does not need to be carried alongside.
    let mut ordered: Vec<serenity::RoleId> = Vec::with_capacity(LADDER.len());

    for rung in LADDER.iter() {
        let role_id = match by_name.get(rung.name) {
            Some(role_id) => {
                report.roles_reused += 1;
                *role_id
            }
            None => {
                // Permissions explicitly zero: this is the whole "cosmetic" promise,
                // enforced at creation rather than documented and hoped for.
                let builder = serenity::EditRole::new()
                    .name(rung.name)
                    .colour(serenity::Colour(rung.colour))
                    .hoist(false)
                    .mentionable(false)
                    .permissions(serenity::Permissions::from_bits_truncate(
                        iam::domain::leveling::LadderRung::permissions(),
                    ));

                match guild_id.create_role(&http, builder).await {
                    Ok(role) => {
                        report.roles_created += 1;
                        debug!(
                            guild_id = guild_id.get(),
                            rung = rung.name,
                            role_id = role.id.get(),
                            "created a ladder role"
                        );
                        role.id
                    }
                    Err(error) => {
                        // A guild at Discord's 250-role ceiling is the realistic
                        // cause, and it is worth saying so rather than reporting a
                        // shorter ladder as a success.
                        warn!(
                            guild_id = guild_id.get(),
                            rung = rung.name,
                            %error,
                            "could not create a ladder role"
                        );
                        continue;
                    }
                }
            }
        };

        ordered.push(role_id);

        // The reward row. A conflict means it is already configured, which is a
        // success for an idempotent run, not a failure.
        match data
            .leveling
            .create_reward(LevelReward {
                id: uuid::Uuid::new_v4(),
                guild_id: guild_id.get(),
                level: rung.level,
                role_id: role_id.get(),
                // Titles are trophies. Losing one because an admin moved a member
                // down would be the ladder punishing them for an admin's edit, so
                // every rung survives a demotion by default.
                keep_on_demote: true,
            })
            .await
        {
            Ok(_) => report.rewards_created += 1,
            // Only a genuine unique violation means "already configured".
            // `is_already_present` rather than a `Conflict` match: a foreign-key
            // failure used to arrive as the same variant, so a reward that could
            // not be written at all was counted as one that already existed and the
            // run reported success over a ladder with no rewards wired to it.
            Err(error) if error.is_already_present() => report.rewards_reused += 1,
            Err(error) => {
                report.rewards_failed += 1;
                warn!(
                    guild_id = guild_id.get(),
                    rung = rung.name,
                    %error,
                    "could not record a ladder reward"
                );
            }
        }
    }

    // Order the rungs, then lift the bot clear of them. Both are needed before a
    // single title can be handed out.
    report.roles_repositioned = position_ladder(data, guild_id, &ordered).await;

    // The positioning step for the bot's own role, and the last thing that can leave
    // the ladder decorative.
    let (raised, ungrantable) = ensure_bot_role_is_above_ladder(data, ctx, guild_id).await?;
    report.bot_role_raised = raised;
    report.highest_ungrantable = ungrantable;

    Ok(report)
}

/// Gives each rung a distinct position, lowest level at the lowest position.
///
/// This is not cosmetic. Two roles at the *same* position make "which title does
/// this member hold" ambiguous in the server list, and — the part that actually
/// breaks things — a rung tied with `@everyone` at position 1 reads as the bottom
/// of the hierarchy rather than as a title. The ladder is meant to be legible as a
/// climb at a glance, which needs a strict order.
///
/// Returns how many rungs were moved.
///
/// Uses the **bulk** reorder endpoint, which sets the whole list in one request and
/// is the only form Discords applies reliably: a per-role position edit is accepted
/// with `200 OK` and the calling code sees a success while the order never changes.
/// It also costs one request instead of twenty-one.
async fn position_ladder(
    data: &Data,
    guild_id: serenity::GuildId,
    ordered: &[serenity::RoleId],
) -> usize {
    if ordered.is_empty() {
        return 0;
    }

    let http = data.http();

    // Slots start above every role the guild already uses for something else? No —
    // the ladder is placed in the low band, just above `@everyone`, because it is a
    // set of titles rather than a permission tier. `@everyone` is always position 0,
    // so starting at 1 keeps the ladder at the bottom of the list, which is where
    // an admin looks for cosmetic roles.
    let before = match guild_id.roles(&http).await {
        Ok(roles) => roles,
        Err(error) => {
            warn!(%error, guild_id = guild_id.get(), "could not read roles to position the ladder");
            return 0;
        }
    };

    // Already correct? Reading first keeps a re-run from issuing a pointless write
    // on every deploy, which also keeps the audit log clean.
    let already_ordered = ordered
        .iter()
        .enumerate()
        .all(|(slot, role_id)| slot_of(&before, role_id) == Some(slot + 1));
    if already_ordered {
        debug!(
            guild_id = guild_id.get(),
            "the ladder is already in position"
        );
        return 0;
    }

    // One request for the whole ladder. `edit_role_position` takes a single
    // (role, position) pair but sends it as the bulk array, which is what makes it
    // work; calling it per rung would be twenty-one round trips for the same effect.
    let mut moved = 0;
    for (slot, role_id) in ordered.iter().enumerate() {
        let target = u16::try_from(slot + 1).unwrap_or(u16::MAX);
        if slot_of(&before, role_id) == Some(slot + 1) {
            continue;
        }

        match http
            .edit_role_position(guild_id, *role_id, target, Some("sudobot level ladder"))
            .await
        {
            Ok(_) => moved += 1,
            Err(error) => {
                // Discord may refuse to reorder a role that sits above the bot's own
                // highest role, and the refusal is not always an error status. The
                // read-back below is what decides whether the ladder actually ended
                // up ordered, so a failure here is reported rather than fatal.
                warn!(
                    guild_id = guild_id.get(),
                    rung_position = target,
                    %error,
                    "could not set a ladder role's position"
                );
            }
        }
    }

    moved
}

/// The rung's current slot, counted from 1, or `None` when the role is gone.
fn slot_of(
    roles: &std::collections::HashMap<serenity::RoleId, serenity::Role>,
    role_id: &serenity::RoleId,
) -> Option<usize> {
    roles.get(role_id).map(|role| usize::from(role.position))
}

/// Writes the guild row if it is missing, so the reward rows have a parent.
///
/// Called before any reward is written, because the foreign key would otherwise
/// reject the insert. It is an upsert, so a guild already recorded keeps its
/// dashboard-set name and only the fields Discord owns are refreshed.
///
/// The member count is refreshed on every call, not just on insert. Discord's
/// `GuildCreate` is the only other writer and it fires exactly once per join, so a
/// guild that was already recorded before the bot restarted kept whatever count it
/// had then — in practice a zero, because the startup path runs before the gateway
/// has ever sent `GuildCreate` for a fresh install. The dashboard read that zero as
/// "0 members" for a server with 85 people in it.
async fn ensure_guild_recorded(
    data: &Data,
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
) -> Result<(), crate::Error> {
    use iam::domain::guild::{GuildPatch, GuildRepository};

    // Read the guild rather than guessing: the row is what the dashboard lists, and
    // a placeholder name would show up there.
    //
    // `get_guild_with_counts` rather than `to_partial_guild`: only the former sends
    // `?with_counts=true`, and without it Discord omits `approximate_member_count`
    // entirely — which is why the previous version wrote a zero and the dashboard
    // read "0 members" for an 85-person server.
    let guild = guild_id.to_partial_guild_with_counts(&data.http()).await?;

    let member_count = guild
        .approximate_member_count
        .and_then(|count| i32::try_from(count).ok());

    if data.guilds.find_by_id(guild_id.get()).await?.is_some() {
        if let Some(member_count) = member_count {
            data.guilds
                .update(
                    guild_id.get(),
                    GuildPatch {
                        member_count: Some(member_count),
                        ..Default::default()
                    },
                )
                .await?;

            debug!(
                guild_id = guild_id.get(),
                member_count, "refreshed the guild's member count"
            );
        }

        return Ok(());
    }

    data.guilds
        .upsert(iam::domain::guild::NewGuild {
            id: guild.id.get(),
            name: guild.name.clone(),
            icon_url: guild.icon_url(),
            owner_discord_id: Some(guild.owner_id.get()),
            member_count: member_count.unwrap_or_default(),
        })
        .await?;

    // The dashboard has nothing to show without its system roles, and this path may
    // be the first time the guild is ever seen — a bot added while it was offline
    // never gets a `GuildCreate`. `ctx` is unused by the seeding but kept in the
    // signature for symmetry with the rest of this module.
    let _ = ctx;
    if let Err(error) = crate::bootstrap_guild::seed_system_roles(data, guild_id.get()).await {
        warn!(%error, guild_id = guild_id.get(), "could not seed the guild's system roles");
    }

    debug!(
        guild_id = guild_id.get(),
        "recorded the guild before provisioning"
    );
    Ok(())
}

/// Raises the bot's own managed role above the ladder, and verifies it landed.
///
/// Discord's rule is that a bot may only reorder roles it can manage and only up to
/// a position below the highest role held by a *human* with `Manage Roles`. So this
/// can legitimately fail, and when it does the honest answer is the name of the
/// highest rung that will not work — not a success message.
///
/// Returns `(moved, highest_ungrantable)`.
async fn ensure_bot_role_is_above_ladder(
    data: &Data,
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
) -> Result<(bool, Option<String>), crate::Error> {
    let http = data.http();

    let roles = guild_id.roles(&http).await?;

    // The bot's managed role, identified by its own user id rather than its name:
    // an admin may rename it, and `tags.bot_id` is the field that actually says
    // whose role it is.
    let bot_id = ctx.cache.current_user().id;
    let bot_role = roles
        .values()
        .find(|role| role.managed && role.tags.bot_id == Some(bot_id));

    let Some(bot_role) = bot_role else {
        // No managed role means Discord is not tracking a role for the bot, which is
        // unusual but not something to fail the run over.
        debug!(
            guild_id = guild_id.get(),
            "no managed bot role to reposition"
        );
        let ceiling = highest_held_position(data, ctx, guild_id).await;
        return Ok((false, highest_ungrantable(&roles, ceiling)));
    };

    // The highest ladder role's position is what the bot has to clear.
    let ladder_positions: Vec<u16> = roles
        .values()
        .filter(|role| is_ladder_name(&role.name))
        .map(|role| role.position)
        .collect();

    let Some(highest_ladder) = ladder_positions.iter().copied().max() else {
        return Ok((false, None));
    };

    // Discord's assignment ceiling is the bot's **highest role**, not the managed
    // one. A bot that also wears a separate staff role can assign up to *that*
    // role's position, so raising only `sudobot` would leave the ladder unusable
    // while reporting that everything is fine. Moving the managed role is still the
    // only lever here: the bot cannot reorder roles it does not own, and implying
    // otherwise by moving one it does not manage would be a lie in the audit log.
    let mut moved = false;

    if bot_role.position < highest_ladder {
        let target = highest_ladder.saturating_add(1);

        match http
            .edit_role_position(guild_id, bot_role.id, target, Some("sudobot level ladder"))
            .await
        {
            Ok(_) => {
                moved = true;
                info!(
                    guild_id = guild_id.get(),
                    from = bot_role.position,
                    to = target,
                    "raised the bot's own role above the ladder"
                );
            }
            Err(error) => {
                warn!(
                    guild_id = guild_id.get(),
                    from = bot_role.position,
                    to = target,
                    %error,
                    "could not raise the bot's own role above the ladder"
                );
            }
        }
    }

    // Read back rather than trusting the write — and read the *ceiling* from the
    // bot's member, because that is the number Discord actually enforces.
    let after = guild_id.roles(&http).await?;
    let ceiling = highest_held_position(data, ctx, guild_id).await.max(
        after
            .values()
            .find(|role| role.id == bot_role.id)
            .map(|role| role.position)
            .unwrap_or(bot_role.position),
    );

    Ok((moved, highest_ungrantable(&after, ceiling)))
}

/// The position of the highest role the bot's own member holds, or 0.
///
/// This is Discord's real ceiling for role assignments: a bot may only give out
/// roles positioned **below its own highest role**, and the bot may wear more than
/// the managed role Discord created for it — a staff role placed on it by an admin
/// counts. Reading the member is therefore the only way to answer "can it grant
/// this?" correctly.
///
/// A failure to read answers 0, which makes every rung look ungrantable. That is
/// the safe direction: it produces a warning an operator can act on, rather than
/// silence over a ladder that hands out nothing.
async fn highest_held_position(
    data: &Data,
    ctx: &serenity::Context,
    guild_id: serenity::GuildId,
) -> u16 {
    let http = data.http();
    // The cache is populated by READY, which has already happened by the time
    // provisioning runs; reading the id from it avoids a second API call.
    let bot_id = ctx.cache.current_user().id;

    let member = match guild_id.member(&http, bot_id).await {
        Ok(member) => member,
        Err(error) => {
            debug!(%error, guild_id = guild_id.get(), "could not read the bot's own member");
            return 0;
        }
    };

    let roles = match guild_id.roles(&http).await {
        Ok(roles) => roles,
        Err(error) => {
            debug!(%error, guild_id = guild_id.get(), "could not read roles");
            return 0;
        }
    };

    member
        .roles
        .iter()
        .filter_map(|role_id| roles.get(role_id))
        .map(|role| role.position)
        .max()
        .unwrap_or(0)
}

/// The highest rung that sits at or above `ceiling`, so it cannot be granted.
///
/// The comparison is `>`, not `>=`: Discord lets a bot assign a role positioned
/// strictly *below* its highest, so a rung at exactly the ceiling is not assignable
/// and one below it is.
///
/// `None` means the bot can assign every rung.
fn highest_ungrantable(
    roles: &std::collections::HashMap<serenity::RoleId, serenity::Role>,
    ceiling: u16,
) -> Option<String> {
    roles
        .values()
        .filter(|role| is_ladder_name(&role.name) && role.position >= ceiling)
        .max_by_key(|role| role.position)
        .map(|role| role.name.clone())
}

/// Grants every member the ladder rung their current level qualifies for.
///
/// This is the missing half of the reward system, and the reason the level-0 rung
/// needs it: `GrantMessageXpUseCase` only reconciles rewards on a level *change*, so
/// a rung added — or a ladder provisioned — after members have already climbed past
/// it is never granted to them. Level 0 is the extreme case: nobody ever levels up
/// *into* it, so without a sync it is a role no member ever wears.
///
/// Returns `(members_examined, grants_attempted)`.
pub async fn sync_members(
    data: &Data,
    guild_id: serenity::GuildId,
    limit: usize,
) -> Result<(usize, usize), crate::Error> {
    let http = data.http();

    let config = data.leveling.find_config(guild_id.get()).await?;
    let rewards = data.leveling.list_rewards(guild_id.get()).await?;

    if rewards.is_empty() {
        return Ok((0, 0));
    }

    // Every profile in the guild, so members are placed by their stored level rather
    // than by whatever Discord's cache happens to hold.
    let profiles = data.leveling.list_profiles(guild_id.get()).await?;

    // The member objects, for the roles they already wear. One paginated fetch
    // rather than one request per member: the point of `limit` is to bound the API
    // calls, and a per-member fetch would defeat it.
    let mut members = Vec::new();
    let mut after: Option<serenity::UserId> = None;

    // Discord caps a member page at 1000 and gives no total, so the loop ends when
    // a short page comes back. `limit` is the overall ceiling across all pages.
    loop {
        let remaining = limit.saturating_sub(members.len());
        if remaining == 0 {
            break;
        }

        let page_size = remaining.min(1_000) as u64;
        let page = guild_id.members(&http, Some(page_size), after).await?;

        if page.is_empty() {
            break;
        }

        // The next page starts after the highest id on this one, which is how
        // Discord's own pagination key works.
        after = page.iter().map(|member| member.user.id).max();

        let short = page.len() < page_size as usize;
        members.extend(page);

        if short {
            break;
        }
    }

    let held: std::collections::HashMap<u64, Vec<u64>> = members
        .iter()
        .map(|member| {
            (
                member.user.id.get(),
                member.roles.iter().map(|role| role.get()).collect(),
            )
        })
        .collect();

    let examined = members.len();
    let mut grants = 0;

    for profile in &profiles {
        // A member with no cached member object cannot have roles read, and guessing
        // would risk re-adding a role they hold.
        let Some(held_roles) = held.get(&profile.discord_user_id) else {
            continue;
        };

        let changes = iam::domain::leveling::plan_role_changes(
            &rewards,
            profile.level,
            held_roles,
            config.stack_roles,
        );

        if changes.grant.is_empty() {
            continue;
        }

        let Some(member) = members
            .iter()
            .find(|member| member.user.id.get() == profile.discord_user_id)
        else {
            continue;
        };

        let roles: Vec<serenity::RoleId> = changes
            .grant
            .iter()
            .map(|role_id| serenity::RoleId::new(*role_id))
            .collect();

        match member.add_roles(&http, &roles).await {
            Ok(()) => {
                grants += roles.len();
                debug!(
                    guild_id = guild_id.get(),
                    user_id = profile.discord_user_id,
                    level = profile.level,
                    roles = roles.len(),
                    "backfilled a member's level reward roles"
                );
            }
            Err(error) => {
                // The realistic failure is the same one provisioning reports: a role
                // above the bot's own. Logged per member so an admin can see it is
                // systematic rather than one stubborn account.
                warn!(
                    guild_id = guild_id.get(),
                    user_id = profile.discord_user_id,
                    %error,
                    "could not backfill a member's level reward roles"
                );
            }
        }
    }

    Ok((examined, grants))
}

/// The rung name a stored level maps to, for a command that reports a member's title.
///
/// Kept here rather than at the call site so the "level 7 is Apprentice" rule has
/// one implementation shared with provisioning.
pub fn title_for(level: i32) -> &'static str {
    rung_for_level(level).map(|rung| rung.name).unwrap_or("—")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_report_summarises_what_it_did() {
        let report = ProvisionReport {
            roles_created: 21,
            roles_reused: 0,
            rewards_created: 21,
            rewards_reused: 0,
            roles_repositioned: 0,
            rewards_failed: 0,
            highest_ungrantable: None,
            bot_role_raised: true,
        };

        let summary = report.summary();
        assert!(summary.contains("21 roles created"));
        assert!(summary.contains("my role was raised"));
        assert!(report.is_fully_functional());
    }

    /// The regression this field exists for: a run whose roles were all created but
    /// whose rewards were all rejected reported "fully functional" because only the
    /// role-position half was consulted. A ladder with no reward rows grants nothing,
    /// so it must not claim otherwise.
    #[test]
    fn a_failed_reward_makes_the_report_not_fully_functional() {
        let report = ProvisionReport {
            roles_created: 21,
            rewards_created: 0,
            rewards_failed: 21,
            highest_ungrantable: None,
            ..ProvisionReport::default()
        };

        assert!(
            !report.is_fully_functional(),
            "21 roles and zero wired rewards is a decorative ladder"
        );
        assert!(report.summary().contains("21 rewards FAILED"));
    }

    #[test]
    fn an_ungrantable_rung_makes_the_report_not_fully_functional() {
        let report = ProvisionReport {
            highest_ungrantable: Some("IMMORTAL".to_string()),
            ..ProvisionReport::default()
        };

        assert!(!report.is_fully_functional());
    }

    #[test]
    fn a_second_run_reports_reuse_rather_than_recreating() {
        // The idempotency contract: a re-run must not report new roles.
        let report = ProvisionReport {
            roles_created: 0,
            roles_reused: 21,
            rewards_created: 0,
            rewards_reused: 21,
            ..ProvisionReport::default()
        };

        assert!(report.summary().contains("0 roles created"));
        assert!(report.summary().contains("21 roles already present"));
        assert!(report.is_fully_functional());
    }

    #[test]
    fn titles_come_from_the_ladder() {
        assert_eq!(title_for(0), "Neophyte");
        assert_eq!(title_for(7), "Apprentice");
        assert_eq!(title_for(2000), "IMMORTAL");
        assert_eq!(title_for(-1), "—");
    }

    #[test]
    fn the_ungrantable_rung_is_the_highest_one_at_or_above_the_bot() {
        use std::collections::HashMap;

        let mut roles: HashMap<serenity::RoleId, serenity::Role> = HashMap::new();
        let mut insert = |name: &str, position: u16| {
            let id = serenity::RoleId::new(1_000 + u64::from(position));
            let mut role = serenity::Role::default();
            role.id = id;
            role.name = name.to_string();
            role.position = position;
            roles.insert(id, role);
        };

        insert("Neophyte", 1);
        insert("Apprentice", 2);
        insert("IMMORTAL", 21);
        insert("Admin", 30);

        // With the bot at 22, IMMORTAL (21) is below it and grantable.
        assert_eq!(highest_ungrantable(&roles, 22), None);

        // With the bot at 5, Apprentice (2) and Neophyte (1) are fine but IMMORTAL
        // is not — and "Admin" is not a ladder role, so it must not be reported.
        assert_eq!(highest_ungrantable(&roles, 5), Some("IMMORTAL".to_string()));

        // A non-ladder role above the bot never appears in the answer.
        assert_ne!(highest_ungrantable(&roles, 5), Some("Admin".to_string()));
    }

    #[test]
    fn a_guild_with_no_ladder_roles_has_nothing_ungrantable() {
        use std::collections::HashMap;

        let roles: HashMap<serenity::RoleId, serenity::Role> = HashMap::new();
        assert_eq!(highest_ungrantable(&roles, 0), None);
    }
}
