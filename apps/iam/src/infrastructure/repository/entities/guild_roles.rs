//! Per-guild roles: named bundles of permissions.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "guild_roles")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub guild_id: i64,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub position: i32,
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
    #[sea_orm(has_many = "super::guild_role_permissions::Entity")]
    GuildRolePermission,
    #[sea_orm(has_many = "super::guild_members::Entity")]
    GuildMember,
}

impl Related<super::guilds::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Guild.def()
    }
}

impl Related<super::guild_role_permissions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildRolePermission.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
