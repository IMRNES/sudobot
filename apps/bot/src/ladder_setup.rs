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
    /// Existing rungs switched to hoisted, so the member list groups by rung.
    pub roles_hoisted: usize,
    /// Existing rungs given their badge, or re-given one that had been removed.
    ///
    /// Counted separately from `roles_hoisted` because the two travel together but
    /// fail apart: a rung can keep its hoist and still lose its icon to a Discord
    /// hiccup, and a report that folds them together says "nothing to do" while
    /// every badge is missing.
    pub icons_applied: usize,
    /// Rungs whose reward row was created.
    pub rewards_created: usize,
    /// Rungs that already had a reward row at the correct level.
    pub rewards_reused: usize,
    /// Rungs whose existing reward row was moved to the level the ladder now
    /// specifies.
    ///
    /// Not folded into `rewards_reused`: a row that already sat at the right
    /// level and a row that had to be relocated are different outcomes, and the
    /// second is the one that tells an operator their thresholds changed.
    pub rewards_moved: usize,
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

        if self.rewards_moved > 0 {
            parts.push(format!("{} rewards moved", self.rewards_moved));
        }

        if self.bot_role_raised {
            parts.push("my role was raised above the ladder".to_string());
        }
        if self.roles_hoisted > 0 {
            parts.push(format!("{} roles hoisted", self.roles_hoisted));
        }
        // Reported on its own line because a run that only attached badges
        // otherwise reads as "nothing to do" — every count above says the rungs
        // were already present, while the visible change was the badges.
        if self.icons_applied > 0 {
            parts.push(format!("{} badges applied", self.icons_applied));
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

                // A reused rung still has to be brought in line: it may have been
                // created before hoisting was the default, so it sits in the member
                // list with no group of its own. Editing is idempotent, and doing it
                // here means an existing ladder is fixed by the same run that would
                // have created it.
                let needs_hoist = existing.get(role_id).is_some_and(|role| !role.hoist);
                // The badge is re-uploaded on every run, whenever one is embedded.
                //
                // Not gated on `role.icon.is_none()`: that was the first version, and
                // it meant corrected artwork could never reach the guild — the rung
                // already had *an* icon, so the run reported nothing to do while
                // Discord kept serving the old picture. Every badge in this release
                // was wrong in the same way, so "only when missing" would have fixed
                // none of them.
                //
                // A hash comparison would be cheaper, but Discord re-encodes the
                // upload: the hash it returns is not the hash of the bytes sent, so
                // there is nothing local to compare against. Re-sending is 21 role
                // edits once per boot, which is the honest price of being able to
                // repair a bad badge by deploying.
                let needs_icon = crate::ladder_icons::badge(rung.name).is_some();

                if needs_hoist || needs_icon {
                    let mut edit = serenity::EditRole::new();
                    if needs_hoist {
                        edit = edit.hoist(true);
                    }
                    if needs_icon {
                        if let Some(badge) = crate::ladder_icons::badge(rung.name) {
                            let attachment = serenity::CreateAttachment::bytes(
                                badge.png.to_vec(),
                                badge.file.to_string(),
                            );
                            edit = edit.icon(Some(&attachment));
                        } else {
                            warn!(
                                guild_id = guild_id.get(),
                                rung = rung.name,
                                "no badge is embedded for this rung; leaving the icon alone"
                            );
                        }
                    }

                    // `edit_role` on the guild, not `RoleId::edit`: serenity 0.12
                    // exposes the role edit through the guild handle here.
                    match http
                        .edit_role(guild_id, *role_id, &edit, Some("sudobot level ladder"))
                        .await
                    {
                        Ok(_) => {
                            if needs_hoist {
                                report.roles_hoisted += 1;
                            }
                            if needs_icon {
                                report.icons_applied += 1;
                            }
                        }
                        Err(error) => warn!(
                            guild_id = guild_id.get(),
                            rung = rung.name,
                            %error,
                            "could not bring an existing ladder role in line"
                        ),
                    }
                }

                *role_id
            }
            None => {
                // Permissions explicitly zero: this is the whole "cosmetic" promise,
                // enforced at creation rather than documented and hoped for.
                // Hoisted, so the ladder reads as a set of sections in the member
                // list: each rung gets its own group and the members inside it are
                // the ones at that level. Without this a 21-rung ladder is 21 roles
                // nobody sees, because Discord only groups members under hoisted
                // roles.
                let builder = serenity::EditRole::new()
                    .name(rung.name)
                    .colour(serenity::Colour(rung.colour))
                    .hoist(true)
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

        // The reward row. A fresh insert is the expected case for a guild that
        // has never provisioned. A conflict means the level already has a reward
        // row: if that row's role id matches this rung, the ladder is already
        // wired and the run is a no-op. If it does *not* match, someone moved the
        // rung's threshold (or provisioned a different ladder over the same guild)
        // — move the existing row to this rung's level so the member is still
        // placed at the level the badge actually means.
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
            Err(error) if error.is_already_present() => {
                let existing_level = match data.leveling.list_rewards(guild_id.get()).await {
                    Ok(rewards) => rewards
                        .iter()
                        .find(|reward| reward.role_id == role_id.get())
                        .map(|reward| reward.level),
                    Err(_) => None,
                };

                if existing_level == Some(rung.level) {
                    report.rewards_reused += 1;
                } else {
                    // The row is for this rung's role but sits at a different
                    // level: the ladder's thresholds changed since it was
                    // written. Move it rather than leave the title earned at a
                    // level nobody is told about.
                    match data.leveling.list_rewards(guild_id.get()).await {
                        Ok(rewards) => {
                            let row = rewards
                                .iter()
                                .find(|reward| reward.role_id == role_id.get());

                            match row {
                                Some(reward) => {
                                    match data
                                        .leveling
                                        .update_reward(reward.id, rung.level, role_id.get())
                                        .await
                                    {
                                        Ok(_) => report.rewards_moved += 1,
                                        Err(error) => {
                                            report.rewards_failed += 1;
                                            warn!(
                                                guild_id = guild_id.get(),
                                                rung = rung.name,
                                                %error,
                                                "could not move a ladder reward to its new level"
                                            );
                                        }
                                    }
                                }
                                None => {
                                    report.rewards_failed += 1;
                                    warn!(
                                        guild_id = guild_id.get(),
                                        rung = rung.name,
                                        "the level is taken by a reward row whose role is not this rung"
                                    );
                                }
                            }
                        }
                        Err(error) => {
                            report.rewards_failed += 1;
                            warn!(
                                guild_id = guild_id.get(),
                                rung = rung.name,
                                %error,
                                "could not read the ladder's reward rows"
                            );
                        }
                    }
                }
            }
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

    // The bot's own role is raised FIRST, and the order matters. Discord refuses a
    // reorder that would place a role at or above the bot's highest role, and the
    // ladder's target positions sit just above the base member role — so on a fresh
    // guild the bot is below them and every positioning call is rejected. Raising the
    // bot first is what makes the next line able to take effect at all.
    let (raised, ungrantable) = ensure_bot_role_is_above_ladder(data, ctx, guild_id).await?;
    report.bot_role_raised = raised;
    report.highest_ungrantable = ungrantable;

    // Then place the rungs. This is the step that decides the order members see.
    report.roles_repositioned = position_ladder(data, guild_id, &ordered).await;

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

    let before = match guild_id.roles(&http).await {
        Ok(roles) => roles,
        Err(error) => {
            warn!(%error, guild_id = guild_id.get(), "could not read roles to position the ladder");
            return 0;
        }
    };

    // Where the ladder starts. The default is just above `@everyone` — a cosmetic
    // title belongs at the bottom of the hierarchy. A guild whose members all wear a
    // base role ("Users", "Members") sets `ladder_base_role_id`, and then the ladder
    // must sit *above* that role: a badge positioned below the role every plain
    // member holds reads as a demotion and, worse, sorts below it in the member list
    // so the climb appears upside down.
    let base = match data.leveling.find_config(guild_id.get()).await {
        Ok(config) => config.ladder_base_role_id,
        Err(error) => {
            warn!(%error, guild_id = guild_id.get(), "could not read the ladder's base role");
            None
        }
    };

    // Where the ladder starts: one slot above the base role when one is configured,
    // else just above `@everyone`.
    let start = match base.and_then(|role_id| before.get(&serenity::RoleId::new(role_id))) {
        Some(role) => usize::from(role.position) + 1,
        None => 1,
    };

    // Already correct? Reading first keeps a re-run from issuing a pointless write
    // on every deploy, which also keeps the audit log clean.
    let already_ordered = ordered
        .iter()
        .enumerate()
        .all(|(slot, role_id)| slot_of(&before, role_id) == Some(start + slot));
    if already_ordered {
        debug!(
            guild_id = guild_id.get(),
            "the ladder is already in position"
        );
        return 0;
    }

    // ONE request naming EVERY role, and that completeness is the whole trick.
    //
    // Discord moves a role to the position you name only if nothing else ends up
    // there; naming just the twenty-one rungs asks for slots 2..22 while the guild's
    // own roles still hold most of them, and the rungs that lose the slot stay where
    // they were. The observed result was a ladder at 2,3,4,6,7,8,… with the guild's
    // roles interleaved, which is worse than not trying.
    //
    // Sending the complete list makes the requested positions a permutation of every
    // slot, so each rung lands exactly where asked. `@everyone` is excluded: it is
    // pinned at position 0 and naming it is an error. serenity's own
    // `edit_role_position` cannot express this — it builds a one-element array, so
    // twenty-one calls are twenty-one separate single-role moves.
    let existing: Vec<(u64, u16)> = before
        .values()
        .filter(|role| role.id.get() != guild_id.get())
        .map(|role| (role.id.get(), role.position))
        .collect();

    let ladder: Vec<u64> = ordered.iter().map(|role_id| role_id.get()).collect();

    let payload: Vec<serde_json::Value> = position_plan(&existing, &ladder, start)
        .into_iter()
        .map(|(id, position)| serde_json::json!({ "id": id.to_string(), "position": position }))
        .collect();

    let body = match serde_json::to_vec(&payload) {
        Ok(body) => body,
        Err(error) => {
            warn!(%error, guild_id = guild_id.get(), "could not encode the ladder's positions");
            return 0;
        }
    };

    let request = serenity::http::Request::new(
        serenity::http::Route::GuildRoles { guild_id },
        serenity::http::LightMethod::Patch,
    )
    .body(Some(body));

    match http.request(request).await {
        Ok(_) => {}
        Err(error) => {
            warn!(
                guild_id = guild_id.get(),
                %error,
                "could not place the ladder in one request"
            );
            return 0;
        }
    }

    // Read back rather than reporting the request count. Discord accepts a bulk
    // reorder and may still leave the ladder interleaved — a role above the bot's own
    // is skipped element by element — so the number that matters is how many rungs
    // actually landed, not how many were asked for.
    let after = match guild_id.roles(&http).await {
        Ok(roles) => roles,
        Err(error) => {
            warn!(%error, guild_id = guild_id.get(), "could not re-read the ladder's positions");
            return 0;
        }
    };

    let moved = ordered
        .iter()
        .enumerate()
        .filter(|(slot, role_id)| slot_of(&after, role_id) == Some(start + *slot))
        .count();

    if moved != ordered.len() {
        warn!(
            guild_id = guild_id.get(),
            placed = moved,
            expected = ordered.len(),
            start,
            "the ladder is not contiguous; a rung may sit above the bot's own role"
        );
    }

    moved
}

