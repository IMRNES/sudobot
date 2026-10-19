use tracing::{error, info, warn};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::economy::{
    Account, EconomyError, EconomyRepository, NewTransaction, TransactionReason,
};

/// What one daily claim is worth.
///
/// A constant rather than a roll: a wound-up random reward makes the ledger
/// impossible to audit ("why did this member get 340?"), and the daily is the one
/// income every member shares, so it is also the one players compare.
pub const DAILY_REWARD: i64 = 100;

/// How long a member waits between claims.
///
/// Passed to `Account::can_claim_daily` rather than compared here: the cooldown
/// is a rule about the account, so it stays testable next to the entity.
pub const DAILY_COOLDOWN_HOURS: i64 = 24;

/// A claim, as a slash command or a button press delivers it.
#[derive(Debug, Clone)]
pub struct ClaimDailyCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// A retried interaction must not pay twice; the repository rejects the
    /// second insert on this key.
    pub idempotency_key: Option<String>,
}

/// The balance the claim left behind, so the reply can name it without re-reading.
#[derive(Debug, Clone)]
pub struct ClaimDailyOutcome {
    pub account: Account,
    /// What this claim added; always [`DAILY_REWARD`].
    pub credited: i64,
}

pub struct ClaimDailyUseCase<E, C> {
    economy: E,
    clock: C,
}

impl<E: EconomyRepository, C: Clock> ClaimDailyUseCase<E, C> {
    pub fn new(economy: E, clock: C) -> Self {
        Self { economy, clock }
    }

    pub async fn execute(
        &self,
        command: ClaimDailyCommand,
    ) -> Result<ClaimDailyOutcome, EconomyError> {
        // The clock is read once, so the cooldown check and the entry the
        // repository writes cannot disagree about "now".
        let now = self.clock.now();

        // 1. The account, created on the first claim. `ensure_account` is the only
        //    path that creates a wallet, so a member who never claimed has none.
        let account = self
            .economy
            .ensure_account(command.guild_id, command.discord_user_id)
            .await?;

        // 2. The cooldown. Waiting is the normal case, not a fault: a player
        //    pressing the button again is exactly what a 24-hour reward invites.
        if !account.can_claim_daily(now, DAILY_COOLDOWN_HOURS) {
            warn!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                remaining_secs = account
                    .daily_remaining(now, DAILY_COOLDOWN_HOURS)
                    .map(|remaining| remaining.num_seconds()),
                "daily reward is not ready yet"
            );
            return Err(EconomyError::DailyNotReady);
        }

