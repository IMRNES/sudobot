use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum EconomyError {
    #[error("account not found")]
    AccountNotFound,

    #[error("item not found")]
    ItemNotFound(Uuid),

    #[error("insufficient funds: {balance} available, {price} required")]
    InsufficientFunds { balance: i64, price: i64 },

    #[error("the daily reward is not ready yet")]
    DailyNotReady,

    #[error("amount must be greater than zero")]
    NonPositiveAmount,

    #[error("you cannot transfer currency to yourself")]
    SelfTransfer,

    #[error("that item is sold out")]
    SoldOut,

    #[error("that item is not for sale")]
    NotForSale,

    #[error("the item's stock changed while you were buying it; try again")]
    StockRaceLost,

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl EconomyError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::Repository(_))
    }
}

/// Rejects a non-positive amount before it reaches the ledger.
///
/// A zero-amount row is noise in the history and a negative one would let a
/// caller express a debit through the credit path.
pub fn validate_amount(amount: i64) -> Result<i64, EconomyError> {
    if amount <= 0 {
        return Err(EconomyError::NonPositiveAmount);
    }
    Ok(amount)
}

/// The overdraft rule, in one place.
///
/// Failures here are expected — players try to buy things they cannot afford —
/// so this logs at `warn`, not `error`.
pub fn ensure_affordable(balance: i64, price: i64) -> Result<(), EconomyError> {
    if balance < price {
        tracing::warn!(balance, price, "insufficient funds");
        return Err(EconomyError::InsufficientFunds { balance, price });
    }
    Ok(())
}

/// Whether an administrative adjustment may take an account below zero.
pub fn ensure_admin_deduction_allowed(balance: i64, amount: i64) -> Result<i64, EconomyError> {
    let deducted = balance.saturating_sub(amount);
    Ok(deducted)
}
