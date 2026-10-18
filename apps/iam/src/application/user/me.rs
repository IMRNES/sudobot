use tracing::{debug, info};
use uuid::Uuid;

use crate::domain::guild::GuildRepository;
use crate::domain::guild_member::GuildMemberRepository;
use crate::domain::rbac::GuildRoleRepository;
use crate::domain::user::{User, UserError, UserRepository};

/// One guild in the picker the dashboard shows after sign-in.
#[derive(Debug, Clone)]
pub struct MyGuildView {
    pub id: u64,
    pub name: String,
    pub icon_url: Option<String>,
    pub member_count: i32,
    pub bot_present: bool,
    /// The dashboard role this user holds here, for the badge beside the name.
    pub role_slug: String,
    pub role_name: String,
    /// The permission names that role grants, so the client can hide the screens
    /// this user cannot use instead of rendering them and collecting 403s.
    pub permissions: Vec<String>,
}

/// The signed-in account and every guild it can act in.
#[derive(Debug, Clone)]
pub struct MeView {
    pub user: User,
    pub guilds: Vec<MyGuildView>,
}

/// Answers "who am I, and where can I go?" — the first call the dashboard makes.
///
/// The guild list comes from memberships, not from Discord: a guild the bot is
/// not installed in has no settings to show, and one the user cannot administer
/// would be a page of buttons that all fail.
pub struct GetMeUseCase<U, G, M, R> {
    users: U,
    guilds: G,
    members: M,
    roles: R,
}

impl<U, G, M, R> GetMeUseCase<U, G, M, R>
where
    U: UserRepository,
    G: GuildRepository,
    M: GuildMemberRepository,
    R: GuildRoleRepository,
{
    pub fn new(users: U, guilds: G, members: M, roles: R) -> Self {
        Self {
            users,
            guilds,
            members,
            roles,
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<MeView, UserError> {
        let user = self
            .users
            .find_by_id(user_id)
            .await?
            .ok_or_else(|| UserError::not_found_for(user_id))?;

        // One query for every membership, and the role it points at travels with
        // it — that is what keeps the role slug and name off the per-guild path.
        let memberships = self.members.list_for_user(user_id).await?;

        let mut guilds = Vec::with_capacity(memberships.len());

        for membership in memberships {
            let Some(guild) = self.guilds.find_by_id(membership.member.guild_id).await? else {
                // The bot was removed and the membership row outlived it. Skipping
                // is the honest answer: there is nothing to administer.
                debug!(
                    guild_id = membership.member.guild_id,
                    user_id = %user_id,
                    "skipping a membership whose guild is gone"
                );
                continue;
            };

            // The permission names are resolved per guild here. A future
            // optimisation is a single joined query returning every guild with its
            // matrix inlined; it is not done today because the repository contract
            // is per-guild and a dashboard sign-in is not a hot path.
            let permissions = self
                .roles
                .permissions_for_member(guild.id, user_id)
                .await?
                .iter()
                .map(str::to_string)
                .collect();

            guilds.push(MyGuildView {
                id: guild.id,
                name: guild.name,
                icon_url: guild.icon_url,
                member_count: guild.member_count,
                bot_present: guild.bot_present,
                role_slug: membership.role.slug,
                role_name: membership.role.name,
                permissions,
            });
        }

        info!(
            user_id = %user.id,
            guilds = guilds.len(),
            "resolved the signed-in account"
        );

        Ok(MeView { user, guilds })
    }
}
