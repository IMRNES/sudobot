use std::future::Future;
use uuid::Uuid;

use super::entity::{Account, NewTransaction, ShopItem, Transaction};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

pub trait EconomyRepository: Send + Sync {
    fn find_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<Option<Account>, RepositoryError>> + Send;

    /// Returns the account, creating it with the starting balance when absent.
    fn ensure_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> impl Future<Output = Result<Account, RepositoryError>> + Send;

    /// Applies a ledger entry and updates the cached balance atomically.
    ///
    /// Both writes happen in one transaction, and the balance is re-read inside
    /// it: computing the new balance from a value read outside the transaction is
    /// how currency gets duplicated under concurrent purchases.
    fn apply_transaction(
        &self,
        transaction: NewTransaction,
    ) -> impl Future<Output = Result<(Account, Transaction), RepositoryError>> + Send;

    fn list_transactions(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<Transaction>, RepositoryError>> + Send;

    /// Top balances in a guild, highest first.
    fn leaderboard(
        &self,
        guild_id: u64,
        limit: u32,
    ) -> impl Future<Output = Result<Vec<Account>, RepositoryError>> + Send;

    fn find_item_by_slug(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> impl Future<Output = Result<Option<ShopItem>, RepositoryError>> + Send;

    fn find_item_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<ShopItem>, RepositoryError>> + Send;

    fn list_items(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<ShopItem>, RepositoryError>> + Send;

    fn create_item(
        &self,
        item: ShopItem,
    ) -> impl Future<Output = Result<ShopItem, RepositoryError>> + Send;

    fn update_item(
        &self,
        id: Uuid,
        name: Option<String>,
        description: Option<String>,
        price: Option<i64>,
        is_active: Option<bool>,
        stock: Option<Option<i32>>,
    ) -> impl Future<Output = Result<ShopItem, RepositoryError>> + Send;

    /// Decrements stock, guarded so a concurrent buyer cannot take it below zero.
    ///
    /// Returns the number of rows changed: zero means someone else bought the last
    /// one, which the use case reports as a lost race rather than a sale.
    fn decrement_stock_if_available(
        &self,
        item_id: Uuid,
    ) -> impl Future<Output = Result<u64, RepositoryError>> + Send;
}
