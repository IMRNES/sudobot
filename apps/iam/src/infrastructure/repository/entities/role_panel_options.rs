//! One button on a panel.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "role_panel_options")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub panel_id: Uuid,
    pub guild_id: i64,
    #[sea_orm(unique)]
    pub custom_id: String,
    pub label: String,
    pub emoji: Option<String>,
    pub role_id: i64,
    pub style: i16,
    pub position: i32,
    pub behavior: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::role_panels::Entity",
        from = "Column::PanelId",
        to = "super::role_panels::Column::Id",
        on_delete = "Cascade"
    )]
    RolePanel,
}

impl Related<super::role_panels::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RolePanel.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
