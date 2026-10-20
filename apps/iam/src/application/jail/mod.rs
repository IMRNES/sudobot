//! Putting a member in jail, and the ways back out.
//!
//! Every release route is versioned here rather than in the bot's command body, so
//! the dashboard and the bot cannot disagree about what "released" means or which
//! paths exist. The routes are:
//!
//! - **time** — the sentence runs out; a sweeper releases them.
//! - **a moderator** — `/jail release`, the dashboard, or removing the role by hand
//!   (which the sweeper reconciles rather than leaving a row claiming otherwise).
//! - **bail** — the prisoner buys their way out, if the guild priced it.
//!
//! What is deliberately *not* a route: leaving and rejoining. A jail that could be
//! escaped that way is not a jail, and Discord hands the bot the rejoin event, so the
//! sweeper re-applies the role to anyone who left a live sentence behind.

use tracing::{debug, info};

use crate::application::ports::Clock;
use crate::domain::errors::RepositoryError;
use crate::domain::jail::{Imprisonment, JailConfig, JailError, JailRepository};

/// How a sentence was ended. Returned so the caller can say *why* somebody is out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReleaseCause {
    /// The sentence ran its course.
    Served,
    /// A moderator let them out.
    Pardoned,
    /// The prisoner paid.
    Bail,
    /// The member holds no jail role any more, so the row is stale.
    Reconciled,
}

impl ReleaseCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Served => "sentence served",
            Self::Pardoned => "pardoned by a moderator",
            Self::Bail => "released on bail",
            Self::Reconciled => "already free",
        }
    }
}

/* -------------------------------------------------------------------------- */
/* Config                                                                      */
/* -------------------------------------------------------------------------- */

pub struct UpdateJailConfigCommand {
    pub guild_id: u64,
    /// `Some(Some(id))` sets, `Some(None)` clears, `None` leaves alone.
    pub jail_role_id: Option<Option<u64>>,
    pub jail_channel_id: Option<Option<u64>>,
    pub default_sentence_seconds: Option<Option<i64>>,
    pub bail_amount: Option<Option<i64>>,
}

pub struct UpdateJailConfigUseCase<R> {
    jail: R,
}

impl<R: JailRepository> UpdateJailConfigUseCase<R> {
    pub fn new(jail: R) -> Self {
        Self { jail }
    }

    /// Applies a partial update, refusing a configuration that would lock members
    /// out of the cell they are supposed to sit in.
    pub async fn execute(&self, command: UpdateJailConfigCommand) -> Result<JailConfig, JailError> {
        let mut config = self.jail.find_config(command.guild_id).await?;

        if let Some(role) = command.jail_role_id {
            config.jail_role_id = role;
        }
        if let Some(channel) = command.jail_channel_id {
            config.jail_channel_id = channel;
        }
        if let Some(default) = command.default_sentence_seconds {
            if let Some(seconds) = default
                && seconds <= 0
            {
                return Err(JailError::InvalidDuration);
            }
            config.default_sentence_seconds = default;
        }
        if let Some(bail) = command.bail_amount {
            if let Some(amount) = bail
                && amount <= 0
            {
                return Err(JailError::InvalidBailAmount);
            }
            config.bail_amount = bail;
        }

        // A role and a channel are different kinds of snowflake, so treating a
        // shared value as a mistake catches the copy-paste that would otherwise
        // produce a jail whose cell is the cell's own role.
        if config.jail_role_id.is_some() && config.jail_role_id == config.jail_channel_id {
            return Err(JailError::SameRoleAndChannel);
        }

        Ok(self.jail.upsert_config(config).await?)
    }
}

/* -------------------------------------------------------------------------- */
/* Jailing                                                                     */
/* -------------------------------------------------------------------------- */

/// Reads the guild's jail configuration.
pub struct GetJailConfigUseCase<R> {
    jail: R,
}

impl<R: JailRepository> GetJailConfigUseCase<R> {
    pub fn new(jail: R) -> Self {
        Self { jail }
    }

    /// The stored config, or the default for a guild that has never configured one.
    ///
    /// Returns a default rather than `NotFound`: "no jail" is a valid state that the
    /// dashboard renders as an empty form, and a 404 would make a fresh guild look
    /// broken.
    pub async fn execute(&self, guild_id: u64) -> Result<JailConfig, JailError> {
        Ok(self.jail.find_config(guild_id).await?)
    }
}

