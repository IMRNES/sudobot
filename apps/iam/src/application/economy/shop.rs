use tracing::{error, info, warn};
use uuid::Uuid;

use crate::application::ports::Clock;
use crate::domain::economy::{
    Account, EconomyError, EconomyRepository, NewTransaction, ShopItem, TransactionReason,
    ensure_affordable,
};
use crate::domain::errors::{PaginationParams, RepositoryError};

/// How many shop items one listing request reads.
///
/// The endpoint returns a list rather than a page, so this is a ceiling rather
/// than a page size. A guild that somehow exceeds it gets a truncated list and a
/// warning, never a silently short shop.
const SHOP_LISTING_LIMIT: u32 = 500;

/// A shop row as the API renders it.
///
/// A view rather than the entity because the dashboard has no business knowing
/// about `created_at`, and because the bot and the HTTP layer can then share one
/// shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopItemView {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: i64,
    pub role_id: Option<u64>,
    /// `None` means unlimited.
    pub stock: Option<i32>,
    pub is_active: bool,
}

impl From<ShopItem> for ShopItemView {
    fn from(item: ShopItem) -> Self {
        Self {
            id: item.id,
            slug: item.slug,
            name: item.name,
            description: item.description,
            price: item.price,
            role_id: item.role_id,
            stock: item.stock,
            is_active: item.is_active,
        }
    }
}

/// One row of the balance leaderboard.
///
/// The rank is positional: the repository returns the top balances in order, so
/// the position *is* the rank and no window function is needed to find it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaderboardEntry {
    pub discord_user_id: u64,
    pub balance: i64,
}

/// Browsing the shop and the rich list.
pub struct ShopListingUseCase<E> {
    economy: E,
}

impl<E: EconomyRepository> ShopListingUseCase<E> {
    pub fn new(economy: E) -> Self {
        Self { economy }
    }

    /// Every item in the guild, retired ones included.
    ///
    /// Inactive rows are part of the answer on purpose: this is the management
    /// view, and an admin cannot switch an item back on if it is filtered out of
    /// the only list that shows it.
    pub async fn execute(&self, guild_id: u64) -> Result<Vec<ShopItemView>, EconomyError> {
        let params = PaginationParams {
            page: 1,
            per_page: SHOP_LISTING_LIMIT,
        };

        let page = self.economy.list_items(guild_id, &params).await?;

        if page.total > page.data.len() as u64 {
            warn!(
                guild_id,
                returned = page.data.len(),
                total = page.total,
                "the shop listing is larger than one page; the caller is showing a partial shop"
            );
        }

        Ok(page.data.into_iter().map(ShopItemView::from).collect())
    }

    /// The top `limit` balances in the guild, highest first.
    ///
    /// The unity of this view with the store listing is not incidental: both are
    /// "read the guild's economy for the dashboard" and both take the repository
    /// as given.
    pub async fn leaderboard(
        &self,
        guild_id: u64,
        limit: u32,
    ) -> Result<Vec<LeaderboardEntry>, EconomyError> {
        let accounts: Vec<Account> = self.economy.leaderboard(guild_id, limit).await?;

        Ok(accounts
            .into_iter()
            .map(|account| LeaderboardEntry {
                discord_user_id: account.discord_user_id,
                balance: account.balance,
            })
            .collect())
    }
}

/// A new shop item, as the dashboard form delivers it.
#[derive(Debug, Clone)]
pub struct CreateShopItemCommand {
    pub guild_id: u64,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: i64,
    pub role_id: Option<u64>,
    /// `None` means unlimited.
    pub stock: Option<i32>,
}

pub struct CreateShopItemUseCase<E> {
    economy: E,
    /// See [`CreateShopItemUseCase::new`] for why this is not a type parameter.
    clock: crate::application::ports::SystemClock,
}

impl<E: EconomyRepository> CreateShopItemUseCase<E> {
    /// The contract for this command takes no clock, and the entity it builds
    /// needs two timestamps. Rather than reach for `Utc::now()` inside a use case,
    /// the port's system implementation is used: the time still comes from
    /// `Clock`, it is simply not injectable on this path. A fake clock here would
    /// be nicer to test with, and would change an API the dashboard already calls.
    pub fn new(economy: E) -> Self {
        Self {
            economy,
            clock: crate::application::ports::SystemClock,
        }
    }

