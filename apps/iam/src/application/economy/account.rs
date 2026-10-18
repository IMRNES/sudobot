use crate::domain::economy::{Account, EconomyError, EconomyRepository, Transaction};
use crate::domain::errors::{Page, PaginationParams};

/// Reads one member's wallet.
///
/// A plain read on purpose: the row is created by `ensure_account` at the moment
/// currency first moves, so browsing the dashboard does not litter the table with
/// empty accounts for everyone an admin happens to click on.
pub struct GetAccountUseCase<E> {
    economy: E,
}

impl<E: EconomyRepository> GetAccountUseCase<E> {
    pub fn new(economy: E) -> Self {
        Self { economy }
    }

    pub async fn execute(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Account, EconomyError> {
        self.economy
            .find_account(guild_id, discord_user_id)
            .await?
            .ok_or(EconomyError::AccountNotFound)
    }
}

/// One page of a member's ledger, as the dashboard's history tab shows it.
///
/// Read-only and unguarded by any rule: the ledger is append-only, so there is
/// nothing here to validate beyond forwarding the page window.
pub struct ListTransactionsUseCase<E> {
    economy: E,
}

impl<E: EconomyRepository> ListTransactionsUseCase<E> {
    pub fn new(economy: E) -> Self {
        Self { economy }
    }

    pub async fn execute(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<Transaction>, EconomyError> {
        Ok(self
            .economy
            .list_transactions(guild_id, discord_user_id, params)
            .await?)
    }
}
