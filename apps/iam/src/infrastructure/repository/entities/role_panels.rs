//! Button-role panels: the message members self-assign roles from.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "role_panels")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub guild_id: i64,
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    pub channel_id: Option<i64>,
    pub message_id: Option<i64>,
    pub is_published: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::guilds::Entity",
        from = "Column::GuildId",
        to = "super::guilds::Column::Id",
        on_delete = "Cascade"
    )]
    Guild,
    #[sea_orm(has_many = "super::role_panel_options::Entity")]
    RolePanelOption,
}

impl Related<super::guilds::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Guild.def()
    }
}

impl Related<super::role_panel_options::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RolePanelOption.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
