//! Giving a newly joined guild its system roles.
//!
//! The dashboard has nothing to show until these exist: without an admin role,
//! nobody — including the person who just installed the bot — can open a settings
//! page. So the roles are seeded on join rather than waiting for the first login.

use tracing::debug;

use iam::domain::rbac::{GuildRoleRepository, default_role_permissions, slugs};

use crate::data::Data;

/// Creates the three system roles for a guild, skipping any that already exist.
///
/// Idempotent: a rejoin, or a guild that predates the bot's own record of it, must
/// not end up with two "Admin" roles. An admin's rename of a system role is also
/// preserved, because the slug is what is checked and it never changes.
pub async fn seed_system_roles(
    data: &Data,
    guild_id: u64,
) -> Result<(), iam::domain::errors::RepositoryError> {
    for (position, slug) in slugs::SYSTEM_SLUGS.iter().enumerate() {
        if data
            .guild_roles
            .find_by_slug(guild_id, slug)
            .await?
            .is_some()
        {
            continue;
        }

        let (name, description) = match *slug {
            slugs::ADMIN => ("Admin", "Full control of this server's dashboard"),
            slugs::MODERATOR => ("Moderator", "Day-to-day management of members and content"),
            _ => ("Viewer", "Can see the dashboard but change nothing"),
        };

        let role = data
            .guild_roles
            .create(
                guild_id,
                slug,
                name,
                Some(description),
                true,
                position as i32 * 10,
            )
            .await?;

        let granted: Vec<String> = default_role_permissions(slug)
            .iter()
            .map(|permission| permission.to_string())
            .collect();

        data.guild_roles
            .replace_role_permissions(role.id, &granted)
            .await?;

        debug!(guild_id, slug, "seeded a system role");
    }

    Ok(())
}