/// Lists the sentences a guild is currently serving.
pub struct ListSentencesUseCase<R> {
    jail: R,
}

impl<R: JailRepository> ListSentencesUseCase<R> {
    pub fn new(jail: R) -> Self {
        Self { jail }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<Vec<Imprisonment>, JailError> {
        Ok(self.jail.list_for_guild(guild_id).await?)
    }
}

pub struct JailMemberCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub jailed_by: u64,
    pub reason: String,
    /// `None` falls back to the guild's default, and then to indefinite.
    pub duration_seconds: Option<i64>,
}

/// The recorded sentence, plus the config the bot needs to apply it.
#[derive(Debug, Clone)]
pub struct JailOutcome {
    pub imprisonment: Imprisonment,
    pub config: JailConfig,
}

pub struct JailMemberUseCase<R, C> {
    jail: R,
    clock: C,
}

impl<R: JailRepository, C: Clock> JailMemberUseCase<R, C> {
    pub fn new(jail: R, clock: C) -> Self {
        Self { jail, clock }
    }

    pub async fn execute(&self, command: JailMemberCommand) -> Result<JailOutcome, JailError> {
        let config = self.jail.find_config(command.guild_id).await?;
        if !config.is_configured() {
            return Err(JailError::NotConfigured);
        }

        let duration = command.duration_seconds.or(config.default_sentence_seconds);

        if let Some(seconds) = duration
            && seconds <= 0
        {
            return Err(JailError::InvalidDuration);
        }

        let now = self.clock.now();
        let imprisonment = Imprisonment {
            guild_id: command.guild_id,
            discord_user_id: command.discord_user_id,
            jailed_by: command.jailed_by,
            // A blank reason is allowed — a moderator typing nothing is not an
            // error — but an empty string stored forever reads as data loss later.
            reason: if command.reason.trim().is_empty() {
                "no reason given".to_string()
            } else {
                command.reason.trim().to_string()
            },
            expires_at: duration.map(|seconds| now + chrono::Duration::seconds(seconds)),
            created_at: now,
        };

        let stored = self.jail.upsert(imprisonment).await?;

        info!(
            guild_id = command.guild_id,
            user_id = command.discord_user_id,
            expires_at = ?stored.expires_at,
            "member jailed"
        );

        Ok(JailOutcome {
            imprisonment: stored,
            config,
        })
    }
}

/* -------------------------------------------------------------------------- */
/* Release                                                                     */
/* -------------------------------------------------------------------------- */

pub struct ReleaseMemberCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub cause: ReleaseCause,
    /// WHO asked, when a person did. Recorded on the log line so "who let them out"
    /// is answerable; `None` for the sweeper.
    pub released_by: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ReleaseOutcome {
    /// The sentence that was ended. `None` when the member had none — already free,
    /// which is an outcome, not an error, but inventing an empty sentence to say so
    /// would put a fabricated row into the caller's hands.
    pub imprisonment: Option<Imprisonment>,
    pub config: JailConfig,
    pub cause: ReleaseCause,
}

pub struct ReleaseMemberUseCase<R> {
    jail: R,
}

impl<R: JailRepository> ReleaseMemberUseCase<R> {
    pub fn new(jail: R) -> Self {
        Self { jail }
    }

    /// Ends a sentence. The row is deleted *before* the bot removes the role, by the
    /// caller's ordering: if the role removal then fails, the sweeper sees the row
    /// is gone and does not re-jail them, which fails in the direction of freedom
    /// rather than of a member stuck with a role nobody is tracking.
    ///
    /// A member with no live sentence is **not** an error. Releasing somebody who is
    /// already free is the ordinary outcome of two moderators reacting to the same
    /// incident, and of a moderator releasing a member the sweeper freed a moment
    /// earlier; both should report "done", not a 404. `was_imprisoned` is what lets
    /// the caller tell the two apart.
    pub async fn execute(
        &self,
        command: ReleaseMemberCommand,
    ) -> Result<ReleaseOutcome, JailError> {
        let config = self.jail.find_config(command.guild_id).await?;

        let Some(existing) = self
            .jail
            .find(command.guild_id, command.discord_user_id)
            .await?
        else {
            debug!(
                guild_id = command.guild_id,
                user_id = command.discord_user_id,
                cause = command.cause.as_str(),
                "release requested for a member with no live sentence"
            );

            return Ok(ReleaseOutcome {
                imprisonment: None,
                config,
                cause: command.cause,
            });
        };

        self.jail
            .delete(command.guild_id, command.discord_user_id)
            .await?;

        info!(
            guild_id = command.guild_id,
            user_id = command.discord_user_id,
            cause = command.cause.as_str(),
            released_by = command.released_by,
            "member released"
        );

        Ok(ReleaseOutcome {
            imprisonment: Some(existing),
            config,
            cause: command.cause,
        })
    }
}

