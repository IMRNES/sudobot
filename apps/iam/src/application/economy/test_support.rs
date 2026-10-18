// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! A hand-written [`EconomyRepository`] for the use-case tests.
//!
//! Small enough to read in one sitting and with no database behind it, which is
//! the point: the rules under test are the cooldown boundary and the order of the
//! purchase steps, and a fake can record the order. [`FakeEconomyRepository::calls`]
//! returns the repository methods that ran, so a test can assert not only the
//! outcome but which side effects did and did not happen.

use std::sync::{Arc, Mutex, MutexGuard};

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::economy::{
    Account, EconomyRepository, NewTransaction, ShopItem, Transaction, TransactionReason,
};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

/// A fixed instant the fakes build their timestamps from, so nothing in a test
/// depends on when it runs.
pub fn fixed_now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_780_000_000, 0).unwrap_or_else(Utc::now)
}

/// The start of a clock-driven test.
pub fn epoch() -> DateTime<Utc> {
    fixed_now()
}

/// A [`Clock`] that answers whatever the test decided "now" is.
#[derive(Debug, Clone, Copy)]
pub struct FakeClock {
    now: DateTime<Utc>,
}

impl FakeClock {
    pub fn at(now: DateTime<Utc>) -> Self {
        Self { now }
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.now
    }

    async fn sleep_until(&self, _deadline: DateTime<Utc>) {
        // Nothing in the economy use cases waits; a test that needed a wait would
        // be testing something else.
    }
}

/// A repository call, for asserting order and for proving an absence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EconomyCall {
    FindAccount {
        guild_id: u64,
        discord_user_id: u64,
    },
    EnsureAccount {
        guild_id: u64,
        discord_user_id: u64,
    },
    ApplyTransaction {
        discord_user_id: u64,
        amount: i64,
        reason: &'static str,
    },
    DecrementStock {
        item_id: Uuid,
    },
}

/// An in-memory economy: wallets, the ledger and a shop.
#[derive(Debug, Clone, Default)]
pub struct FakeEconomyRepository {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    accounts: Vec<Account>,
    ledger: Vec<Transaction>,
    items: Vec<ShopItem>,
    calls: Vec<EconomyCall>,
    /// Idempotency keys already spent. Kept beside the ledger rather than on the
    /// entry because `Transaction` has no field for one.
    used_keys: Vec<String>,
    /// Failure injection for writes: `None` for a healthy repository, `Some(n)`
    /// lets `n` writes through and fails every one after them. `Some(0)` fails
    /// immediately — what a test of a two-part operation needs, since the first
    /// leg has to stand for the second one's failure to mean anything.
    writes_until_failure: Option<usize>,
    /// When set, the next stock decrement reports zero rows changed.
    lose_next_race: bool,
}

impl FakeEconomyRepository {
    pub fn new() -> Self {
        Self::default()
    }

    /// A poisoned lock only means another test thread panicked; the state itself
    /// is still readable, and recovering keeps this file free of `unwrap`.
    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Creates a wallet directly, without going through a use case.
    pub fn seed_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        balance: i64,
        last_daily_at: Option<DateTime<Utc>>,
    ) {
        let now = fixed_now();
        self.state().accounts.push(Account {
            id: Uuid::new_v4(),
            guild_id,
            discord_user_id,
            balance,
            total_earned: balance.max(0),
            total_spent: 0,
            last_daily_at,
            created_at: now,
            updated_at: now,
        });
    }

    /// Creates a shop item directly.
    pub fn seed_item(&self, slug: &str, price: i64, stock: Option<i32>, is_active: bool) -> Uuid {
        let now = fixed_now();
        let id = Uuid::new_v4();
        self.state().items.push(ShopItem {
            id,
            guild_id: 7,
            slug: slug.to_string(),
            name: slug.to_uppercase(),
            description: None,
            price,
            role_id: None,
            stock,
            is_active,
            created_at: now,
            updated_at: now,
        });
        id
    }

    /// Lets `writes` writes through, then fails every one after them, so the
    /// second leg of a two-part operation can be made to fail while the first
    /// stands. `fail_after(0)` fails the very next write.
    pub fn fail_after(&self, writes: usize) {
        self.state().writes_until_failure = Some(writes);
    }

    /// Makes the next stock decrement report zero rows changed.
    pub fn force_stock_race(&self) {
        self.state().lose_next_race = true;
    }

    pub fn balance(&self, guild_id: u64, discord_user_id: u64) -> i64 {
        self.state()
            .accounts
            .iter()
            .find(|account| {
                account.guild_id == guild_id && account.discord_user_id == discord_user_id
            })
            .map(|account| account.balance)
            .unwrap_or_default()
    }

    pub fn account_exists(&self, guild_id: u64, discord_user_id: u64) -> bool {
        self.state().accounts.iter().any(|account| {
            account.guild_id == guild_id && account.discord_user_id == discord_user_id
        })
    }

    pub fn transactions(&self) -> Vec<Transaction> {
        self.state().ledger.clone()
    }

    pub fn calls(&self) -> Vec<EconomyCall> {
        self.state().calls.clone()
    }

    pub fn item(&self, id: Uuid) -> Option<ShopItem> {
        self.state()
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
    }

    pub fn stock(&self, id: Uuid) -> Option<i32> {
        self.item(id).and_then(|item| item.stock)
    }

    /// Whether an idempotency key has been spent. `Transaction` has no field for
    /// one, so a test asks the fake rather than the row.
    pub fn used_key(&self, key: &str) -> bool {
        self.state().used_keys.iter().any(|used| used == key)
    }
}

