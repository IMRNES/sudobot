//! The role/permission matrix. Both columns form the primary key.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "guild_role_permissions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_role_id: Uuid,
    #[sea_orm(primary_key, auto_increment = false)]
    pub permission_id: Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::guild_roles::Entity",
        from = "Column::GuildRoleId",
        to = "super::guild_roles::Column::Id",
        on_delete = "Cascade"
    )]
    GuildRole,
    #[sea_orm(
        belongs_to = "super::permissions::Entity",
        from = "Column::PermissionId",
        to = "super::permissions::Column::Id",
        on_delete = "Cascade"
    )]
    Permission,
}

impl Related<super::guild_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildRole.def()
    }
}

impl Related<super::permissions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Permission.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
