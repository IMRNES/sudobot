use tracing::info;
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::auth::AuthError;
use crate::domain::guild::GuildRepository;
use crate::domain::guild_member::{GuildMemberRepository, NewGuildMember};
use crate::domain::rbac::{GuildRoleRepository, default_role_permissions, slugs};

/// The one guild the dashboard serves, as the client sees it.
///
/// Kept for the picker's shape, which the dashboard already renders: a list of one
/// entry means the existing UI works unchanged and the "no servers" state can no
/// longer occur for a signed-in user.
#[derive(Debug, Clone)]
pub struct GuildView {
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    pub member_count: i32,
    pub bot_present: bool,
    pub role_slug: String,
    pub role_name: String,
    pub permissions: Vec<String>,
}

/// Records that a whitelisted user belongs to the dashboard's guild.
///
/// Replaces the old guild-import use case, which asked Discord for every guild the
/// caller was in and filtered by their Discord permissions. That whole question is
/// gone: the guild is configuration now, and the caller arrived here already
/// proven to hold a whitelisted role — see
/// [`DiscordLoginUseCase`](crate::application::auth::DiscordLoginUseCase).
///
/// What remains is bookkeeping: make sure the guild row and its system roles exist,
/// then point a membership row at the admin role.
pub struct DashboardMembershipUseCase<G, M, R, C> {
    guilds: G,
    members: M,
    roles: R,
    clock: C,
}

impl<G, M, R, C> DashboardMembershipUseCase<G, M, R, C>
where
    G: GuildRepository,
    M: GuildMemberRepository,
    R: GuildRoleRepository,
    C: Clock,
{
    pub fn new(guilds: G, members: M, roles: R, clock: C) -> Self {
        Self {
            guilds,
            members,
            roles,
            clock,
        }
    }

    /// Upserts the caller's membership and returns the single guild to show.
    pub async fn execute(&self, user_id: Uuid, guild_id: u64) -> Result<GuildView, AuthError> {
        let Some(guild) = self.guilds.find_by_id(guild_id).await? else {
            // The bot has never seen this guild, so `DASHBOARD_GUILD_ID` names
            // something the deployment is not actually in. A missing invitation,
            // not a permission problem — and saying so beats a generic refusal.
            return Err(AuthError::DashboardGuildUnavailable(guild_id));
        };

        let role = self.admin_role(guild_id).await?;

        self.members
            .upsert_from_discord(NewGuildMember {
                id: Uuid::new_v4(),
                guild_id,
                user_id,
                guild_role_id: role.id,
                // Zero, deliberately: access came from a whitelisted Discord role,
                // not from a Discord permission bitfield, and this column is only
                // ever displayed. Writing `DISCORD_ADMINISTRATOR` here would make
                // `is_discord_admin` assert something Discord never reported.
                discord_permissions: 0,
                discord_nick: None,
            })
            .await?;

        let permissions = self
            .roles
            .permissions_for_member(guild_id, user_id)
            .await?
            .iter()
            .map(str::to_string)
            .collect();

        let _ = self.clock.now();

        info!(user_id = %user_id, guild_id, "dashboard membership recorded");

        Ok(GuildView {
            id: guild.id,
            name: guild.name,
            icon_url: guild.icon_url,
            member_count: guild.member_count,
            bot_present: guild.bot_present,
            role_slug: role.slug,
            role_name: role.name,
            permissions,
        })
    }

    /// The admin role, created with its default matrix if `bootstrap` has not run.
    ///
    /// Self-healing rather than an error: a guild that predates the seeding would
    /// otherwise make the dashboard unusable for staff who are provably allowed in.
    async fn admin_role(&self, guild_id: u64) -> Result<crate::domain::rbac::GuildRole, AuthError> {
        if let Some(role) = self.roles.find_by_slug(guild_id, slugs::ADMIN).await? {
            return Ok(role);
        }

        let created = self
            .roles
            .create(
                guild_id,
                slugs::ADMIN,
                "Admin",
                Some("Full dashboard access"),
                true,
                0,
            )
            .await?;

        let permissions: Vec<String> = default_role_permissions(slugs::ADMIN)
            .iter()
            .map(|name| (*name).to_string())
            .collect();
        self.roles
            .replace_role_permissions(created.id, &permissions)
            .await?;

        Ok(created)
    }
}
