//! What was bought, for how much, against which ledger row.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "economy_purchases")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub guild_id: i64,
    pub discord_user_id: i64,
    pub item_id: Uuid,
    pub price_paid: i64,
    pub transaction_id: Option<Uuid>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::economy_shop_items::Entity",
        from = "Column::ItemId",
        to = "super::economy_shop_items::Column::Id",
        on_delete = "Cascade"
    )]
    ShopItem,
    #[sea_orm(
        belongs_to = "super::economy_transactions::Entity",
        from = "Column::TransactionId",
        to = "super::economy_transactions::Column::Id",
        on_delete = "SetNull"
    )]
    Transaction,
}

impl Related<super::economy_shop_items::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::ShopItem.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
