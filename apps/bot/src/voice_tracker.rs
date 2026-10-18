//! Which members are in voice, and since when.
//!
//! Discord reports a voice *state*, not voice *events*: a `VOICE_STATE_UPDATE`
//! arrives for every change — joining, leaving, moving channel, muting,
//! deafening, streaming, even the server-side suppression flip — and the payload
//! describes the new state rather than the transition. Turning that into "this
//! member spent 43 minutes in voice" is what this module does.
//!
//! Sessions are held in memory rather than in a table on purpose. A voice session
//! only has meaning while the gateway is up: the bot is the only observer, and a
//! restart means the session boundary it was tracking is gone. The alternative —
//! persisting an open session row and paying it out on reboot — would have to
//! decide how to bill a crash, and inventing that answer is worse than losing one
//! session of XP for whoever happened to be talking at the moment of a deploy.
//!
//! The tracker is keyed by `(guild, member)`. A member can only be in one voice
//! channel per guild at a time, so that pair is the natural key and a second entry
//! for the same pair would be a bug rather than something to store.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

/// One member's open voice session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Session {
    /// The channel they are in, so a move can be told apart from a mute. Without
    /// it, moving channel would look like an unchanged session and the time would
    /// never be paid.
    channel_id: u64,
    joined_at: DateTime<Utc>,
}

/// Tracks open voice sessions across gateway events.
#[derive(Debug, Default)]
pub struct VoiceTracker {
    /// A plain `Mutex`, not an async one: every critical section here is a couple
    /// of map lookups with no `await` inside, so the lock is never held across a
    /// yield point and a `parking_lot`-style async mutex would buy nothing.
    sessions: Mutex<HashMap<(u64, u64), Session>>,
}

