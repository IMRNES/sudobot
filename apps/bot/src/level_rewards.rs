//! Turning a [`LevelUpOutcome`] into actual Discord role changes.
//!
//! The use cases decide *which* roles a member should hold; this module is the
//! only place that talks to Discord about them. Keeping it separate matters for
//! two reasons:
//!
//! - The decision is testable without a gateway connection, and it is tested —
//!   `domain::leveling::plan_role_changes`.
//! - Every failure here is an API failure with the same shape (the role is above
//!   the bot's own highest role, or the member left), so they are handled once.
//!
//! Nothing in this module fails a caller. A member who keeps chatting while the
//! role grant fails must still earn XP: the roles are cosmetic and the next
//! message retries implicitly through the reconcile path.

use poise::serenity_prelude as serenity;
use tracing::{debug, warn};

use iam::domain::leveling::LevelUpOutcome;

/// Applies the role changes an outcome implies, best effort.
///
/// Returns how many role edits actually reached Discord, which is what the
/// caller logs. `http` rather than `ctx` because the voice sweeper runs outside a
/// command and has no context to borrow.
pub async fn apply(outcome: &LevelUpOutcome, http: &serenity::Http) -> usize {
    if outcome.roles_to_grant.is_empty() && outcome.roles_to_revoke.is_empty() {
        return 0;
    }

    // Only fetch the member when there is something to change. A levelling member
    // who qualified for nothing they do not already hold is the common case once
    // the ladder is fully worn, and it costs no request.
    let guild_id = serenity::GuildId::new(outcome.profile.guild_id);
    let user_id = serenity::UserId::new(outcome.profile.discord_user_id);

    let member = match guild_id.member(http, user_id).await {
        Ok(member) => member,
        Err(error) => {
            // "Unknown Member" is a member who left between earning the XP and
            // this call, which is ordinary and needs no alarm.
            if is_unknown_member(&error) {
                debug!(
                    guild_id = outcome.profile.guild_id,
                    user_id = outcome.profile.discord_user_id,
                    "member left before their reward roles could be applied"
                );
            } else {
                warn!(
                    guild_id = outcome.profile.guild_id,
                    user_id = outcome.profile.discord_user_id,
                    %error,
                    "could not read a member to apply their reward roles"
                );
            }
            return 0;
        }
    };

    let mut applied = 0;

    for role_id in &outcome.roles_to_revoke {
        let role = serenity::RoleId::new(*role_id);

        if member.roles.contains(&role) {
            match member.remove_role(http, role).await {
                Ok(()) => {
                    applied += 1;
                    debug!(
                        guild_id = outcome.profile.guild_id,
                        user_id = outcome.profile.discord_user_id,
                        role_id = *role_id,
                        "removed a level reward role"
                    );
                }
                Err(error) => {
                    warn!(
                        guild_id = outcome.profile.guild_id,
                        user_id = outcome.profile.discord_user_id,
                        role_id = *role_id,
                        %error,
                        "could not remove a level reward role"
                    );
                }
            }
        }
    }

    for role_id in &outcome.roles_to_grant {
        let role = serenity::RoleId::new(*role_id);

        // A role already worn is skipped rather than re-added: the API would accept
        // it, but it is a pointless request and it pollutes the audit log.
        if member.roles.contains(&role) {
            continue;
        }

        match member.add_role(http, role).await {
            Ok(()) => {
                applied += 1;
                debug!(
                    guild_id = outcome.profile.guild_id,
                    user_id = outcome.profile.discord_user_id,
                    level = outcome.profile.level,
                    role_id = *role_id,
                    "granted a level reward role"
                );
            }
            Err(error) => {
                // The realistic cause is a role positioned above the bot's own
                // highest role, which only a server admin can fix — so the message
                // says so instead of leaving a silent hole in the ladder.
                warn!(
                    guild_id = outcome.profile.guild_id,
                    user_id = outcome.profile.discord_user_id,
                    role_id = *role_id,
                    %error,
                    "could not grant a level reward role; it may sit above the bot's own role"
                );
            }
        }
    }

    applied
}

/// Whether a serenity error means "that member is not in this guild".
///
/// Matched on the rendered message because serenity surfaces Discord's JSON error
/// code as a string here; the same test the autorole sweeper uses, and for the
/// same reason — the alternative is depending on an internal enum that is not
/// public.
fn is_unknown_member(error: &serenity::Error) -> bool {
    let rendered = error.to_string();
    rendered.contains("Unknown Member") || rendered.contains("10007")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The matcher reads the rendered message, so these cases drive it through
    /// `Display` exactly as a real `Http` error would be read.
    #[test]
    fn unknown_member_is_recognised_by_name_and_by_code() {
        // Discord's own wording, which is what `add_member_role` produces for a
        // member who left.
        assert!(is_unknown_member(&serenity::Error::Other("Unknown Member")));

        // And the bare error code, in case the wording is ever localised or
        // rephrased by a serenity update.
        assert!(is_unknown_member(&serenity::Error::Other(
            "Invalid Form Body (10007)"
        )));
    }

    #[test]
    fn an_unrelated_failure_is_not_mistaken_for_a_departure() {
        // A role above the bot's own role must be reported, not filed as "they
        // left" — that is the difference between an admin being told to move the
        // bot's role up and nobody ever being told.
        assert!(!is_unknown_member(&serenity::Error::Other(
            "Missing Permissions"
        )));
        assert!(!is_unknown_member(&serenity::Error::Other("Unknown Role")));
        assert!(!is_unknown_member(&serenity::Error::Other(
            "something else entirely"
        )));
    }
}
