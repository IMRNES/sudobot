//! Delayed grants waiting for the bot's sweep.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "auto_role_pending")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub guild_id: i64,
    pub discord_user_id: i64,
    pub auto_role_id: Uuid,
    pub role_id: i64,
    pub due_at: DateTimeWithTimeZone,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::auto_roles::Entity",
        from = "Column::AutoRoleId",
        to = "super::auto_roles::Column::Id",
        on_delete = "Cascade"
    )]
    AutoRole,
}

impl Related<super::auto_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::AutoRole.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
