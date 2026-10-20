//! The slash commands.
//!
//! Each command is a thin adapter: it reads arguments off the interaction, calls a
//! use case, and renders the result. No business rule lives here — the rules live
//! in `iam::application`, shared with the dashboard, so the two surfaces can never
//! disagree about what "500 coins" or "level 5" means.
//!
//! Registration is one entry per top-level command; poise discovers subcommands
//! through the `subcommands(...)` attribute on each parent, so `economy::economy`
//! brings `/economy balance`, `/economy daily` and the rest with it.

pub mod economy;
pub mod jail;
pub mod leveling;
pub mod leveling_admin;
pub mod roles;
pub mod utility;

use poise::serenity_prelude as serenity;

use crate::Error;
use crate::data::Data;

/// Every command the bot registers.
pub fn all() -> Vec<poise::Command<Data, Error>> {
    vec![
        utility::ping(),
        utility::help(),
        utility::serverinfo(),
        economy::economy(),
        leveling::level(),
        leveling::rank(),
        leveling::leaderboard(),
        // The administrative half, under its own parent so `Manage Roles` gating is
        // visible from the command name.
        leveling_admin::leveling(),
        roles::rolepanel(),
        jail::jail(),
    ]
}

/// Rejects a command that only makes sense inside a guild.
///
/// Slash commands can be invoked from a DM, where there is no guild to configure
/// and no member to grant anything to.
pub fn require_guild(ctx: crate::Context<'_>) -> Result<serenity::GuildId, Error> {
    ctx.guild_id()
        .ok_or_else(|| "This command only works inside a server.".into())
}
