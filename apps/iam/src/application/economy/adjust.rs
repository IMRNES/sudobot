use tracing::{error, info, warn};
use uuid::Uuid;

use crate::domain::economy::{
    Account, EconomyError, EconomyRepository, NewTransaction, TransactionReason,
    ensure_admin_deduction_allowed,
};
use crate::domain::errors::RepositoryError;

/// An administrative adjustment, from the dashboard.
#[derive(Debug, Clone)]
pub struct AdjustBalanceCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// Signed: positive grants, negative deducts.
    pub amount: i64,
    /// The dashboard user who made the change, for the audit line.
    pub actor_user_id: Option<Uuid>,
    /// Why. Required, because an adjustment with no stated reason is an
    /// unexplainable number in somebody's history three months later.
    pub reason: String,
}

pub struct AdjustBalanceUseCase<E> {
    economy: E,
}

impl<E: EconomyRepository> AdjustBalanceUseCase<E> {
    pub fn new(economy: E) -> Self {
        Self { economy }
    }

    /// Grants or deducts currency on behalf of an administrator.
    ///
    /// Unlike a purchase or a transfer, a deduction here is **allowed to drive the
    /// balance below zero**: an admin correcting a duplicated grant has to be able
    /// to take back more than the member still holds, and a refusal would leave the
    /// books wrong. The permission to overdraw is the domain's
    /// (`TransactionReason::AdminDeduct::may_overdraw`), not a decision made here.
    pub async fn execute(&self, command: AdjustBalanceCommand) -> Result<Account, EconomyError> {
        // A zero adjustment is a ledger row that changes nothing, and the reason
        // is what makes the row explicable later.
        if command.amount == 0 {
            warn!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                "refused a zero adjustment"
            );
            return Err(EconomyError::NonPositiveAmount);
        }

        if command.reason.trim().is_empty() {
            warn!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                "refused an adjustment with no reason"
            );
            // The domain has no variant for "missing reason", and `InvalidData` is
            // its designated "this value cannot represent a domain value" error.
            return Err(EconomyError::Repository(RepositoryError::invalid_data(
                "reason",
                "an adjustment must state why",
            )));
        }

        let account = self
            .economy
            .ensure_account(command.guild_id, command.discord_user_id)
            .await?;

        // The direction picks the reason, and the reason decides whether an
        // overdraft is permitted — which is why the ledger never sees a signed
        // "adjustment" it would have to interpret.
        let (reason, projected_balance) = if command.amount > 0 {
            (
                TransactionReason::AdminGrant,
                account.balance + command.amount,
            )
        } else {
            // Delegated to the domain so the overdraft rule has one home. The
            // repository recomputes the balance inside its transaction; this value
            // is here to be logged, not to be written.
            let deducted = ensure_admin_deduction_allowed(account.balance, command.amount.abs())?;
            if deducted < 0 {
                warn!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    balance = account.balance,
                    amount = command.amount,
                    projected = deducted,
                    "an administrative deduction takes this account below zero"
                );
            }
            (TransactionReason::AdminDeduct, deducted)
        };

        let (account, _entry) = self
            .economy
            .apply_transaction(NewTransaction {
                id: Uuid::new_v4(),
                guild_id: command.guild_id,
                discord_user_id: command.discord_user_id,
                amount: command.amount,
                reason,
                // The ledger identifies actors by Discord id and an adjustment is
                // made by a dashboard user, so there is no id to record here: the
                // actor is carried in the log line below instead.
                actor_discord_id: None,
                // An admin adjustment is not a retryable client action, but
                // accepting a key keeps the write path uniform for callers that
                // do have one.
                idempotency_key: None,
            })
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    amount = command.amount,
                    error = %error,
                    "an administrative adjustment failed"
                );
                EconomyError::from(error)
            })?;

        info!(
            guild_id = command.guild_id,
            discord_user_id = command.discord_user_id,
            amount = command.amount,
            reason = %command.reason,
            actor_user_id = ?command.actor_user_id,
            projected_balance,
            balance = account.balance,
            "balance adjusted"
        );

        Ok(account)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::application::economy::test_support::FakeEconomyRepository;

    fn command(amount: i64) -> AdjustBalanceCommand {
        AdjustBalanceCommand {
            guild_id: 7,
            discord_user_id: 42,
            amount,
            actor_user_id: Some(Uuid::new_v4()),
            reason: "event prize".to_string(),
        }
    }

    #[tokio::test]
    async fn a_positive_adjustment_grants_and_is_labelled_admin_grant() {
        let economy = FakeEconomyRepository::new();

        let account = AdjustBalanceUseCase::new(economy.clone())
            .execute(command(500))
            .await
            .expect("a grant needs no funds");

        assert_eq!(account.balance, 500);
        let ledger = economy.transactions();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].amount, 500);
        assert_eq!(ledger[0].reason, TransactionReason::AdminGrant.as_str());
    }

    #[tokio::test]
    async fn a_negative_adjustment_may_drive_the_balance_below_zero() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 42, 100, None);

        let account = AdjustBalanceUseCase::new(economy.clone())
            .execute(command(-250))
            .await
            .expect("an admin deduction may overdraw");

        assert_eq!(account.balance, -150);
        let ledger = economy.transactions();
        assert_eq!(ledger[0].amount, -250);
        // The reason is what licenses the overdraft; `AdminGrant` would not.
        assert_eq!(ledger[0].reason, TransactionReason::AdminDeduct.as_str());
        assert!(TransactionReason::AdminDeduct.may_overdraw());
        assert!(!TransactionReason::AdminGrant.may_overdraw());
    }

    #[tokio::test]
    async fn a_zero_adjustment_is_refused() {
        let economy = FakeEconomyRepository::new();

        let error = AdjustBalanceUseCase::new(economy.clone())
            .execute(command(0))
            .await
            .expect_err("zero changes nothing");

        assert!(matches!(error, EconomyError::NonPositiveAmount));
        assert!(economy.transactions().is_empty());
    }

    #[tokio::test]
    async fn an_adjustment_without_a_reason_is_refused() {
        let economy = FakeEconomyRepository::new();

        let mut blank = command(100);
        blank.reason = "   ".to_string();

        let error = AdjustBalanceUseCase::new(economy.clone())
            .execute(blank)
            .await
            .expect_err("an unexplained adjustment is not auditable");

        assert!(matches!(
            error,
            EconomyError::Repository(RepositoryError::InvalidData { .. })
        ));
        assert!(economy.transactions().is_empty());
        assert!(
            !economy.account_exists(7, 42),
            "the wallet is not created for a request that is refused"
        );
    }

    #[tokio::test]
    async fn repeated_deductions_keep_going_below_zero() {
        let economy = FakeEconomyRepository::new();
        let use_case = AdjustBalanceUseCase::new(economy.clone());

        use_case.execute(command(-50)).await.expect("first");
        let account = use_case.execute(command(-50)).await.expect("second");

        assert_eq!(account.balance, -100);
        assert_eq!(economy.transactions().len(), 2);
    }
}
