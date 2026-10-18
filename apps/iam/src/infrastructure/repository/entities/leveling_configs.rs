//! Per-guild XP configuration. The guild id is the primary key.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "leveling_configs")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: i64,
    pub enabled: bool,
    pub xp_min: i32,
    pub xp_max: i32,
    pub cooldown_secs: i32,
    pub voice_xp_multiplier: f64,
    pub curve: String,
    pub announce_channel_id: Option<i64>,
    pub stack_roles: bool,
    /// `bigint[]`; `None` is SQL NULL, `Some(vec![])` an empty array.
    pub ignored_channel_ids: Option<Vec<i64>>,
    pub ignored_role_ids: Option<Vec<i64>>,
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