impl VoiceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records that a member is in `channel_id` as of `now`.
    ///
    /// Idempotent for a member already tracked in the same channel, which is what
    /// makes a mute or a deafen harmless: those arrive as state updates for a
    /// session that has not changed, and they must not restart the clock.
    pub fn open(&self, guild_id: u64, user_id: u64, channel_id: Option<u64>, now: DateTime<Utc>) {
        let Some(channel_id) = channel_id else {
            // Not in voice. `close` is the call that pays a session out; this is
            // the defensive path for a caller that skips it.
            return;
        };

        let mut sessions = self.lock();

        match sessions.get(&(guild_id, user_id)) {
            // Already tracked in this channel: leave the original `joined_at`
            // alone. Overwriting it here is exactly how a member who mutes
            // themselves repeatedly would never earn any voice XP.
            Some(existing) if existing.channel_id == channel_id => {}

            // A different channel is a move. The caller is expected to have closed
            // the old session first; replacing it is the safe fallback, and it
            // still pays nothing rather than double-paying.
            _ => {
                sessions.insert(
                    (guild_id, user_id),
                    Session {
                        channel_id,
                        joined_at: now,
                    },
                );
            }
        }
    }

    /// Ends a member's session and returns how many whole seconds it lasted.
    ///
    /// `None` when there was no session to end, which is the ordinary answer for
    /// an update the tracker never opened (the member was already in voice when
    /// the bot started, or a leave arrives twice). `Some(0)` is a real zero-length
    /// session, and the caller's own `seconds <= 0` guard drops it.
    pub fn close(&self, guild_id: u64, user_id: u64, now: DateTime<Utc>) -> Option<i64> {
        let session = self.lock().remove(&(guild_id, user_id))?;

        // Signed and clamped at zero: a clock that steps backwards (NTP, a host
        // resume) must not produce a negative duration, which would subtract XP.
        Some(
            now.signed_duration_since(session.joined_at)
                .num_seconds()
                .max(0),
        )
    }

    /// Forgets a member's session without paying it out.
    ///
    /// For a member who left the guild: their voice session is over, and the XP is
    /// not owed to someone who is no longer there to receive it.
    pub fn forget(&self, guild_id: u64, user_id: u64) {
        self.lock().remove(&(guild_id, user_id));
    }

    /// How many sessions are open, for the readiness log and tests.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A poisoned lock is recovered rather than propagated: the data behind it is a
    /// map of timestamps, and refusing to track voice forever because one task
    /// panicked while holding it would be a worse outcome than reading a map that
    /// is still structurally sound.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<(u64, u64), Session>> {
        self.sessions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_780_000_000 + seconds, 0).expect("a valid timestamp")
    }

    #[test]
    fn a_session_is_paid_its_whole_duration() {
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        let seconds = tracker.close(7, 1, at(3_600)).expect("a session was open");

        assert_eq!(seconds, 3_600);
        assert!(tracker.is_empty(), "the session is gone once paid");
    }

    #[test]
    fn a_mute_does_not_restart_the_clock() {
        // The bug this guards: Discord sends an update on mute. If `open` reset
        // `joined_at` every time, a member who toggles mute would earn nothing.
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        tracker.open(7, 1, Some(100), at(600)); // they muted themselves at 10:00
        tracker.open(7, 1, Some(100), at(1_200)); // and unmuted at 20:00

        let seconds = tracker.close(7, 1, at(3_600)).expect("a session was open");
        assert_eq!(seconds, 3_600, "the original join time stands");
    }

    #[test]
    fn moving_channel_starts_a_new_session() {
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        // The caller closes before reopening, as the event handler does.
        assert_eq!(tracker.close(7, 1, at(300)), Some(300));

        tracker.open(7, 1, Some(200), at(300));
        assert_eq!(
            tracker.close(7, 1, at(900)),
            Some(600),
            "the second channel is timed from the move"
        );
    }

    #[test]
    fn opening_in_a_new_channel_without_closing_replaces_the_session() {
        // The fallback path: it loses the first leg rather than paying it twice.
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        tracker.open(7, 1, Some(200), at(300));

        assert_eq!(tracker.close(7, 1, at(900)), Some(600));
    }

    #[test]
    fn closing_an_untracked_member_pays_nothing() {
        // A member who was already in voice when the bot booted has no session to
        // close, and inventing one would credit them time they may not have spent.
        let tracker = VoiceTracker::new();

        assert_eq!(tracker.close(7, 1, at(3_600)), None);
    }

    #[test]
    fn a_second_close_pays_nothing() {
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        assert_eq!(tracker.close(7, 1, at(60)), Some(60));
        assert_eq!(tracker.close(7, 1, at(120)), None, "already paid");
    }

    #[test]
    fn a_backwards_clock_yields_zero_not_a_negative() {
        // An NTP step or a host resume must not subtract XP from anyone.
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(3_600));
        assert_eq!(tracker.close(7, 1, at(0)), Some(0));
    }

    #[test]
    fn sessions_are_scoped_to_their_guild() {
        // The same user id in two guilds is two different members' time.
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        tracker.open(8, 1, Some(200), at(0));

        assert_eq!(tracker.len(), 2);
        assert_eq!(tracker.close(7, 1, at(600)), Some(600));
        assert_eq!(tracker.close(8, 1, at(1_200)), Some(1_200));
    }

    #[test]
    fn opening_without_a_channel_adds_nothing() {
        let tracker = VoiceTracker::new();
        tracker.open(7, 1, None, at(0));
        assert!(tracker.is_empty());
    }

    #[test]
    fn forgetting_drops_the_session_without_paying_it() {
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        tracker.forget(7, 1);

        assert!(tracker.is_empty());
        assert_eq!(tracker.close(7, 1, at(3_600)), None);
    }

    #[test]
    fn a_zero_length_session_is_distinguishable_from_no_session() {
        // The handler needs both: `Some(0)` is a member who joined and left in the
        // same second (earns nothing), `None` is a member it never saw (also earns
        // nothing, but for a different reason and worth a different log line).
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(5));
        assert_eq!(tracker.close(7, 1, at(5)), Some(0));
        assert_eq!(tracker.close(7, 2, at(5)), None);
    }

    #[test]
    fn a_duration_just_under_a_second_truncates_to_zero() {
        let tracker = VoiceTracker::new();

        tracker.open(7, 1, Some(100), at(0));
        assert_eq!(
            tracker.close(7, 1, at(0) + Duration::milliseconds(900)),
            Some(0)
        );
    }
}
