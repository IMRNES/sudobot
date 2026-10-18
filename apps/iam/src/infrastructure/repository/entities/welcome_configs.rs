//! Welcome and goodbye configuration. The guild id is the primary key.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "welcome_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: i64,
    pub welcome_enabled: bool,
    pub welcome_channel_id: Option<i64>,
    pub welcome_message: Option<String>,
    pub welcome_embed_color: Option<i32>,
    pub welcome_dm_enabled: bool,
    pub goodbye_enabled: bool,
    pub goodbye_channel_id: Option<i64>,
    pub goodbye_message: Option<String>,
    pub goodbye_embed_color: Option<i32>,
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
}

impl Related<super::guilds::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Guild.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