    /// Adds an item to the guild's shop.
    ///
    /// A duplicate slug is refused by the unique index rather than checked for
    /// first: two admins saving the same slug at the same moment would both pass a
    /// "does it exist" read, and the loser would silently overwrite the winner.
    pub async fn execute(&self, command: CreateShopItemCommand) -> Result<ShopItem, EconomyError> {
        let CreateShopItemCommand {
            guild_id,
            slug,
            name,
            description,
            price,
            role_id,
            stock,
        } = command;

        let price = validate_price(price)?;
        let now = self.clock.now();

        let item = ShopItem {
            id: Uuid::new_v4(),
            guild_id,
            slug: slug.clone(),
            name,
            description,
            price,
            role_id,
            stock,
            // New items are on sale: an admin who wants it hidden creates it and
            // switches it off, which is one more click than being confused by a
            // shop row nobody can see.
            is_active: true,
            created_at: now,
            updated_at: now,
        };

        match self.economy.create_item(item).await {
            Ok(created) => {
                info!(
                    guild_id = created.guild_id,
                    item_id = %created.id,
                    slug = %created.slug,
                    price = created.price,
                    "shop item created"
                );
                Ok(created)
            }
            Err(RepositoryError::Conflict(message)) => {
                warn!(
                    guild_id,
                    slug = %slug,
                    "a shop item with this slug already exists"
                );
                Err(EconomyError::Repository(RepositoryError::Conflict(message)))
            }
            Err(error) => {
                error!(guild_id, error = %error, "creating a shop item failed");
                Err(EconomyError::from(error))
            }
        }
    }
}

/// A partial update to a shop item.
///
/// Note the two-level option on `stock`: absent means "leave it", `null` means
/// "make it unlimited".
#[derive(Debug, Clone)]
pub struct UpdateShopItemCommand {
    /// The guild in the route. Carried so an item belonging to another guild
    /// cannot be edited by quoting its id — the same hole the delete path closed.
    pub guild_id: u64,
    pub id: Uuid,
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<i64>,
    pub is_active: Option<bool>,
    pub stock: Option<Option<i32>>,
}

pub struct UpdateShopItemUseCase<E> {
    economy: E,
}

impl<E: EconomyRepository> UpdateShopItemUseCase<E> {
    pub fn new(economy: E) -> Self {
        Self { economy }
    }

    pub async fn execute(&self, command: UpdateShopItemCommand) -> Result<ShopItem, EconomyError> {
        // Validated before the write, so a bad price cannot be stored even
        // momentarily: `-price` is what a purchase debits, so a negative price
        // would pay the buyer instead of charging them.
        let price = command.price.map(validate_price).transpose()?;

        // Scoped read first: an id alone is not a tenant boundary.
        let existing = self
            .economy
            .find_item_by_id(command.id)
            .await
            .map_err(|error| map_item_error(command.id, error))?
            .filter(|item| item.guild_id == command.guild_id)
            .ok_or(EconomyError::ItemNotFound(command.id))?;

        let item = self
            .economy
            .update_item(
                existing.id,
                command.name,
                command.description,
                price,
                command.is_active,
                command.stock,
            )
            .await
            .map_err(|error| map_item_error(command.id, error))?;

        info!(
            item_id = %item.id,
            guild_id = item.guild_id,
            is_active = item.is_active,
            price = item.price,
            "shop item updated"
        );

        Ok(item)
    }
}

/// Retires a shop item.
pub struct DeleteShopItemUseCase<E> {
    economy: E,
}

impl<E: EconomyRepository> DeleteShopItemUseCase<E> {
    pub fn new(economy: E) -> Self {
        Self { economy }
    }