        // 3. One ledger entry, through the only write path there is.
        //
        //    Note for the repository: `last_daily_at` can only advance here if
        //    `apply_transaction` stamps it for `TransactionReason::Daily` — no
        //    other method carries the claim time, and without the stamp the
        //    cooldown in step 2 never starts.
        let (account, _entry) = self
            .economy
            .apply_transaction(NewTransaction {
                id: Uuid::new_v4(),
                guild_id: command.guild_id,
                discord_user_id: command.discord_user_id,
                amount: DAILY_REWARD,
                reason: TransactionReason::Daily,
                actor_discord_id: None,
                // A daily claim needs no explanation: the category is the whole
                // story.
                note: None,
                idempotency_key: command.idempotency_key,
            })
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    error = %error,
                    "the ledger write for a daily claim failed"
                );
                EconomyError::from(error)
            })?;

        info!(
            guild_id = command.guild_id,
            discord_user_id = command.discord_user_id,
            credited = DAILY_REWARD,
            balance = account.balance,
            "daily reward claimed"
        );

        Ok(ClaimDailyOutcome {
            account,
            credited: DAILY_REWARD,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use chrono::{Duration, Utc};

    use super::*;
    use crate::application::economy::test_support::{FakeClock, FakeEconomyRepository, epoch};

    fn use_case(
        economy: FakeEconomyRepository,
        clock: FakeClock,
    ) -> ClaimDailyUseCase<FakeEconomyRepository, FakeClock> {
        ClaimDailyUseCase::new(economy, clock)
    }

    fn command() -> ClaimDailyCommand {
        ClaimDailyCommand {
            guild_id: 7,
            discord_user_id: 42,
            idempotency_key: Some("interaction-1".to_string()),
        }
    }

    #[tokio::test]
    async fn first_claim_credits_the_fixed_reward() {
        let economy = FakeEconomyRepository::new();
        let outcome = use_case(economy.clone(), FakeClock::at(epoch()))
            .execute(command())
            .await
            .expect("the first claim is always allowed");

        assert_eq!(outcome.credited, DAILY_REWARD);
        assert_eq!(outcome.account.balance, DAILY_REWARD);
        assert_eq!(outcome.account.last_daily_at, Some(epoch()));

        let ledger = economy.transactions();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].amount, DAILY_REWARD);
        assert_eq!(ledger[0].reason, TransactionReason::Daily.as_str());
    }

    #[tokio::test]
    async fn a_claim_one_second_short_of_the_cooldown_is_refused() {
        let economy = FakeEconomyRepository::new();
        let now = epoch();
        economy.seed_account(
            7,
            42,
            0,
            Some(now - Duration::hours(24) + Duration::seconds(1)),
        );

        let error = use_case(economy.clone(), FakeClock::at(now))
            .execute(command())
            .await
            .expect_err("23h59m59s is not a day");

        assert!(matches!(error, EconomyError::DailyNotReady));
        assert!(economy.transactions().is_empty(), "nothing may be paid out");
    }

    #[tokio::test]
    async fn the_cooldown_boundary_is_exactly_twenty_four_hours() {
        let economy = FakeEconomyRepository::new();
        let now = epoch();
        economy.seed_account(7, 42, 0, Some(now - Duration::hours(24)));

        let outcome = use_case(economy.clone(), FakeClock::at(now))
            .execute(command())
            .await
            .expect("24h to the second is ready");

        assert_eq!(outcome.credited, DAILY_REWARD);
        assert_eq!(economy.transactions().len(), 1);
    }

    #[tokio::test]
    async fn claiming_twice_in_a_row_is_refused_the_second_time() {
        let economy = FakeEconomyRepository::new();
        let clock = FakeClock::at(epoch());

        use_case(economy.clone(), clock)
            .execute(command())
            .await
            .expect("first claim");

        let error = use_case(economy.clone(), clock)
            .execute(command())
            .await
            .expect_err("the second claim must wait");

        assert!(matches!(error, EconomyError::DailyNotReady));
        assert_eq!(economy.transactions().len(), 1);
        assert_eq!(economy.balance(7, 42), DAILY_REWARD);
    }

    #[tokio::test]
    async fn a_claim_after_a_full_day_is_paid_again() {
        let economy = FakeEconomyRepository::new();
        let start = epoch();

        use_case(economy.clone(), FakeClock::at(start))
            .execute(command())
            .await
            .expect("first claim");

        // A second day's claim is a different interaction and so a different key;
        // reusing one would be a replay, which the ledger is supposed to refuse.
        let mut tomorrow = command();
        tomorrow.idempotency_key = Some("interaction-2".to_string());

        let outcome = use_case(
            economy.clone(),
            FakeClock::at(start + Duration::hours(DAILY_COOLDOWN_HOURS)),
        )
        .execute(tomorrow)
        .await
        .expect("a day later the reward is ready again");

        assert_eq!(outcome.account.balance, DAILY_REWARD * 2);
        assert_eq!(economy.transactions().len(), 2);
    }

    #[tokio::test]
    async fn a_replayed_interaction_key_pays_once() {
        let economy = FakeEconomyRepository::new();
        let mut replay = command();
        replay.idempotency_key = Some("interaction-9".to_string());
        let start = epoch();

        use_case(economy.clone(), FakeClock::at(start))
            .execute(replay.clone())
            .await
            .expect("first claim");

        // The same key arriving again is what a Discord retry looks like; the
        // ledger's unique index is what stops it, so the fake reports the
        // conflict the real repository would.
        let error = use_case(economy.clone(), FakeClock::at(start + Duration::days(2)))
            .execute(replay)
            .await
            .expect_err("a replayed key cannot pay a second time");

        assert!(matches!(error, EconomyError::Repository(_)));
        assert_eq!(economy.transactions().len(), 1);
    }

    #[tokio::test]
    async fn the_clock_is_the_only_source_of_now() {
        // A claim made "yesterday" by the clock is refused even though the test
        // itself runs today: nothing in the use case calls `Utc::now()`.
        let economy = FakeEconomyRepository::new();
        let yesterday = Utc::now() - Duration::days(1);
        economy.seed_account(7, 42, 0, Some(yesterday));

        let error = use_case(economy.clone(), FakeClock::at(yesterday))
            .execute(command())
            .await
            .expect_err("no time has passed on the fake clock");

        assert!(matches!(error, EconomyError::DailyNotReady));
    }
}