/* -------------------------------------------------------------------------- */
/* Expiry sweep                                                                */
/* -------------------------------------------------------------------------- */

/// One sentence whose time is up, with the role the bot must strip.
pub struct ExpiredSentence {
    pub imprisonment: Imprisonment,
    pub jail_role_id: Option<u64>,
}

pub struct SweepExpiredSentencesUseCase<R, C> {
    jail: R,
    clock: C,
}

impl<R: JailRepository, C: Clock> SweepExpiredSentencesUseCase<R, C> {
    pub fn new(jail: R, clock: C) -> Self {
        Self { jail, clock }
    }

    /// Sentences due for release, oldest deadline first.
    ///
    /// Returns rather than releases: the row is deleted only after the bot confirms
    /// the role came off, so a Discord outage does not silently free nobody while
    /// the table says they are out.
    pub async fn due(&self, limit: u32) -> Result<Vec<ExpiredSentence>, RepositoryError> {
        let now = self.clock.now();
        let expired = self.jail.expired(now, limit).await?;

        let mut due = Vec::with_capacity(expired.len());
        for imprisonment in expired {
            let config = self.jail.find_config(imprisonment.guild_id).await?;
            due.push(ExpiredSentence {
                imprisonment,
                jail_role_id: config.jail_role_id,
            });
        }

        debug!(count = due.len(), "sentences are due for release");
        Ok(due)
    }

    /// Drops the row for a sentence the bot has already undone.
    ///
    /// Idempotent: a moderator releasing the member while the sweep is in flight is
    /// the ordinary race, and arriving second is success, not a failure.
    pub async fn complete(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<(), RepositoryError> {
        self.jail.delete(guild_id, discord_user_id).await
    }
}

/* -------------------------------------------------------------------------- */
/* Bail                                                                        */
/* -------------------------------------------------------------------------- */

pub struct BailQuote {
    pub amount: i64,
    /// `None` for an indefinite sentence, which still has a price.
    pub remaining_seconds: Option<i64>,
}

pub struct QuoteBailUseCase<R, C> {
    jail: R,
    clock: C,
}

impl<R: JailRepository, C: Clock> QuoteBailUseCase<R, C> {
    pub fn new(jail: R, clock: C) -> Self {
        Self { jail, clock }
    }

