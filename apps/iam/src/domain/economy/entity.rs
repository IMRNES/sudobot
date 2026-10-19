use chrono::{DateTime, Utc};
use uuid::Uuid;

/// A member's wallet in one guild.
///
/// `balance` is a cached projection of the ledger, kept here so "show my balance"
/// is a single row read. Every change goes through the ledger, so the two can
/// always be reconciled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: Uuid,
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub balance: i64,
    pub total_earned: i64,
    pub total_spent: i64,
    pub last_daily_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Account {
    /// The prompt-managed starting balance.
    pub const STARTING_BALANCE: i64 = 0;

    /// Whether the member has spent little enough to afford `price`.
    pub fn can_afford(&self, price: i64) -> bool {
        price >= 0 && self.balance >= price
    }

    /// Whether `now` is far enough past the last claim.
    ///
    /// Takes `now` as an argument rather than calling `Utc::now()` so the rule is
    /// testable without freezing the clock.
    pub fn can_claim_daily(&self, now: DateTime<Utc>, cooldown_hours: i64) -> bool {
        match self.last_daily_at {
            None => true,
            Some(last) => now.signed_duration_since(last).num_hours() >= cooldown_hours,
        }
    }

    /// Time remaining before the next claim, if any.
    pub fn daily_remaining(
        &self,
        now: DateTime<Utc>,
        cooldown_hours: i64,
    ) -> Option<chrono::Duration> {
        let last = self.last_daily_at?;
        let ready_at = last + chrono::Duration::hours(cooldown_hours);
        let remaining = ready_at.signed_duration_since(now);
        (remaining.num_seconds() > 0).then_some(remaining)
    }
}

/// Why currency moved. Kept as a closed set so the ledger stays queryable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionReason {
    Daily,
    Work,
    Transfer,
    Purchase,
    AdminGrant,
    AdminDeduct,
    LevelUpReward,
    Refund,
}

impl TransactionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Work => "work",
            Self::Transfer => "transfer",
            Self::Purchase => "purchase",
            Self::AdminGrant => "admin_grant",
            Self::AdminDeduct => "admin_deduct",
            Self::LevelUpReward => "level_up_reward",
            Self::Refund => "refund",
        }
    }

    /// Whether this reason can push a balance negative.
    ///
    /// Only an explicit administrative deduction may; everything else is bounced
    /// by the insufficient-funds check first.
    pub fn may_overdraw(self) -> bool {
        matches!(self, Self::AdminDeduct)
    }
}

/// One append-only ledger entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub id: Uuid,
    pub guild_id: u64,
    pub discord_user_id: u64,
    /// Signed: positive credits the account, negative debits it.
    pub amount: i64,
    pub balance_after: i64,
    pub reason: String,
    pub actor_discord_id: Option<u64>,
    /// An admin's own words, when the category alone does not explain the row.
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// A request to move currency, as the use case receives it.
#[derive(Debug, Clone)]
pub struct NewTransaction {
    pub id: Uuid,
    pub guild_id: u64,
    pub discord_user_id: u64,
    pub amount: i64,
    pub reason: TransactionReason,
    pub actor_discord_id: Option<u64>,
    /// An admin's own words, when the category alone does not explain the row.
    ///
    /// Separate from `reason` because `reason` is a closed set the ledger's rules
    /// are keyed on — only `admin_deduct` may overdraw, for one — so free text
    /// cannot live there. This is what makes "why was I given 5000?" answerable.
    pub note: Option<String>,
    /// A retried Discord interaction must not pay twice; the repository rejects
    /// the second insert on this key.
    pub idempotency_key: Option<String>,
}

/// Something the shop sells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShopItem {
    pub id: Uuid,
    pub guild_id: u64,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: i64,
    /// The Discord role handed out on purchase, when the item represents one.
    pub role_id: Option<u64>,
    /// `None` means unlimited.
    pub stock: Option<i32>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ShopItem {
    pub fn is_available(&self) -> bool {
        self.is_active && self.stock.map(|stock| stock > 0).unwrap_or(true)
    }

    pub fn is_sold_out(&self) -> bool {
        matches!(self.stock, Some(0))
    }
}
