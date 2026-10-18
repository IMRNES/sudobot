//! Seeds the data a fresh installation needs and nothing else.
//!
//! Run once after `migrate up`, and safe to run again: every step checks before it
//! writes, so a redeploy that runs it out of habit changes nothing. It creates the
//! permission catalog, promotes the first platform administrator, and gives every
//! guild the bot has already joined its three system roles.
//!
//! ```text
//! cargo run -p bootstrap
//! ```

use anyhow::{Context, Result};
use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use std::time::Duration;
use tracing::{info, warn};

use iam::domain::rbac::{default_role_permissions, names, slugs};
use iam::infrastructure::repository::{
    SeaOrmGuildRepository, SeaOrmGuildRoleRepository, SeaOrmPermissionRepository,
    SeaOrmUserRepository,
};

#[tokio::main]
async fn main() -> Result<()> {
    let config = sudobot_config::load_or_exit();
    config.init_tracing()?;

    let db = connect(&config).await?;
    let users = SeaOrmUserRepository::new(db.clone());
    let permissions = SeaOrmPermissionRepository::new(db.clone());
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let roles = SeaOrmGuildRoleRepository::new(db.clone());

    let catalog = seed_permissions(&permissions).await?;
    info!(count = catalog, "permission catalog is in place");

    seed_super_admin(&users).await?;

    let guild_count = seed_guild_roles(&guilds, &roles).await?;
    info!(guilds = guild_count, "system roles are in place");

    info!("bootstrap complete");
    Ok(())
}

/// Opens a connection for the seeding run.
async fn connect(config: &sudobot_config::AppConfig) -> Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(config.database.url.clone());
    options
        .max_connections(2)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(config.database.acquire_timeout_secs))
        .sqlx_logging(false);

    SeaDatabase::connect(options)
        .await
        .with_context(|| format!("connecting to {}", config.database.redacted_url()))
}

/// Writes the permission catalog.
///
/// Upserted rather than inserted: a release that adds a permission must be able to
/// re-run this, and the description is allowed to change between versions.
async fn seed_permissions(permissions: &SeaOrmPermissionRepository) -> Result<usize> {
    use iam::domain::rbac::PermissionRepository;

    let mut count = 0;
    for (name, group, description) in names::CATALOG {
        permissions
            .upsert(name, group, description)
            .await
            .with_context(|| format!("seeding permission `{name}`"))?;
        count += 1;
    }

    Ok(count)
}

/// Promotes the configured account to platform administrator.
///
/// The account has to exist first, which means signing in through Discord once.
/// When it does not, this says so and exits successfully — a bootstrap run before
/// the first login is a normal sequence, not a failure.
async fn seed_super_admin(users: &SeaOrmUserRepository) -> Result<()> {
    use iam::domain::user::UserRepository;

    let raw = std::env::var("BOOTSTRAP_SUPER_ADMIN_DISCORD_ID").unwrap_or_default();
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        info!(
            "BOOTSTRAP_SUPER_ADMIN_DISCORD_ID is not set; \
             skipping the platform administrator"
        );
        return Ok(());
    }

    let discord_id: u64 = trimmed.parse().with_context(|| {
        format!("BOOTSTRAP_SUPER_ADMIN_DISCORD_ID `{trimmed}` is not a snowflake")
    })?;

    let Some(user) = users.find_by_discord_id(discord_id).await? else {
        warn!(
            discord_id,
            "that account has not signed in yet; run bootstrap again after its first login"
        );
        return Ok(());
    };

    if user.is_platform_admin() {
        info!(user_id = %user.id, "platform administrator already set");
        return Ok(());
    }

    users.set_super_admin(user.id, true).await?;
    info!(user_id = %user.id, discord_id, "promoted to platform administrator");
    Ok(())
}

/// Gives every recorded guild its three system roles.
///
/// A guild that joined before bootstrap has no roles, and without them nobody can
/// administer anything through the dashboard — including the person who just
/// installed the bot.
async fn seed_guild_roles(
    guilds: &SeaOrmGuildRepository,
    roles: &SeaOrmGuildRoleRepository,
) -> Result<usize> {
    use iam::domain::guild::GuildRepository;
    use iam::domain::rbac::GuildRoleRepository;

    let all = guilds.list_active().await?;
    let mut seeded = 0;

    for guild in all {
        for (position, slug) in slugs::SYSTEM_SLUGS.iter().enumerate() {
            // Skipping an existing role is what makes this idempotent, and it also
            // means an admin's rename survives a redeploy.
            if roles.find_by_slug(guild.id, slug).await?.is_some() {
                continue;
            }

            let (name, description) = match *slug {
                slugs::ADMIN => ("Admin", "Full control of this server's dashboard"),
                slugs::MODERATOR => ("Moderator", "Day-to-day management of members and content"),
                _ => ("Viewer", "Can see the dashboard but change nothing"),
            };

            let role = roles
                .create(
                    guild.id,
                    slug,
                    name,
                    Some(description),
                    true,
                    position as i32 * 10,
                )
                .await
                .with_context(|| format!("seeding role `{slug}` for guild {}", guild.id))?;

            let granted: Vec<String> = default_role_permissions(slug)
                .iter()
                .map(|name| name.to_string())
                .collect();

            roles
                .replace_role_permissions(role.id, &granted)
                .await
                .with_context(|| {
                    format!("granting permissions to `{slug}` in guild {}", guild.id)
                })?;

            seeded += 1;
        }
    }

    Ok(seeded)
}