    /// What this member's release costs, or `None` when the guild has not set bail.
    pub async fn execute(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Option<BailQuote>, JailError> {
        let config = self.jail.find_config(guild_id).await?;
        let Some(amount) = config.bail_amount else {
            return Ok(None);
        };

        let imprisonment = self
            .jail
            .find(guild_id, discord_user_id)
            .await?
            .ok_or(JailError::NotImprisoned(discord_user_id))?;

        let now = self.clock.now();
        if imprisonment.is_expired_at(now) {
            // Already free by time: charging for a sentence that has run out is the
            // one thing bail must never do.
            return Ok(None);
        }

        Ok(Some(BailQuote {
            amount,
            remaining_seconds: imprisonment.remaining_seconds(now),
        }))
    }
}

/// Whether a player can afford a price. Kept pure and here so the economy module
/// and the jail agree on the rule, which is simply "you have at least this much".
pub fn can_afford(balance: i64, amount: i64) -> bool {
    balance >= amount
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::domain::jail::JailConfig;
    use chrono::{DateTime, Duration, TimeZone};

    /* ---------------------------------------------------------------- fakes */

    #[derive(Clone, Default)]
    struct FakeJail {
        configs: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u64, JailConfig>>>,
        sentences:
            std::sync::Arc<std::sync::Mutex<std::collections::HashMap<(u64, u64), Imprisonment>>>,
    }

    impl FakeJail {
        fn with_config(config: JailConfig) -> Self {
            let fake = Self::default();
            fake.configs.lock().unwrap().insert(config.guild_id, config);
            fake
        }
    }

    impl JailRepository for FakeJail {
        async fn find_config(&self, guild_id: u64) -> Result<JailConfig, RepositoryError> {
            Ok(self
                .configs
                .lock()
                .unwrap()
                .get(&guild_id)
                .cloned()
                .unwrap_or_else(|| JailConfig::default_for(guild_id)))
        }

        async fn upsert_config(&self, config: JailConfig) -> Result<JailConfig, RepositoryError> {
            self.configs
                .lock()
                .unwrap()
                .insert(config.guild_id, config.clone());
            Ok(config)
        }

        async fn find(
            &self,
            guild_id: u64,
            discord_user_id: u64,
        ) -> Result<Option<Imprisonment>, RepositoryError> {
            Ok(self
                .sentences
                .lock()
                .unwrap()
                .get(&(guild_id, discord_user_id))
                .cloned())
        }

        async fn upsert(
            &self,
            imprisonment: Imprisonment,
        ) -> Result<Imprisonment, RepositoryError> {
            self.sentences.lock().unwrap().insert(
                (imprisonment.guild_id, imprisonment.discord_user_id),
                imprisonment.clone(),
            );
            Ok(imprisonment)
        }

        async fn delete(&self, guild_id: u64, discord_user_id: u64) -> Result<(), RepositoryError> {
            self.sentences
                .lock()
                .unwrap()
                .remove(&(guild_id, discord_user_id));
            Ok(())
        }

        async fn expired(
            &self,
            now: DateTime<chrono::Utc>,
            limit: u32,
        ) -> Result<Vec<Imprisonment>, RepositoryError> {
            let mut due: Vec<Imprisonment> = self
                .sentences
                .lock()
                .unwrap()
                .values()
                .filter(|sentence| sentence.is_expired_at(now))
                .cloned()
                .collect();
            due.sort_by_key(|sentence| sentence.expires_at);
            due.truncate(limit as usize);
            Ok(due)
        }

        async fn list_for_guild(
            &self,
            guild_id: u64,
        ) -> Result<Vec<Imprisonment>, RepositoryError> {
            Ok(self
                .sentences
                .lock()
                .unwrap()
                .values()
                .filter(|sentence| sentence.guild_id == guild_id)
                .cloned()
                .collect())
        }
    }

    #[derive(Clone, Copy)]
    struct FixedClock(DateTime<chrono::Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<chrono::Utc> {
            self.0
        }

        /// Not used by any jail path — nothing here waits on a timer, because the
        /// sweep asks "what has expired" rather than sleeping until a deadline.
        async fn sleep_until(&self, _deadline: DateTime<chrono::Utc>) {}
    }

    fn now() -> DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 10, 20, 12, 0, 0)
            .unwrap()
    }

    fn configured(guild_id: u64) -> JailConfig {
        JailConfig {
            guild_id,
            jail_role_id: Some(111),
            jail_channel_id: Some(222),
            default_sentence_seconds: None,
            bail_amount: None,
            updated_at: now(),
        }
    }

    /* ---------------------------------------------------------------- tests */