    /// Takes the item off sale.
    ///
    /// `EconomyRepository` declares no hard delete, and this is the right shape
    /// for an economy anyway: purchases reference the item, so removing the row
    /// would either dangle or cascade the history away. Retiring it keeps the
    /// ledger's explanation of where the currency went.
    ///
    /// `guild_id` scopes the operation. The handler authorizes against the guild in
    /// the path, so without this check a caller holding `economy:write` in guild A
    /// could retire an item belonging to guild B by naming B's id under A's path.
    pub async fn execute(&self, guild_id: u64, id: Uuid) -> Result<(), EconomyError> {
        // Read first so "there is no such item" is told apart from "the update
        // matched nothing", which the update alone cannot distinguish.
        let existing = self
            .economy
            .find_item_by_id(id)
            .await
            .map_err(|error| map_item_error(id, error))?
            .ok_or(EconomyError::ItemNotFound(id))?;

        if existing.guild_id != guild_id {
            return Err(EconomyError::ItemNotFound(id));
        }

        self.economy
            .update_item(id, None, None, None, Some(false), None)
            .await
            .map_err(|error| map_item_error(id, error))?;

        info!(
            item_id = %id,
            guild_id = existing.guild_id,
            slug = %existing.slug,
            "shop item retired"
        );

        Ok(())
    }
}

/// A purchase, as a slash command delivers it.
#[derive(Debug, Clone)]
pub struct PurchaseItemCommand {
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// What the buyer typed, and what the shop is keyed by.
    pub slug: String,
    /// A retried interaction must not pay twice; the repository rejects the
    /// second insert on this key.
    pub idempotency_key: Option<String>,
}

/// What the buyer is left with, so the reply can name the balance and the role
/// the caller should now hand over.
#[derive(Debug, Clone)]
pub struct PurchaseOutcome {
    pub account: Account,
    pub item: ShopItemView,
}

pub struct PurchaseItemUseCase<E, C> {
    economy: E,
    clock: C,
}

impl<E: EconomyRepository, C: Clock> PurchaseItemUseCase<E, C> {
    pub fn new(economy: E, clock: C) -> Self {
        Self { economy, clock }
    }

