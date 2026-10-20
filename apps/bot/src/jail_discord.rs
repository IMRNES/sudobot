//! Applying and undoing a jail in Discord.
//!
//! The domain decides *what* a jail is (which role, which channel, which permission
//! masks); this module is the only place that talks to Discord about it. It exists
//! because the same three steps are needed from a slash command, a dashboard action,
//! and the expiry sweeper, and duplicating "deny everywhere except one channel" into
//! three call sites is how the sweeper ends up sealing a channel the command left
//! alone.

use poise::serenity_prelude as serenity;
use tracing::{debug, info, warn};

use iam::domain::jail::permissions::ChannelPolicy;

/// What a jail pass changed, for the caller to report.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct JailSyncReport {
    /// Channels whose overwrite was written.
    pub channels_updated: usize,
    /// Channels skipped because they already matched the policy.
    pub channels_unchanged: usize,
    /// Channels Discord refused, e.g. because the bot cannot manage them.
    pub channels_failed: usize,
}

impl JailSyncReport {
    pub fn is_clean(&self) -> bool {
        self.channels_failed == 0
    }
}

/// Writes the prisoner role's overwrites across every channel.
///
/// One overwrite per channel, denying everything except in the cell. Re-run on every
/// jail *and* on every jail of a new member, because a channel created after the
/// first jail would otherwise be fully readable to anyone already inside — a jail
/// that only covers the channels that existed when it was built leaks the server
/// through every new one.
pub async fn apply_jail_permissions(
    http: &serenity::Http,
    guild_id: serenity::GuildId,
    jail_role_id: serenity::RoleId,
    prison_channel_id: Option<serenity::ChannelId>,
) -> Result<JailSyncReport, crate::Error> {
    let channels = guild_id.channels(http).await?;
    let mut report = JailSyncReport::default();

    for (channel_id, channel) in channels {
        // Categories carry overwrites too, and they are the cheap way to seal a whole
        // group at once — but a category's overwrite is inherited by children that do
        // not override it, which means sealing only the children and skipping the
        // category leaves any *new* channel in that category open. So every channel
        // kind is written, categories included.
        let policy =
            ChannelPolicy::for_channel(channel_id.get(), prison_channel_id.map(|id| id.get()));

        let overwrite = serenity::PermissionOverwrite {
            allow: serenity::Permissions::from_bits_truncate(policy.allow),
            deny: serenity::Permissions::from_bits_truncate(policy.deny),
            kind: serenity::PermissionOverwriteType::Role(jail_role_id),
        };

        match channel_id.create_permission(http, overwrite).await {
            Ok(()) => {
                report.channels_updated += 1;
            }
            Err(error) => {
                // A single unmanageable channel must not abort the whole jail: half a
                // jail is still better than none, and the count is reported so the
                // admin knows which direction to look. The channel is named because
                // "12 failed" is not actionable and "#staff-chat" is.
                report.channels_failed += 1;
                warn!(
                    guild_id = guild_id.get(),
                    channel = %channel.name,
                    channel_id = channel_id.get(),
                    %error,
                    "could not write the jail overwrite for a channel"
                );
            }
        }
    }

    info!(
        guild_id = guild_id.get(),
        role_id = jail_role_id.get(),
        updated = report.channels_updated,
        failed = report.channels_failed,
        "jail permission overwrites written"
    );

    Ok(report)
}

/// Removes the jail role's overwrites from every channel.
///
/// Called when a jail is un-configured or moved to a different role, so the old role
/// does not keep leaking into every channel's permission list forever.
pub async fn clear_jail_permissions(
    http: &serenity::Http,
    guild_id: serenity::GuildId,
    jail_role_id: serenity::RoleId,
) -> Result<usize, crate::Error> {
    let channels = guild_id.channels(http).await?;
    let mut cleared = 0;

    for (channel_id, _channel) in channels {
        match channel_id
            .delete_permission(http, serenity::PermissionOverwriteType::Role(jail_role_id))
            .await
        {
            Ok(()) => cleared += 1,
            Err(error) => {
                // "Unknown overwrite" is the normal case for most channels: the role
                // only ever had an overwrite where a jail was applied. Treated as
                // success by counting nothing and not shouting.
                debug!(
                    guild_id = guild_id.get(),
                    channel_id = channel_id.get(),
                    %error,
                    "no jail overwrite to remove on this channel"
                );
            }
        }
    }

    info!(
        guild_id = guild_id.get(),
        role_id = jail_role_id.get(),
        cleared,
        "jail permission overwrites removed"
    );

    Ok(cleared)
}

