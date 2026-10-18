use std::collections::HashSet;

use tracing::{debug, info};
use uuid::Uuid;

use crate::application::ports::{Clock, DiscordGuildSummary, DiscordOAuthClient};
use crate::domain::auth::AuthError;
use crate::domain::guild::{DISCORD_ADMINISTRATOR, GuildRepository, has_manage_guild};
use crate::domain::guild_member::{GuildMemberRepository, NewGuildMember};
use crate::domain::rbac::{GuildRoleRepository, default_role_permissions, slugs};

/// Runs after a successful login, with the OAuth token that login used.
///
/// Kept separate from the login use case: signing in and importing guilds are
/// different jobs with different failure modes, and a Discord hiccup while
/// listing guilds must not fail the sign-in itself.
#[derive(Debug, Clone)]
pub struct SyncGuildsCommand {
    pub user_id: Uuid,
    pub discord_access_token: String,
}

/// What the sync produced, so the client knows where to send the user next.
#[derive(Debug, Clone, Default)]
pub struct GuildSyncOutcome {
    /// Guilds the bot is in and where this user holds authority.
    pub manageable: Vec<u64>,
    /// Guilds the bot is in but where this user holds no authority.
    pub ignored: Vec<u64>,
}

impl GuildSyncOutcome {
    /// True when the dashboard should go straight to a guild instead of the picker.
    pub fn has_manageable_guild(&self) -> bool {
        !self.manageable.is_empty()
    }
}

pub struct SyncGuildsUseCase<O, G, M, R, C> {
    oauth: O,
    guilds: G,
    members: M,
    roles: R,
    clock: C,
}

impl<O, G, M, R, C> SyncGuildsUseCase<O, G, M, R, C>
where
    O: DiscordOAuthClient,
    G: GuildRepository,
    M: GuildMemberRepository,
    R: GuildRoleRepository,
    C: Clock,
{
    pub fn new(oauth: O, guilds: G, members: M, roles: R, clock: C) -> Self {
        Self {
            oauth,
            guilds,
            members,
            roles,
            clock,
        }
    }

    /// Imports the guilds this user may administer, creating membership rows.
    ///
    /// The bot's own guild list — the `guilds` table, maintained by the bot's
    /// join and leave events — is the source of truth for "is the bot here".
    /// Discord returns every guild the *user* belongs to, including ones the bot
    /// was never invited to, and offering those in the dashboard would produce a
    /// settings page that cannot do anything.
    pub async fn execute(&self, command: SyncGuildsCommand) -> Result<GuildSyncOutcome, AuthError> {
        let now = self.clock.now();

        let discord_guilds = self
            .oauth
            .fetch_guilds(&command.discord_access_token)
            .await
            .map_err(|error| AuthError::DiscordUnavailable(error.to_string()))?;

        let bot_guilds = self.guilds.list_active().await?;
        let bot_guild_ids: HashSet<u64> = bot_guilds.iter().map(|guild| guild.id).collect();

        let mut outcome = GuildSyncOutcome::default();

        for guild in &discord_guilds {
            if !bot_guild_ids.contains(&guild.id) {
                debug!(guild_id = guild.id, "skipping a guild the bot is not in");
                continue;
            }

            if !has_manage_guild(guild.permissions) {
                outcome.ignored.push(guild.id);
                continue;
            }

            let role_id = self.default_role_for(guild).await?;

            self.members
                .upsert_from_discord(NewGuildMember {
                    id: Uuid::new_v4(),
                    guild_id: guild.id,
                    user_id: command.user_id,
                    guild_role_id: role_id,
                    discord_permissions: guild.permissions,
                    discord_nick: None,
                })
                .await?;

            outcome.manageable.push(guild.id);
        }

        info!(
            user_id = %command.user_id,
            manageable = outcome.manageable.len(),
            ignored = outcome.ignored.len(),
            "guild sync complete"
        );

        let _ = now;
        Ok(outcome)
    }

    /// The role a member gets the first time they are synced into a guild.
    ///
    /// A guild owner or a Discord administrator lands on full authority; everyone
    /// else on the read-only role. Anything else either hands out write access
    /// too freely or makes the dashboard useless on a first visit.
    async fn default_role_for(&self, guild: &DiscordGuildSummary) -> Result<Uuid, AuthError> {
        let is_admin = guild.owner || guild.permissions & DISCORD_ADMINISTRATOR != 0;

        // A guild that has no admin role yet falls through to the viewer role
        // rather than failing: the point is to get the member into the dashboard,
        // and `bootstrap` creates the missing roles on its next run.
        if is_admin && let Some(role) = self.roles.find_by_slug(guild.id, slugs::ADMIN).await? {
            return Ok(role.id);
        }

        if let Some(role) = self.roles.find_by_slug(guild.id, slugs::VIEWER).await? {
            return Ok(role.id);
        }

        // A guild that predates bootstrap has no system roles; create the one we
        // need rather than failing the sign-in.
        let created = self
            .roles
            .create(
                guild.id,
                slugs::VIEWER,
                "Viewer",
                Some("Can see the dashboard but change nothing"),
                true,
                0,
            )
            .await?;

        let permissions: Vec<String> = default_role_permissions(slugs::VIEWER)
            .iter()
            .map(|name| name.to_string())
            .collect();
        self.roles
            .replace_role_permissions(created.id, &permissions)
            .await?;

        Ok(created.id)
    }
}