    /// Buys one item, in the only order that cannot take money for nothing.
    pub async fn execute(
        &self,
        command: PurchaseItemCommand,
    ) -> Result<PurchaseOutcome, EconomyError> {
        let now = self.clock.now();

        // 1. The item, by the slug the buyer typed.
        let item = match self
            .economy
            .find_item_by_slug(command.guild_id, &command.slug)
            .await?
        {
            Some(item) if item.is_available() => item,
            Some(item) => {
                // Sold out and switched off are different answers: one is worth
                // coming back for, the other never will be.
                let error = if item.is_sold_out() {
                    EconomyError::SoldOut
                } else {
                    EconomyError::NotForSale
                };
                warn!(
                    guild_id = command.guild_id,
                    slug = %command.slug,
                    stock = ?item.stock,
                    is_active = item.is_active,
                    "shop item is not purchasable"
                );
                return Err(error);
            }
            // A slug nobody sells and an item that is switched off are the same
            // answer to a buyer. Keeping them the same also keeps unpublished shop
            // rows from being probed for.
            None => {
                warn!(
                    guild_id = command.guild_id,
                    slug = %command.slug,
                    "a purchase was attempted for an unknown slug"
                );
                return Err(EconomyError::NotForSale);
            }
        };

        // 2. The wallet, created on a first purchase, and the affordability check
        //    from the domain. Reading first means a purchase that cannot be paid
        //    for touches nothing at all — no stock, no ledger row.
        let account = self
            .economy
            .ensure_account(command.guild_id, command.discord_user_id)
            .await?;

        if let Err(error) = ensure_affordable(account.balance, item.price) {
            warn!(
                guild_id = command.guild_id,
                discord_user_id = command.discord_user_id,
                slug = %item.slug,
                balance = account.balance,
                price = item.price,
                "purchase refused: insufficient funds"
            );
            return Err(error);
        }

        // 3. Claim the stock *before* taking any money.
        //
        //    The order is the point. `decrement_stock_if_available` reports how
        //    many rows it changed, and zero means another buyer took the last one
        //    between step 1 and here. Charging first would mean refunding — a
        //    second ledger entry to undo a sale that never happened — so the race
        //    is settled while nothing but stock is at stake.
        //
        //    Unlimited items are skipped: there is no count to race over, and
        //    asking for a decrement on a null stock would come back as "lost".
        if item.stock.is_some() {
            let claimed = self
                .economy
                .decrement_stock_if_available(item.id)
                .await
                .map_err(|error| {
                    error!(
                        guild_id = command.guild_id,
                        item_id = %item.id,
                        error = %error,
                        "claiming stock failed"
                    );
                    EconomyError::from(error)
                })?;

            if claimed == 0 {
                warn!(
                    guild_id = command.guild_id,
                    item_id = %item.id,
                    slug = %item.slug,
                    "lost the stock race for the last item"
                );
                return Err(EconomyError::StockRaceLost);
            }
        }

        // 4. Take the money. `-price`, because a purchase is a debit and the
        //    ledger's convention is that negative is out.
        //
        //    The entry carries no item id — `NewTransaction` has no field for one —
        //    so the `economy_purchases` row the schema provides has no writer on
        //    this side of the trait; recording it is the repository's business.
        //
        //    A replayed `idempotency_key` is refused by the ledger here, after the
        //    stock claim. That is the price of settling the race first: the retry
        //    is not charged twice, and the one claimed unit is not returned.
        let (account, _entry) = self
            .economy
            .apply_transaction(NewTransaction {
                id: Uuid::new_v4(),
                guild_id: command.guild_id,
                discord_user_id: command.discord_user_id,
                amount: -item.price,
                reason: TransactionReason::Purchase,
                // The buyer is the account owner; there is no other party.
                actor_discord_id: None,
                note: None,
                idempotency_key: command.idempotency_key.clone(),
            })
            .await
            .map_err(|error| {
                error!(
                    guild_id = command.guild_id,
                    discord_user_id = command.discord_user_id,
                    item_id = %item.id,
                    price = item.price,
                    error = %error,
                    "the ledger write for a purchase failed after the stock claim"
                );
                EconomyError::from(error)
            })?;

        info!(
            guild_id = command.guild_id,
            discord_user_id = command.discord_user_id,
            slug = %item.slug,
            price = item.price,
            balance = account.balance,
            at = %now,
            "item purchased"
        );

        Ok(PurchaseOutcome {
            account,
            item: ShopItemView::from(item),
        })
    }
}

/// A price of zero is allowed — a claimable free item is a legitimate shop row —
/// but a negative one never is.
///
/// A purchase debits `-price`, so a negative price would credit the buyer: the
/// shop would be a money printer with a checkout page.
fn validate_price(price: i64) -> Result<i64, EconomyError> {
    if price < 0 {
        warn!(price, "refused a negative shop price");
        return Err(EconomyError::NonPositiveAmount);
    }
    Ok(price)
}