impl State {
    fn account_mut(&mut self, guild_id: u64, discord_user_id: u64) -> Option<&mut Account> {
        self.accounts.iter_mut().find(|account| {
            account.guild_id == guild_id && account.discord_user_id == discord_user_id
        })
    }
}

impl EconomyRepository for FakeEconomyRepository {
    async fn find_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Option<Account>, RepositoryError> {
        let mut state = self.state();
        state.calls.push(EconomyCall::FindAccount {
            guild_id,
            discord_user_id,
        });

        Ok(state
            .accounts
            .iter()
            .find(|account| {
                account.guild_id == guild_id && account.discord_user_id == discord_user_id
            })
            .cloned())
    }

    async fn ensure_account(
        &self,
        guild_id: u64,
        discord_user_id: u64,
    ) -> Result<Account, RepositoryError> {
        let mut state = self.state();
        state.calls.push(EconomyCall::EnsureAccount {
            guild_id,
            discord_user_id,
        });

        if let Some(account) = state.account_mut(guild_id, discord_user_id) {
            return Ok(account.clone());
        }

        let now = fixed_now();
        let account = Account {
            id: Uuid::new_v4(),
            guild_id,
            discord_user_id,
            balance: Account::STARTING_BALANCE,
            total_earned: 0,
            total_spent: 0,
            last_daily_at: None,
            created_at: now,
            updated_at: now,
        };

        state.accounts.push(account.clone());
        Ok(account)
    }

    async fn apply_transaction(
        &self,
        transaction: NewTransaction,
    ) -> Result<(Account, Transaction), RepositoryError> {
        let reason = transaction.reason;
        let mut state = self.state();

        state.calls.push(EconomyCall::ApplyTransaction {
            discord_user_id: transaction.discord_user_id,
            amount: transaction.amount,
            reason: reason.as_str(),
        });

        // The call was recorded above either way, so a test can see the attempt
        // that failed as well as the ones that succeeded.
        match state.writes_until_failure.as_mut() {
            None => {}
            Some(0) => return Err(RepositoryError::database("the fake was told to fail")),
            Some(remaining) => *remaining -= 1,
        }

        // The ledger's idempotency key is unique: this is what stops a replayed
        // interaction from paying twice. The real repository reports a constraint
        // violation as `Conflict`, so the fake does too.
        if let Some(key) = transaction.idempotency_key.as_ref()
            && state.used_keys.iter().any(|used| used == key)
        {
            return Err(RepositoryError::conflict("idempotency key already used"));
        }

        let account = state
            .account_mut(transaction.guild_id, transaction.discord_user_id)
            .ok_or_else(|| RepositoryError::not_found("economy account"))?;

        account.balance += transaction.amount;
        if transaction.amount > 0 {
            account.total_earned += transaction.amount;
        } else {
            account.total_spent += transaction.amount.abs();
        }
        if reason == TransactionReason::Daily {
            account.last_daily_at = Some(fixed_now());
        }

        let entry = Transaction {
            id: transaction.id,
            guild_id: transaction.guild_id,
            discord_user_id: transaction.discord_user_id,
            amount: transaction.amount,
            balance_after: account.balance,
            reason: reason.as_str().to_string(),
            actor_discord_id: transaction.actor_discord_id,
            created_at: fixed_now(),
        };

        let updated = account.clone();
        state.ledger.push(entry.clone());
        if let Some(key) = transaction.idempotency_key {
            state.used_keys.push(key);
        }

        Ok((updated, entry))
    }

