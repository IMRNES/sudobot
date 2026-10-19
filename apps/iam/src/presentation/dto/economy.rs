use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use super::{optional_snowflake, snowflake};
use crate::application::economy::{LeaderboardEntry, ShopItemView};
use crate::domain::economy::{Account, Transaction};
use crate::presentation::extract::ValidatedRequest;

#[derive(Debug, Serialize)]
pub struct AccountResponse {
    #[serde(with = "snowflake")]
    pub discord_id: u64,
    pub balance: i64,
    pub total_earned: i64,
    pub total_spent: i64,
    pub last_daily_at: Option<DateTime<Utc>>,
}

impl From<Account> for AccountResponse {
    fn from(account: Account) -> Self {
        Self {
            discord_id: account.discord_user_id,
            balance: account.balance,
            total_earned: account.total_earned,
            total_spent: account.total_spent,
            last_daily_at: account.last_daily_at,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id: Uuid,
    /// Signed: positive credits, negative debits.
    pub amount: i64,
    pub balance_after: i64,
    pub reason: String,
    /// The admin's own words, when the category alone does not explain the entry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(with = "optional_snowflake")]
    pub actor_discord_id: Option<u64>,
    pub created_at: DateTime<Utc>,
}

impl From<Transaction> for TransactionResponse {
    fn from(transaction: Transaction) -> Self {
        Self {
            id: transaction.id,
            amount: transaction.amount,
            balance_after: transaction.balance_after,
            reason: transaction.reason,
            note: transaction.note,
            actor_discord_id: transaction.actor_discord_id,
            created_at: transaction.created_at,
        }
    }
}

/// Grants or deducts currency from the dashboard.
///
/// The reason is required and ends up in the ledger: an adjustment without one
/// becomes an unexplainable number in a member's history three months later.
#[derive(Debug, Deserialize, ZodSchema)]
pub struct AdjustBalanceRequest {
    /// Signed. A negative amount deducts and may take the balance below zero.
    pub amount: i64,
    #[zod(min_length(1), max_length(200))]
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardEntryResponse {
    pub rank: u32,
    #[serde(with = "snowflake")]
    pub discord_id: u64,
    pub balance: i64,
}

impl LeaderboardEntryResponse {
    pub fn list(entries: &[LeaderboardEntry]) -> Vec<Self> {
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| Self {
                // Rank comes from position in the ordered result, so the
                // repository never has to compute a window function.
                rank: index as u32 + 1,
                discord_id: entry.discord_user_id,
                balance: entry.balance,
            })
            .collect()
    }
}

#[derive(Debug, Serialize)]
pub struct ShopItemResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub price: i64,
    #[serde(with = "optional_snowflake")]
    pub role_id: Option<u64>,
    pub stock: Option<i32>,
    pub is_active: bool,
}

impl From<ShopItemView> for ShopItemResponse {
    fn from(item: ShopItemView) -> Self {
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

#[derive(Debug, Deserialize, ZodSchema)]
pub struct CreateShopItemRequest {
    #[zod(min_length(1), max_length(48))]
    pub slug: String,
    #[zod(min_length(1), max_length(96))]
    pub name: String,
    #[zod(max_length(1000))]
    pub description: Option<String>,
    pub price: i64,
    /// Discord role id granted on purchase, as a string.
    pub role_id: Option<String>,
    /// `None` means unlimited.
    pub stock: Option<i32>,
}

/// A partial update. Note the two-level option on `stock`: absent means "leave it",
/// `null` means "make it unlimited" — the distinction a shop needs and a plain
/// `Option<i32>` cannot express.
///
/// Hand-validated rather than derived: `zod-rs`'s derive unwraps one `Option` and
/// then asks `Option::schema()` for the inner type, which does not exist, so a
/// `Option<Option<T>>` field cannot be derived at all. The field-level checks below
/// are the same ones `#[zod(...)]` would have produced.
#[derive(Debug, Deserialize)]
pub struct UpdateShopItemRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub price: Option<i64>,
    pub is_active: Option<bool>,
    #[serde(default, deserialize_with = "double_option")]
    pub stock: Option<Option<i32>>,
}

/// Distinguishes "key absent" from "key present and null".
fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl ValidatedRequest for UpdateShopItemRequest {
    fn validate(value: &serde_json::Value) -> Result<(), Vec<(String, String)>> {
        let mut issues = Vec::new();

        check_optional_string_len(value, "name", 1, 96, &mut issues);
        check_optional_string_len(value, "description", 0, 1000, &mut issues);

        if let Some(price) = value.get("price").and_then(serde_json::Value::as_i64)
            && price < 0
        {
            issues.push(("price".into(), "must not be negative".into()));
        }

        if let Some(stock) = value.get("stock").and_then(serde_json::Value::as_i64)
            && stock < 0
        {
            issues.push(("stock".into(), "must not be negative".into()));
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

/// Length check that ignores an absent or null field.
fn check_optional_string_len(
    value: &serde_json::Value,
    field: &str,
    min: usize,
    max: usize,
    issues: &mut Vec<(String, String)>,
) {
    let Some(text) = value.get(field).and_then(serde_json::Value::as_str) else {
        return;
    };
    let length = text.chars().count();

    if length < min {
        issues.push((
            field.to_string(),
            format!("must be at least {min} characters"),
        ));
    } else if length > max {
        issues.push((
            field.to_string(),
            format!("must be at most {max} characters"),
        ));
    }
}

crate::validated_request!(AdjustBalanceRequest);

crate::validated_request!(CreateShopItemRequest);
