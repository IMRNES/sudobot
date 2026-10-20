//! The permission policy a jail applies.
//!
//! Discord permission bits are a wire constant, like the decimal RGB an embed colour
//! uses, so the numbers live here rather than being imported from a Discord library
//! — the domain must not depend on `serenity`, and the bot maps these to its own
//! `Permissions` type at the edge.
//!
//! The policy is deliberately *two* masks rather than one. A jail that only denies
//! access to normal channels is one configuration mistake away from a member who can
//! see nothing at all, including the channel they are supposed to sit in and be told
//! why. So the prison channel gets an explicit allow, and the tests pin both halves.

/// Bits denied on every channel except the prison.
///
/// `VIEW_CHANNEL` is the one that does the work — without it the channel does not
/// exist for the member — but the rest are denied as well. Relying on invisibility
/// alone means a single stale allow overwrite elsewhere would silently restore
/// talking privileges, and a jail that only *looks* sealed is worse than none.
pub const JAIL_DENY: u64 = VIEW_CHANNEL
    | SEND_MESSAGES
    | SEND_MESSAGES_IN_THREADS
    | CREATE_PUBLIC_THREADS
    | CREATE_PRIVATE_THREADS
    | ADD_REACTIONS
    | USE_EXTERNAL_EMOJIS
    | CREATE_INSTANT_INVITE
    | CONNECT
    | SPEAK
    | STREAM
    | USE_APPLICATION_COMMANDS;

/// Bits allowed in the prison channel.
///
/// Deliberately narrow: read, write, and react. No threads, no invites, no voice —
/// a prisoner who can open a thread in the prison channel has a private room.
pub const PRISON_ALLOW: u64 = VIEW_CHANNEL | READ_MESSAGE_HISTORY | SEND_MESSAGES | ADD_REACTIONS;

/// Bits denied *inside* the prison channel itself.
///
/// `VIEW_CHANNEL` must not appear here or the member cannot see the cell they are in.
pub const PRISON_DENY: u64 = CREATE_PUBLIC_THREADS
    | CREATE_PRIVATE_THREADS
    | CREATE_INSTANT_INVITE
    | CONNECT
    | SPEAK
    | STREAM;

/* -------------------------------------------------------------------------- */
/* Discord permission bits                                                     */
/* -------------------------------------------------------------------------- */

/// Discord's documented permission values.
pub const CREATE_INSTANT_INVITE: u64 = 1 << 0;
pub const ADD_REACTIONS: u64 = 1 << 6;
pub const STREAM: u64 = 1 << 9;
pub const VIEW_CHANNEL: u64 = 1 << 10;
pub const SEND_MESSAGES: u64 = 1 << 11;
pub const USE_EXTERNAL_EMOJIS: u64 = 1 << 18;
pub const CONNECT: u64 = 1 << 20;
pub const SPEAK: u64 = 1 << 21;
pub const USE_APPLICATION_COMMANDS: u64 = 1 << 31;
pub const CREATE_PUBLIC_THREADS: u64 = 1 << 34;
pub const CREATE_PRIVATE_THREADS: u64 = 1 << 36;
pub const READ_MESSAGE_HISTORY: u64 = 1 << 16;
pub const SEND_MESSAGES_IN_THREADS: u64 = 1 << 38;

/// What to write on one channel for the prisoner role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelPolicy {
    /// Permissions to grant.
    pub allow: u64,
    /// Permissions to take away.
    pub deny: u64,
}

impl ChannelPolicy {
    /// The policy for the cell: visible, writable, nothing else.
    pub fn prison() -> Self {
        Self {
            allow: PRISON_ALLOW,
            deny: PRISON_DENY,
        }
    }

    /// The policy for every other channel: sealed.
    pub fn sealed() -> Self {
        Self {
            allow: 0,
            deny: JAIL_DENY,
        }
    }

    /// The policy for a given channel, by whether it is the prison.
    ///
    /// One function so the two branches cannot drift, and so a caller cannot pass
    /// the prison id by accident the way a boolean parameter invites.
    pub fn for_channel(channel_id: u64, prison_channel_id: Option<u64>) -> Self {
        if prison_channel_id == Some(channel_id) {
            Self::prison()
        } else {
            Self::sealed()
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn every_channel_except_the_prison_is_sealed() {
        let prison = Some(999u64);
        assert_eq!(
            ChannelPolicy::for_channel(1, prison),
            ChannelPolicy::sealed()
        );
        assert_eq!(
            ChannelPolicy::for_channel(999, prison),
            ChannelPolicy::prison()
        );
    }

    #[test]
    fn sealed_channels_hide_themselves() {
        assert_ne!(
            ChannelPolicy::sealed().deny & VIEW_CHANNEL,
            0,
            "without VIEW_CHANNEL the jail is only advisory"
        );
    }

    #[test]
    fn the_prison_stays_visible_and_writable() {
        let cell = ChannelPolicy::prison();
        for bit in [VIEW_CHANNEL, SEND_MESSAGES, READ_MESSAGE_HISTORY] {
            assert_ne!(
                cell.allow & bit,
                0,
                "a prisoner must be able to use their cell"
            );
            assert_eq!(
                cell.deny & bit,
                0,
                "denying a bit that is also allowed leaves the outcome to Discord's precedence rules"
            );
        }
    }

    #[test]
    fn the_prison_is_not_a_private_room() {
        let cell = ChannelPolicy::prison();
        assert_eq!(
            cell.allow & CREATE_PRIVATE_THREADS,
            0,
            "threads would give a prisoner an unmonitored channel"
        );
        assert_ne!(cell.deny & CREATE_PRIVATE_THREADS, 0);
    }

    #[test]
    fn no_channel_is_both_allowed_and_denied_for_the_same_bit() {
        // An allow and a deny of the same bit at the same overwrite level is
        // resolved by Discord's precedence rules, not by intent. Keeping the masks
        // disjoint means the outcome is whatever we wrote, not whatever wins.
        let sealed = ChannelPolicy::sealed();
        assert_eq!(
            sealed.allow & sealed.deny,
            0,
            "sealed policy overlaps itself"
        );

        let cell = ChannelPolicy::prison();
        assert_eq!(cell.allow & cell.deny, 0, "prison policy overlaps itself");
    }

    #[test]
    fn permission_bits_match_discords_documented_values() {
        // These are the numbers Discord's docs list. A typo here would silence a
        // prisoner's ability to write in their cell, or fail to seal a channel, and
        // both look like "Discord ignored the overwrite".
        assert_eq!(VIEW_CHANNEL, 1024);
        assert_eq!(SEND_MESSAGES, 2048);
        assert_eq!(ADD_REACTIONS, 64);
        assert_eq!(READ_MESSAGE_HISTORY, 65536);
        assert_eq!(CONNECT, 1_048_576);
        assert_eq!(SPEAK, 2_097_152);
    }
}