    async fn list_transactions(
        &self,
        guild_id: u64,
        discord_user_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<Transaction>, RepositoryError> {
        let matching: Vec<Transaction> = self
            .state()
            .ledger
            .iter()
            .filter(|entry| entry.guild_id == guild_id && entry.discord_user_id == discord_user_id)
            .cloned()
            .collect();

        let total = matching.len() as u64;
        let data = matching
            .into_iter()
            .skip(params.offset() as usize)
            .take(params.per_page as usize)
            .collect();

        Ok(Page::new(data, params, total))
    }

    async fn leaderboard(
        &self,
        guild_id: u64,
        limit: u32,
    ) -> Result<Vec<Account>, RepositoryError> {
        let mut matching: Vec<Account> = self
            .state()
            .accounts
            .iter()
            .filter(|account| account.guild_id == guild_id)
            .cloned()
            .collect();

        // Highest balance first: the caller takes the rank from the position, so
        // the order is the answer.
        matching.sort_by_key(|account| std::cmp::Reverse(account.balance));
        matching.truncate(limit as usize);

        Ok(matching)
    }

    async fn find_item_by_slug(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<Option<ShopItem>, RepositoryError> {
        Ok(self
            .state()
            .items
            .iter()
            .find(|item| item.guild_id == guild_id && item.slug == slug)
            .cloned())
    }

    async fn find_item_by_id(&self, id: Uuid) -> Result<Option<ShopItem>, RepositoryError> {
        Ok(self.item(id))
    }

    async fn list_items(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<ShopItem>, RepositoryError> {
        let matching: Vec<ShopItem> = self
            .state()
            .items
            .iter()
            .filter(|item| item.guild_id == guild_id)
            .cloned()
            .collect();

        let total = matching.len() as u64;
        let data = matching
            .into_iter()
            .skip(params.offset() as usize)
            .take(params.per_page as usize)
            .collect();

        Ok(Page::new(data, params, total))
    }

    async fn create_item(&self, item: ShopItem) -> Result<ShopItem, RepositoryError> {
        let mut state = self.state();

        if state
            .items
            .iter()
            .any(|existing| existing.guild_id == item.guild_id && existing.slug == item.slug)
        {
            return Err(RepositoryError::conflict(
                "a shop item with this slug exists",
            ));
        }

        state.items.push(item.clone());
        Ok(item)
    }

    async fn update_item(
        &self,
        id: Uuid,
        name: Option<String>,
        description: Option<String>,
        price: Option<i64>,
        is_active: Option<bool>,
        stock: Option<Option<i32>>,
    ) -> Result<ShopItem, RepositoryError> {
        let mut state = self.state();

        let item = state
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| RepositoryError::not_found("shop item"))?;

        if let Some(name) = name {
            item.name = name;
        }
        if let Some(description) = description {
            item.description = Some(description);
        }
        if let Some(price) = price {
            item.price = price;
        }
        if let Some(is_active) = is_active {
            item.is_active = is_active;
        }
        // The outer option is "was the field sent", the inner one is its value:
        // `Some(None)` clears the stock to unlimited.
        if let Some(stock) = stock {
            item.stock = stock;
        }
        item.updated_at = fixed_now();

        Ok(item.clone())
    }

    async fn decrement_stock_if_available(&self, item_id: Uuid) -> Result<u64, RepositoryError> {
        let mut state = self.state();
        state.calls.push(EconomyCall::DecrementStock { item_id });

        if state.lose_next_race {
            state.lose_next_race = false;
            return Ok(0);
        }

        let item = state
            .items
            .iter_mut()
            .find(|item| item.id == item_id)
            .ok_or_else(|| RepositoryError::not_found("shop item"))?;

        match item.stock {
            None => Ok(1),
            Some(stock) if stock > 0 => {
                item.stock = Some(stock - 1);
                Ok(1)
            }
            Some(_) => Ok(0),
        }
    }
}