/// The complete role order to send, as `(role id, position)` pairs.
///
/// Pure so it can be tested without a guild: the ordering rule is the part that was
/// wrong, and it is much easier to pin down here than through Discord.
///
/// `existing` is every role but `@everyone`, as `(id, current position)`. The ladder
/// is spliced in directly above everything currently at or below `start`, and the
/// rest keeps its relative order. Positions are then assigned 1..n across the whole
/// list, which is what makes the request a permutation rather than a set of hopeful
/// single moves.
/// One role and the position it should end up at.
type Placed = (u64, u16);

fn position_plan(existing: &[Placed], ladder: &[u64], start: usize) -> Vec<Placed> {
    let ladder_ids: std::collections::HashSet<u64> = ladder.iter().copied().collect();

    let mut others: Vec<Placed> = existing
        .iter()
        .copied()
        .filter(|(id, _)| !ladder_ids.contains(id))
        .collect();
    others.sort_by_key(|(_, position)| *position);

    let (below, above): (Vec<Placed>, Vec<Placed>) = others
        .into_iter()
        .partition(|(_, position)| usize::from(*position) < start);

    below
        .iter()
        .map(|(id, _)| *id)
        .chain(ladder.iter().copied())
        .chain(above.iter().map(|(id, _)| *id))
        .enumerate()
        .map(|(slot, id)| (id, u16::try_from(slot + 1).unwrap_or(u16::MAX)))
        .collect()
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
) -> Result<(usize, usize, usize), crate::Error> {
    let http = data.http();

    let config = data.leveling.find_config(guild_id.get()).await?;
    let rewards = data.leveling.list_rewards(guild_id.get()).await?;

    if rewards.is_empty() {
        return Ok((0, 0, 0));
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
    let mut revocations = 0;

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

        if changes.grant.is_empty() && changes.revoke.is_empty() {
            continue;
        }

        let Some(member) = members
            .iter()
            .find(|member| member.user.id.get() == profile.discord_user_id)
        else {
            continue;
        };

        // Revocations come first: in the replacing configuration the newest rung is
        // granted and the older ones come off, and doing the removal second would
        // leave a member briefly wearing two rungs of a ladder that says they wear
        // one. Neither order can strand them with nothing, since the grant only
        // names a rung they have already reached.
        if !changes.revoke.is_empty() {
            let stale: Vec<serenity::RoleId> = changes
                .revoke
                .iter()
                .map(|role_id| serenity::RoleId::new(*role_id))
                .collect();

            match member.remove_roles(&http, &stale).await {
                Ok(()) => {
                    revocations += stale.len();
                    debug!(
                        guild_id = guild_id.get(),
                        user_id = profile.discord_user_id,
                        level = profile.level,
                        roles = stale.len(),
                        "removed superseded level role rewards"
                    );
                }
                Err(error) => {
                    // Not fatal to the grant below: a member left wearing an extra
                    // rung is worth reporting, but withholding the rung they earned
                    // would be worse.
                    warn!(
                        guild_id = guild_id.get(),
                        user_id = profile.discord_user_id,
                        %error,
                        "could not remove superseded level reward roles"
                    );
                }
            }
        }

        if changes.grant.is_empty() {
            continue;
        }

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

    Ok((examined, grants, revocations))
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
            roles_hoisted: 0,
            icons_applied: 0,
            rewards_created: 21,
            rewards_reused: 0,
            rewards_moved: 0,
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

    #[test]
    fn a_run_that_only_attached_badges_says_so() {
        // The bug this covers: every rung already existed, so the summary read
        // "21 roles already present" and nothing else, while the run had in fact
        // put a badge on each of them. Both `icons_applied` and `roles_hoisted`
        // are silent when zero, so a real change cannot hide behind a zero.
        let report = ProvisionReport {
            roles_reused: 21,
            icons_applied: 21,
            rewards_reused: 21,
            ..ProvisionReport::default()
        };

        let summary = report.summary();
        assert!(summary.contains("21 badges applied"), "{summary}");
        assert!(!summary.contains("0 badges"), "{summary}");
    }

    #[test]
    fn a_quiet_run_claims_nothing_it_did_not_do() {
        let report = ProvisionReport {
            roles_reused: 21,
            rewards_reused: 21,
            ..ProvisionReport::default()
        };

        let summary = report.summary();
        assert!(!summary.contains("badges"), "{summary}");
        assert!(!summary.contains("hoisted"), "{summary}");
        assert!(!summary.contains("reordered"), "{summary}");
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
        assert_eq!(title_for(7), "Neophyte");
        assert_eq!(title_for(10), "Apprentice");
        assert_eq!(title_for(10000), "IMMORTAL");
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

    /// The bug this function exists for: the ladder must come out as one contiguous
    /// block, and everything else must keep its relative order.
    #[test]
    fn the_ladder_is_spliced_in_as_one_block() {
        // @everyone is not in the input (it is pinned), so positions start at 1.
        let existing = vec![
            (10, 1), // "Users"  the base role
            (11, 2), // a bot role
            (12, 3), // some other guild role
        ];
        let ladder = vec![100, 101, 102];

        let plan = position_plan(&existing, &ladder, 2);

        // Users stays at the bottom, the ladder is next as a block, the rest follows.
        assert_eq!(
            plan,
            vec![(10, 1), (100, 2), (101, 3), (102, 4), (11, 5), (12, 6)]
        );
    }

    /// Every role must appear exactly once. A missing role would be left at whatever
    /// position it already had, which is how the ladder ended up interleaved.
    #[test]
    fn every_role_appears_exactly_once() {
        let existing = vec![(10, 1), (11, 2), (12, 3), (13, 4)];
        let ladder = vec![100, 101];

        let plan = position_plan(&existing, &ladder, 1);

        assert_eq!(plan.len(), existing.len() + ladder.len());
        let mut ids: Vec<u64> = plan.iter().map(|(id, _)| *id).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec![10, 11, 12, 13, 100, 101]);
    }

    /// Positions must be 1..n with no gaps and no ties. Discord only moves a role
    /// into a free slot, so a duplicated position leaves a rung where it was.
    #[test]
    fn positions_are_dense_and_unique() {
        let existing = vec![(10, 1), (11, 7), (12, 9)];
        let ladder = vec![100, 101, 102];

        let plan = position_plan(&existing, &ladder, 2);

        let positions: Vec<u16> = plan.iter().map(|(_, position)| *position).collect();
        let expected: Vec<u16> = (1..=plan.len() as u16).collect();
        assert_eq!(positions, expected);
    }

    /// With no base role configured the ladder sits directly above `@everyone`,
    /// which is where it has always been.
    #[test]
    fn without_a_base_role_the_ladder_sits_at_the_bottom() {
        let existing = vec![(10, 1), (11, 2)];
        let ladder = vec![100, 101];

        let plan = position_plan(&existing, &ladder, 1);

        assert_eq!(plan, vec![(100, 1), (101, 2), (10, 3), (11, 4)]);
    }
}