    #[tokio::test]
    async fn jailing_refuses_a_guild_with_no_jail_configured() {
        let jail = FakeJail::default();
        let use_case = JailMemberUseCase::new(jail, FixedClock(now()));

        let error = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(60),
            })
            .await
            .expect_err("an unconfigured jail must refuse");

        assert!(matches!(error, JailError::NotConfigured));
    }

    #[tokio::test]
    async fn a_duration_becomes_a_deadline_from_the_clock() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = JailMemberUseCase::new(jail.clone(), FixedClock(now()));

        let outcome = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(600),
            })
            .await
            .expect("jailing");

        assert_eq!(
            outcome.imprisonment.expires_at,
            Some(now() + Duration::seconds(600)),
            "the deadline must come from the clock, not from wall time"
        );
        assert!(!outcome.imprisonment.is_expired_at(now()));
    }

    #[tokio::test]
    async fn no_duration_falls_back_to_the_default_then_to_indefinite() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = JailMemberUseCase::new(jail.clone(), FixedClock(now()));

        let outcome = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: None,
            })
            .await
            .expect("jailing");
        assert_eq!(
            outcome.imprisonment.expires_at, None,
            "no default means indefinite"
        );

        let mut with_default = configured(7);
        with_default.default_sentence_seconds = Some(3600);
        let jail = FakeJail::with_config(with_default);
        let use_case = JailMemberUseCase::new(jail, FixedClock(now()));

        let outcome = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: None,
            })
            .await
            .expect("jailing");
        assert_eq!(
            outcome.imprisonment.expires_at,
            Some(now() + Duration::seconds(3600))
        );
    }

    #[tokio::test]
    async fn a_zero_duration_is_refused() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = JailMemberUseCase::new(jail, FixedClock(now()));

        let error = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(0),
            })
            .await
            .expect_err("a zero-second sentence is a release, not a jail");

        assert!(matches!(error, JailError::InvalidDuration));
    }

    #[tokio::test]
    async fn re_jailing_replaces_the_sentence_instead_of_failing() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = JailMemberUseCase::new(jail.clone(), FixedClock(now()));

        use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(60),
            })
            .await
            .expect("first sentence");

        let outcome = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 2,
                reason: "again".into(),
                duration_seconds: Some(600),
            })
            .await
            .expect("extending a sentence must not collide on the primary key");

        assert_eq!(outcome.imprisonment.reason, "again");
        assert_eq!(outcome.imprisonment.jailed_by, 2);
    }

    #[tokio::test]
    async fn a_blank_reason_is_recorded_as_words_not_an_empty_string() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = JailMemberUseCase::new(jail, FixedClock(now()));

        let outcome = use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "   ".into(),
                duration_seconds: Some(60),
            })
            .await
            .expect("jailing");

        assert_eq!(outcome.imprisonment.reason, "no reason given");
    }

    #[tokio::test]
    async fn releasing_somebody_who_is_not_jailed_is_a_success_that_says_so() {
        // Two moderators releasing the same member, or one releasing somebody the
        // sweeper freed a second earlier, are ordinary — not a 404. The outcome has
        // to distinguish "freed them" from "they were already free" without turning
        // the second into an error.
        let jail = FakeJail::with_config(configured(7));
        let use_case = ReleaseMemberUseCase::new(jail);

        let outcome = use_case
            .execute(ReleaseMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                cause: ReleaseCause::Pardoned,
                released_by: Some(99),
            })
            .await
            .expect("already-free is an outcome, not a failure");

        assert!(outcome.imprisonment.is_none());
        assert_eq!(outcome.cause, ReleaseCause::Pardoned);
    }

    #[tokio::test]
    async fn the_sweep_finds_served_sentences_and_never_indefinite_ones() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = JailMemberUseCase::new(jail.clone(), FixedClock(now()));

        // One sentence that will have run out, one indefinite.
        use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 1,
                jailed_by: 9,
                reason: "short".into(),
                duration_seconds: Some(60),
            })
            .await
            .expect("short sentence");
        use_case
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 2,
                jailed_by: 9,
                reason: "forever".into(),
                duration_seconds: None,
            })
            .await
            .expect("indefinite sentence");

        let later = FixedClock(now() + Duration::seconds(120));
        let sweep = SweepExpiredSentencesUseCase::new(jail, later);
        let due = sweep.due(50).await.expect("sweeping");

        assert_eq!(due.len(), 1, "only the finite sentence has run out");
        assert_eq!(due[0].imprisonment.discord_user_id, 1);
        assert_eq!(due[0].jail_role_id, Some(111));
    }

    #[tokio::test]
    async fn completing_a_sweep_tolerates_the_row_already_being_gone() {
        // A moderator releasing the member mid-sweep is the ordinary race.
        let jail = FakeJail::with_config(configured(7));
        let sweep = SweepExpiredSentencesUseCase::new(jail, FixedClock(now()));

        sweep
            .complete(7, 5)
            .await
            .expect("a missing row is not an error");
    }

    #[tokio::test]
    async fn bail_is_unavailable_when_the_guild_never_priced_it() {
        let jail = FakeJail::with_config(configured(7));
        let quote = QuoteBailUseCase::new(jail, FixedClock(now()))
            .execute(7, 5)
            .await
            .expect("quoting");
        assert!(quote.is_none(), "no price means no bail, not free bail");
    }

    #[tokio::test]
    async fn bail_is_never_charged_once_the_sentence_has_run_out() {
        let mut config = configured(7);
        config.bail_amount = Some(500);
        let jail = FakeJail::with_config(config);

        // Serve a 60-second sentence, then ask two minutes later.
        JailMemberUseCase::new(jail.clone(), FixedClock(now()))
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(60),
            })
            .await
            .expect("jailing");

        let quote = QuoteBailUseCase::new(jail, FixedClock(now() + Duration::seconds(120)))
            .execute(7, 5)
            .await
            .expect("quoting");

        assert!(
            quote.is_none(),
            "charging for a sentence that already ran out is the one thing bail must not do"
        );
    }

    #[tokio::test]
    async fn bail_reports_the_remaining_time_for_a_live_sentence() {
        let mut config = configured(7);
        config.bail_amount = Some(500);
        let jail = FakeJail::with_config(config);

        JailMemberUseCase::new(jail.clone(), FixedClock(now()))
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(600),
            })
            .await
            .expect("jailing");

        let quote = QuoteBailUseCase::new(jail, FixedClock(now() + Duration::seconds(100)))
            .execute(7, 5)
            .await
            .expect("quoting")
            .expect("a priced live sentence has a quote");

        assert_eq!(quote.amount, 500);
        assert_eq!(quote.remaining_seconds, Some(500));
    }

    #[tokio::test]
    async fn the_config_refuses_a_role_and_channel_that_are_the_same_id() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = UpdateJailConfigUseCase::new(jail);

        let error = use_case
            .execute(UpdateJailConfigCommand {
                guild_id: 7,
                jail_role_id: Some(Some(999)),
                jail_channel_id: Some(Some(999)),
                default_sentence_seconds: None,
                bail_amount: None,
            })
            .await
            .expect_err("a role id is not a channel id");

        assert!(matches!(error, JailError::SameRoleAndChannel));
    }

    #[tokio::test]
    async fn clearing_the_channel_leaves_the_role_alone() {
        let jail = FakeJail::with_config(configured(7));
        let use_case = UpdateJailConfigUseCase::new(jail);

        let config = use_case
            .execute(UpdateJailConfigCommand {
                guild_id: 7,
                jail_role_id: None,
                jail_channel_id: Some(None),
                default_sentence_seconds: None,
                bail_amount: None,
            })
            .await
            .expect("updating");

        assert_eq!(
            config.jail_role_id,
            Some(111),
            "an absent field is not a clear"
        );
        assert_eq!(config.jail_channel_id, None);
    }

    #[test]
    fn affordability_is_inclusive() {
        assert!(can_afford(500, 500));
        assert!(!can_afford(499, 500));
    }

    #[test]
    fn an_indefinite_sentence_never_expires_and_has_no_remaining_time() {
        let sentence = Imprisonment {
            guild_id: 7,
            discord_user_id: 5,
            jailed_by: 1,
            reason: "forever".into(),
            expires_at: None,
            created_at: now(),
        };

        assert!(!sentence.is_expired_at(now() + Duration::days(3650)));
        assert_eq!(sentence.remaining_seconds(now()), None);
    }

    #[tokio::test]
    async fn the_sweep_reports_the_role_so_the_caller_can_strip_it() {
        // The sweep must never release somebody without telling the bot which role
        // to take off — otherwise the row disappears and the member keeps the role.
        let jail = FakeJail::with_config(configured(7));
        JailMemberUseCase::new(jail.clone(), FixedClock(now()))
            .execute(JailMemberCommand {
                guild_id: 7,
                discord_user_id: 5,
                jailed_by: 1,
                reason: "spam".into(),
                duration_seconds: Some(30),
            })
            .await
            .expect("jailing");

        let sweep =
            SweepExpiredSentencesUseCase::new(jail, FixedClock(now() + Duration::seconds(60)));
        let due = sweep.due(10).await.expect("sweeping");

        assert_eq!(due.len(), 1);
        assert_eq!(
            due[0].jail_role_id,
            Some(111),
            "a release with no role id would silently leave the member locked up"
        );
    }
}
