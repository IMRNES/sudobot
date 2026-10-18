//! The economy: wallets, a transfer, admin adjustments and a shop.
//!
//! Every movement of currency is one signed row in the ledger, and every mutation
//! here reaches storage through `apply_transaction`. Nothing in this module
//! computes a balance from a value it read earlier — that value is only ever used
//! to *refuse* an operation, and the repository recomputes the real balance inside
//! its transaction. That split is what keeps two members buying the same last item
//! from being paid twice.

pub mod account;
pub mod adjust;
pub mod daily;
pub mod shop;
pub mod transfer;

#[cfg(test)]
pub mod test_support;

pub use account::{GetAccountUseCase, ListTransactionsUseCase};
pub use adjust::{AdjustBalanceCommand, AdjustBalanceUseCase};
pub use daily::{
    ClaimDailyCommand, ClaimDailyOutcome, ClaimDailyUseCase, DAILY_COOLDOWN_HOURS, DAILY_REWARD,
};
pub use shop::{
    CreateShopItemCommand, CreateShopItemUseCase, DeleteShopItemUseCase, LeaderboardEntry,
    PurchaseItemCommand, PurchaseItemUseCase, PurchaseOutcome, ShopItemView, ShopListingUseCase,
    UpdateShopItemCommand, UpdateShopItemUseCase,
};
pub use transfer::{TransferCommand, TransferOutcome, TransferUseCase};