/// A row that is not there is the item the caller named.
///
/// `find_item_by_id` and `update_item` report a missing row as a repository
/// `NotFound`; the domain's own error is more useful to a client and can only be
/// built here, where the id is still in hand.
fn map_item_error(id: Uuid, error: RepositoryError) -> EconomyError {
    match error {
        RepositoryError::NotFound { .. } => EconomyError::ItemNotFound(id),
        other => EconomyError::Repository(other),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use chrono::Duration;

    use super::*;
    use crate::application::economy::test_support::{
        EconomyCall, FakeClock, FakeEconomyRepository, fixed_now,
    };

    fn purchase_use_case(
        economy: FakeEconomyRepository,
    ) -> PurchaseItemUseCase<FakeEconomyRepository, FakeClock> {
        PurchaseItemUseCase::new(economy, FakeClock::at(fixed_now()))
    }

    fn purchase_command() -> PurchaseItemCommand {
        PurchaseItemCommand {
            guild_id: 7,
            discord_user_id: 42,
            slug: "vip".to_string(),
            idempotency_key: Some("interaction-3".to_string()),
        }
    }

    #[tokio::test]
    async fn a_purchase_claims_stock_then_charges() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 250, Some(3), true);
        economy.seed_account(7, 42, 1000, None);

        let outcome = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect("1000 covers 250");

        assert_eq!(outcome.account.balance, 750);
        assert_eq!(outcome.item.slug, "vip");
        assert_eq!(economy.stock(item_id), Some(2));

        let ledger = economy.transactions();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].amount, -250, "a purchase is negative");
        assert_eq!(ledger[0].reason, TransactionReason::Purchase.as_str());
        assert!(
            economy.used_key("interaction-3"),
            "the caller's key reaches the ledger"
        );

        // The order is the contract, not just the outcome: stock is claimed before
        // the money moves, so a lost race never has to be refunded.
        assert_eq!(
            economy.calls(),
            vec![
                EconomyCall::EnsureAccount {
                    guild_id: 7,
                    discord_user_id: 42
                },
                EconomyCall::DecrementStock { item_id },
                EconomyCall::ApplyTransaction {
                    discord_user_id: 42,
                    amount: -250,
                    reason: "purchase"
                },
            ]
        );
    }

    #[tokio::test]
    async fn an_unaffordable_purchase_never_touches_stock() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 250, Some(3), true);
        economy.seed_account(7, 42, 249, None);

        let error = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect_err("249 does not cover 250");

        assert!(matches!(
            error,
            EconomyError::InsufficientFunds {
                balance: 249,
                price: 250
            }
        ));
        assert_eq!(
            economy.stock(item_id),
            Some(3),
            "the stock claim never happened"
        );
        assert!(economy.transactions().is_empty());
        assert_eq!(
            economy.calls(),
            vec![EconomyCall::EnsureAccount {
                guild_id: 7,
                discord_user_id: 42
            }],
            "nothing past the affordability check ran"
        );
    }

    #[tokio::test]
    async fn a_lost_stock_race_is_refused_before_any_money_moves() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 250, Some(1), true);
        economy.seed_account(7, 42, 1000, None);
        // Another buyer takes the last one between the read and the claim.
        economy.force_stock_race();

        let error = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect_err("the race was lost");

        assert!(matches!(error, EconomyError::StockRaceLost));
        assert!(
            economy.transactions().is_empty(),
            "the loser is not charged for an item they did not get"
        );
        assert_eq!(economy.balance(7, 42), 1000);
        assert_eq!(economy.stock(item_id), Some(1));
    }

    #[tokio::test]
    async fn an_inactive_item_is_not_for_sale() {
        let economy = FakeEconomyRepository::new();
        economy.seed_item("vip", 250, Some(3), false);
        economy.seed_account(7, 42, 1000, None);

        let error = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect_err("a retired item cannot be bought");

        assert!(matches!(error, EconomyError::NotForSale));
        assert!(economy.transactions().is_empty());
    }

    #[tokio::test]
    async fn a_sold_out_item_says_so() {
        let economy = FakeEconomyRepository::new();
        economy.seed_item("vip", 250, Some(0), true);
        economy.seed_account(7, 42, 1000, None);

        let error = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect_err("nothing left in stock");

        assert!(matches!(error, EconomyError::SoldOut));
    }

    #[tokio::test]
    async fn an_unknown_slug_is_not_for_sale() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 42, 1000, None);

        let error = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect_err("nothing is sold under that slug");

        assert!(matches!(error, EconomyError::NotForSale));
        assert!(economy.transactions().is_empty());
    }

    #[tokio::test]
    async fn an_unlimited_item_does_not_race_for_stock() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 100, None, true);
        economy.seed_account(7, 42, 500, None);
        // A decrement on an unlimited item would report zero rows changed; the
        // purchase must not read that as a lost race.
        economy.force_stock_race();

        let outcome = purchase_use_case(economy.clone())
            .execute(purchase_command())
            .await
            .expect("unlimited stock cannot be sold out");

        assert_eq!(outcome.account.balance, 400);
        assert!(
            !economy
                .calls()
                .contains(&EconomyCall::DecrementStock { item_id }),
            "no stock claim is made for an unlimited item"
        );
    }

    #[tokio::test]
    async fn a_free_item_costs_nothing() {
        let economy = FakeEconomyRepository::new();
        economy.seed_item("sticker", 0, Some(1), true);
        economy.seed_account(7, 42, 0, None);

        // The command names the slug the buyer typed, so it has to match the row.
        let freebie = PurchaseItemCommand {
            slug: "sticker".to_string(),
            ..purchase_command()
        };

        let outcome = purchase_use_case(economy.clone())
            .execute(freebie)
            .await
            .expect("a price of zero is affordable at any balance");

        assert_eq!(outcome.account.balance, 0);
        assert_eq!(economy.transactions().len(), 1);
    }

    #[tokio::test]
    async fn the_shop_listing_keeps_retired_items_and_maps_the_view() {
        let economy = FakeEconomyRepository::new();
        economy.seed_item("vip", 5000, None, true);
        economy.seed_item("old", 100, Some(2), false);

        let items = ShopListingUseCase::new(economy.clone())
            .execute(7)
            .await
            .expect("listing");

        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .any(|item| item.slug == "old" && !item.is_active)
        );
        let vip = items
            .iter()
            .find(|item| item.slug == "vip")
            .expect("the vip item is listed");
        assert_eq!(vip.price, 5000);
        assert_eq!(vip.stock, None);
        assert_eq!(vip.role_id, None);
    }

    #[tokio::test]
    async fn the_leaderboard_maps_accounts_in_order() {
        let economy = FakeEconomyRepository::new();
        economy.seed_account(7, 1, 100, None);
        economy.seed_account(7, 2, 900, None);
        economy.seed_account(7, 3, 500, None);

        let entries = ShopListingUseCase::new(economy.clone())
            .leaderboard(7, 10)
            .await
            .expect("leaderboard");

        assert_eq!(
            entries,
            vec![
                LeaderboardEntry {
                    discord_user_id: 2,
                    balance: 900
                },
                LeaderboardEntry {
                    discord_user_id: 3,
                    balance: 500
                },
                LeaderboardEntry {
                    discord_user_id: 1,
                    balance: 100
                },
            ],
            "highest first, because the rank is the position"
        );
    }

    #[tokio::test]
    async fn creating_an_item_refuses_a_negative_price() {
        let economy = FakeEconomyRepository::new();
        let use_case = CreateShopItemUseCase::new(economy.clone());

        let error = use_case
            .execute(CreateShopItemCommand {
                guild_id: 7,
                slug: "trap".to_string(),
                name: "Trap".to_string(),
                description: None,
                price: -1,
                role_id: None,
                stock: None,
            })
            .await
            .expect_err("a negative price would pay the buyer");

        assert!(matches!(error, EconomyError::NonPositiveAmount));
        assert!(economy.calls().is_empty(), "nothing was written");
    }

    #[tokio::test]
    async fn creating_an_item_that_already_exists_is_a_conflict() {
        let economy = FakeEconomyRepository::new();
        economy.seed_item("vip", 500, None, true);

        let error = CreateShopItemUseCase::new(economy.clone())
            .execute(CreateShopItemCommand {
                guild_id: 7,
                slug: "vip".to_string(),
                name: "VIP again".to_string(),
                description: None,
                price: 500,
                role_id: None,
                stock: None,
            })
            .await
            .expect_err("the slug is unique per guild");

        assert!(matches!(
            error,
            EconomyError::Repository(RepositoryError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn updating_an_item_can_make_the_stock_unlimited() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 500, Some(4), true);

        let updated = UpdateShopItemUseCase::new(economy.clone())
            .execute(UpdateShopItemCommand {
                guild_id: 7,
                id: item_id,
                name: Some("VIP+".to_string()),
                description: None,
                price: Some(600),
                is_active: None,
                stock: Some(None),
            })
            .await
            .expect("the item exists");

        assert_eq!(updated.name, "VIP+");
        assert_eq!(updated.price, 600);
        assert_eq!(
            updated.stock, None,
            "two-level option: null means unlimited"
        );
        assert!(updated.is_active, "an absent field is left alone");
    }

    #[tokio::test]
    async fn updating_a_missing_item_names_the_item() {
        let economy = FakeEconomyRepository::new();
        let missing = Uuid::new_v4();

        let error = UpdateShopItemUseCase::new(economy.clone())
            .execute(UpdateShopItemCommand {
                guild_id: 7,
                id: missing,
                name: Some("ghost".to_string()),
                description: None,
                price: None,
                is_active: None,
                stock: None,
            })
            .await
            .expect_err("there is no such item");

        assert!(matches!(error, EconomyError::ItemNotFound(id) if id == missing));
    }

    #[tokio::test]
    async fn updating_an_item_refuses_a_negative_price_before_writing() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 500, None, true);

        let error = UpdateShopItemUseCase::new(economy.clone())
            .execute(UpdateShopItemCommand {
                guild_id: 7,
                id: item_id,
                name: None,
                description: None,
                price: Some(-5),
                is_active: None,
                stock: None,
            })
            .await
            .expect_err("a negative price would pay the buyer");

        assert!(matches!(error, EconomyError::NonPositiveAmount));
        assert_eq!(economy.item(item_id).map(|item| item.price), Some(500));
    }

    #[tokio::test]
    async fn deleting_an_item_retires_it_rather_than_erasing_the_history() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 500, Some(2), true);

        DeleteShopItemUseCase::new(economy.clone())
            .execute(7, item_id)
            .await
            .expect("the item exists");

        let retired = economy.item(item_id).expect("the row is still there");
        assert!(!retired.is_active);
    }

    #[tokio::test]
    async fn deleting_a_missing_item_names_the_item() {
        let economy = FakeEconomyRepository::new();
        let missing = Uuid::new_v4();

        let error = DeleteShopItemUseCase::new(economy.clone())
            .execute(7, missing)
            .await
            .expect_err("there is no such item");

        assert!(matches!(error, EconomyError::ItemNotFound(id) if id == missing));
    }

    /// An item is only reachable through the guild that owns it: the handler
    /// authorizes against the guild in the path, so ignoring the guild here would
    /// let `economy:write` in one server retire another server's listing.
    #[tokio::test]
    async fn an_item_in_another_guild_cannot_be_retired() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 500, None, true);

        let error = DeleteShopItemUseCase::new(economy.clone())
            .execute(8, item_id)
            .await
            .expect_err("guild 8 does not own this item");

        assert!(matches!(error, EconomyError::ItemNotFound(id) if id == item_id));
        assert!(
            economy
                .item(item_id)
                .expect("the row is still there")
                .is_active,
            "the foreign item must stay on sale"
        );
    }

    /// The same tenant boundary applies to an edit: `UpdateShopItemUseCase`
    /// authorizes against the guild in the path, so ignoring the guild would let
    /// `economy:write` in one server reprice another server's listing.
    #[tokio::test]
    async fn an_item_in_another_guild_cannot_be_edited() {
        let economy = FakeEconomyRepository::new();
        let item_id = economy.seed_item("vip", 500, None, true);

        let error = UpdateShopItemUseCase::new(economy.clone())
            .execute(UpdateShopItemCommand {
                guild_id: 8,
                id: item_id,
                name: None,
                description: None,
                price: Some(1),
                is_active: None,
                stock: None,
            })
            .await
            .expect_err("guild 8 does not own this item");

        assert!(matches!(error, EconomyError::ItemNotFound(id) if id == item_id));
        assert_eq!(
            economy.item(item_id).map(|item| item.price),
            Some(500),
            "the foreign item must keep its price"
        );
    }

    #[tokio::test]
    async fn the_clock_decides_nothing_here_but_is_still_the_source_of_time() {
        // A guard against reaching for `Utc::now()` when the entity needs a stamp:
        // the surrounding tests would still pass with a wall clock, so the point is
        // only that the use case stays free of one.
        let economy = FakeEconomyRepository::new();
        let earlier = fixed_now() - Duration::days(3);

        let item = CreateShopItemUseCase::new(economy.clone())
            .execute(CreateShopItemCommand {
                guild_id: 7,
                slug: "vip".to_string(),
                name: "VIP".to_string(),
                description: None,
                price: 10,
                role_id: None,
                stock: None,
            })
            .await
            .expect("creating an item");

        assert!(item.created_at > earlier);
        assert_eq!(item.created_at, item.updated_at);
    }
}