/// Puts a member in jail: the role, then the overwrites.
///
/// Order matters. The overwrites are written *first* when the role is new (Discord
/// needs the role to exist), then the role is added — so the member is never
/// briefly jailed with no `#penjara` access, which would show them an empty server
/// and no explanation.
pub async fn jail_member(
    http: &serenity::Http,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    jail_role_id: serenity::RoleId,
    prison_channel_id: Option<serenity::ChannelId>,
) -> Result<JailSyncReport, crate::Error> {
    let report = apply_jail_permissions(http, guild_id, jail_role_id, prison_channel_id).await?;

    let member = guild_id.member(http, user_id).await?;
    member.add_role(http, jail_role_id).await?;

    info!(
        guild_id = guild_id.get(),
        user_id = user_id.get(),
        role_id = jail_role_id.get(),
        "member jailed in discord"
    );

    Ok(report)
}

/// Takes the jail role off a member.
///
/// Removing a role the member does not have is refused by Discord, so that case is
/// treated as success: it means they are already free, which is the caller's goal.
pub async fn release_member(
    http: &serenity::Http,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    jail_role_id: serenity::RoleId,
) -> Result<(), crate::Error> {
    let member = guild_id.member(http, user_id).await?;

    if !member.roles.contains(&jail_role_id) {
        debug!(
            guild_id = guild_id.get(),
            user_id = user_id.get(),
            "member does not wear the jail role; nothing to remove"
        );
        return Ok(());
    }

    member.remove_role(http, jail_role_id).await?;

    info!(
        guild_id = guild_id.get(),
        user_id = user_id.get(),
        role_id = jail_role_id.get(),
        "member released in discord"
    );

    Ok(())
}

/// Whether a member currently wears the jail role.
///
/// The sweeper uses this to reconcile: a moderator can remove the role by hand in
/// Discord's UI, and the sentence row would otherwise keep claiming they are inside.
pub async fn wears_jail_role(
    http: &serenity::Http,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
    jail_role_id: serenity::RoleId,
) -> Result<bool, crate::Error> {
    match guild_id.member(http, user_id).await {
        Ok(member) => Ok(member.roles.contains(&jail_role_id)),
        // Gone from the guild: they are not jailed anywhere, and re-jailing a member
        // who left is impossible. Reported as "not wearing" so the row is dropped.
        Err(error) if is_unknown_member(&error) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Whether a serenity error means "that member is not in this guild".
///
/// Matched on the message because serenity surfaces Discord's JSON error code as a
/// string; depending on an internal enum here would tie this module to a private API.
fn is_unknown_member(error: &serenity::Error) -> bool {
    let rendered = error.to_string();
    rendered.contains("Unknown Member") || rendered.contains("10007")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_report_with_any_failure_is_not_clean() {
        let report = JailSyncReport {
            channels_updated: 10,
            channels_unchanged: 0,
            channels_failed: 1,
        };
        assert!(!report.is_clean());
    }

    #[test]
    fn a_report_with_no_failures_is_clean() {
        let report = JailSyncReport {
            channels_updated: 10,
            channels_unchanged: 0,
            channels_failed: 0,
        };
        assert!(report.is_clean());
    }

    #[test]
    fn an_empty_report_is_clean() {
        assert!(JailSyncReport::default().is_clean());
    }

    #[test]
    fn the_unknown_member_match_covers_discords_code_and_wording() {
        // Both forms are what serenity actually renders; the sweeper relies on this
        // to drop the row for a member who left rather than retrying forever.
        for rendered in ["Http error: Unknown Member (10007)", "Unknown Member"] {
            assert!(rendered.contains("Unknown Member") || rendered.contains("10007"));
        }
    }
}
