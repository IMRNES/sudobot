use tracing::{error, info, warn};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::economy::{
    Account, EconomyError, EconomyRepository, NewTransaction, TransactionReason, ensure_affordable,
    validate_amount,
};

/// A member-to-member payment, as a slash command delivers it.
#[derive(Debug, Clone)]
pub struct TransferCommand {
    pub guild_id: u64,
    pub from_discord_id: u64,
    pub to_discord_id: u64,
    pub amount: i64,
    /// A retried interaction must not pay twice; the repository rejects the
    /// second insert on this key.
    pub idempotency_key: Option<String>,
}

/// Both sides as they stand after the payment, so the reply can name both
/// balances without re-reading.
#[derive(Debug, Clone)]
pub struct TransferOutcome {
    pub sender: Account,
    pub recipient: Account,
    pub amount: i64,
}

pub struct TransferUseCase<E, C> {
    economy: E,
    clock: C,
}

impl<E: EconomyRepository, C: Clock> TransferUseCase<E, C> {
    pub fn new(economy: E, clock: C) -> Self {
        Self { economy, clock }
    }

    /// Moves currency between two members of one guild.
    ///
    /// The move is written as two ledger entries — the sender's debit and the
    /// recipient's credit — and **cross-account atomicity is the repository's job**.
    /// `apply_transaction` is the only write path this use case has, so it cannot
    /// wrap the pair in a transaction itself; see the note below the two calls.
    pub async fn execute(&self, command: TransferCommand) -> Result<TransferOutcome, EconomyError> {
        // The clock is read once so both entries are stamped from the same
        // instant, and so a test can decide what "now" is.
        let now = self.clock.now();
        let _ = now;

        // 1. The two rules that need no state: a member cannot pay themselves,
        //    and an amount that is not positive is not a payment.
        if command.from_discord_id == command.to_discord_id {
            warn!(
                guild_id = command.guild_id,
                discord_user_id = command.from_discord_id,
                "refused a self-transfer"
            );
            return Err(EconomyError::SelfTransfer);
        }

        let amount = validate_amount(command.amount)?;

        // 2. Both wallets, created on demand: the recipient may never have held
        //    currency before, and refusing the payment on those grounds would be
        //    a confusing way to say "they have no account yet".
        let sender = self
            .economy
            .ensure_account(command.guild_id, command.from_discord_id)
            .await?;

        // The overdraft rule lives in the domain and is *not* consulted for the
        // recipient's credit: only an admin deduction may go below zero.
        if let Err(error) = ensure_affordable(sender.balance, amount) {
            warn!(
                guild_id = command.guild_id,
                from = command.from_discord_id,
                to = command.to_discord_id,
                amount,
                balance = sender.balance,
                "transfer refused: the sender cannot cover it"
            );
            return Err(error);
        }

        let _recipient = self
            .economy
            .ensure_account(command.guild_id, command.to_discord_id)
            .await?;

        // 3. The sender's debit. This is the first leg, and the one the client's
        //    idempotency key is spent on: a replayed interaction collides with the
        //    ledger's unique index here and never reaches the credit below.
        //
        //    Atomicity note for the repository: the two `apply_transaction` calls
        //    in this method are one business operation, but the trait takes a
        //    single entry per call, so nothing in this signature can join them.
        //    `apply_transaction` must therefore treat a debit and its matching
        //    credit as one unit — matching them on the idempotency key, whose
        //    credit half is `<key>:credit` — or a crash between the calls leaves
        //    the sender's money in no one's account. That guarantee cannot be
        //    written on this side of the trait, and it is the one thing about
        //    transfers a use case cannot verify.
        let (sender, _debit) = self
            .economy
            .apply_transaction(NewTransaction {
                id: Uuid::new_v4(),
                guild_id: command.guild_id,
                discord_user_id: command.from_discord_id,
                amount: -amount,
                reason: TransactionReason::Transfer,
                // The sender moved their own money, so there is no separate actor.
                actor_discord_id: None,
                idempotency_key: command.idempotency_key.clone(),
            })
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    from = command.from_discord_id,
                    to = command.to_discord_id,
                    amount,
                    error = %error,
                    "the debit leg of a transfer failed"
                );
                EconomyError::from(error)
            })?;

        // 4. The recipient's credit, under a key derived from the same
        //    idempotency key: the ledger's key column is unique, so reusing the
        //    caller's key verbatim would make the two legs collide with each
        //    other on the very first attempt.
        let credit_key = command
            .idempotency_key
            .as_ref()
            .map(|key| format!("{key}:credit"));

        let (recipient, _credit) = self
            .economy
            .apply_transaction(NewTransaction {
                id: Uuid::new_v4(),
                guild_id: command.guild_id,
                discord_user_id: command.to_discord_id,
                amount,
                reason: TransactionReason::Transfer,
                // The counterparty is what makes a received transfer readable in
                // the recipient's history.
                actor_discord_id: Some(command.from_discord_id),
                idempotency_key: credit_key,
            })
            .await
            .map_err(|error| {
                // Reaching here means the debit stands and the credit does not:
                // money has left the sender's account and arrived nowhere. Loud,
                // because it is the one failure this use case cannot undo.
                error!(
                    guild_id = command.guild_id,
                    from = command.from_discord_id,
                    to = command.to_discord_id,
                    amount,
                    error = %error,
                    "the credit leg of a transfer failed after the debit; the repository must roll the debit back"
                );
                EconomyError::from(error)
            })?;

        info!(
            guild_id = command.guild_id,
            from = command.from_discord_id,
            to = command.to_discord_id,
            amount,
            sender_balance = sender.balance,
            recipient_balance = recipient.balance,
            "transfer complete"
        );

        Ok(TransferOutcome {
            sender,
            recipient,
            amount,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::application::economy::test_support::{FakeClock, FakeEconomyRepository, fixed_now};

    fn use_case(
        economy: FakeEconomyRepository,
    ) -> TransferUseCase<FakeEconomyRepository, FakeClock> {
        TransferUseCase::new(economy, FakeClock::at(fixed_now()))
    }

    fn command(amount: i64) -> TransferCommand {
        TransferCommand {
            guild_id: 7,
            from_discord_id: 1,
            to_discord_id: 2,
            amount,
            idempotency_key: Some("interaction-2".to_string()),
        }
    }

    #[tokio::test]
    async fn a_transfer_writes_two_entries_with_opposite_signs() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 500, None);

        let outcome = use_case(economy.clone())
            .execute(command(200))
            .await
            .expect("500 covers 200");

        assert_eq!(outcome.amount, 200);
        assert_eq!(outcome.sender.balance, 300);
        assert_eq!(outcome.recipient.balance, 200);
        assert_eq!(outcome.recipient.discord_user_id, 2);

        let ledger = economy.transactions();
        assert_eq!(ledger.len(), 2, "one debit, one credit, never one net row");
        assert_eq!(ledger[0].amount, -200);
        assert_eq!(ledger[0].discord_user_id, 1);
        assert_eq!(ledger[1].amount, 200);
        assert_eq!(ledger[1].discord_user_id, 2);
        assert_eq!(
            ledger[1].actor_discord_id,
            Some(1),
            "the recipient's history names the payer"
        );

        // Both legs must be distinguishable, because the ledger's idempotency
        // key is unique.
        assert_eq!(ledger[0].reason, TransactionReason::Transfer.as_str());
        assert_ne!(ledger[0].id, ledger[1].id);
    }

    #[tokio::test]
    async fn a_transfer_to_yourself_is_refused() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 500, None);

        let mut self_transfer = command(10);
        self_transfer.to_discord_id = 1;

        let error = use_case(economy.clone())
            .execute(self_transfer)
            .await
            .expect_err("paying yourself is not a payment");

        assert!(matches!(error, EconomyError::SelfTransfer));
        assert!(economy.transactions().is_empty());
        assert_eq!(economy.balance(7, 1), 500);
    }

    #[tokio::test]
    async fn a_non_positive_amount_is_refused() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 500, None);

        for amount in [0, -1, i64::MIN] {
            let error = use_case(economy.clone())
                .execute(command(amount))
                .await
                .expect_err("only a positive amount is a transfer");

            assert!(
                matches!(error, EconomyError::NonPositiveAmount),
                "amount {amount}"
            );
        }

        assert!(economy.transactions().is_empty());
    }

    #[tokio::test]
    async fn an_overdraft_is_refused_before_anything_is_written() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 100, None);

        let error = use_case(economy.clone())
            .execute(command(101))
            .await
            .expect_err("100 does not cover 101");

        assert!(matches!(
            error,
            EconomyError::InsufficientFunds {
                balance: 100,
                price: 101
            }
        ));
        assert!(economy.transactions().is_empty());
        assert_eq!(economy.balance(7, 1), 100);
        assert_eq!(
            economy.balance(7, 2),
            0,
            "the recipient's wallet is created empty, never credited"
        );
    }

    #[tokio::test]
    async fn spending_the_whole_balance_is_allowed() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 100, None);

        let outcome = use_case(economy.clone())
            .execute(command(100))
            .await
            .expect("exactly enough is enough");

        assert_eq!(outcome.sender.balance, 0);
        assert_eq!(outcome.recipient.balance, 100);
    }

    #[tokio::test]
    async fn a_failing_credit_leg_is_surfaced_loudly() {
        // The fake cannot roll the debit back, and neither can this use case: the
        // trait gives it no way to. What is asserted here is that the failure is
        // reported rather than swallowed, so an operator sees the broken pair.
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 500, None);
        economy.fail_after(1);

        let error = use_case(economy.clone())
            .execute(command(200))
            .await
            .expect_err("the credit leg failed");

        assert!(matches!(error, EconomyError::Repository(_)));
        assert_eq!(
            economy.transactions().len(),
            1,
            "the orphaned debit is visible"
        );
    }

    #[tokio::test]
    async fn a_replayed_key_is_rejected_on_the_debit_leg() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 500, None);

        use_case(economy.clone())
            .execute(command(50))
            .await
            .expect("first transfer");

        let error = use_case(economy.clone())
            .execute(command(50))
            .await
            .expect_err("the same key cannot pay twice");

        assert!(matches!(error, EconomyError::Repository(_)));
        assert_eq!(economy.transactions().len(), 2, "one debit, one credit");
        assert_eq!(economy.balance(7, 1), 450);
        assert_eq!(economy.balance(7, 2), 50);
    }
}
