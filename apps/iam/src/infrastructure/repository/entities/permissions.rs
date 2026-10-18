//! The global permission catalog.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "permissions")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub name: String,
    pub group: String,
    pub description: String,
    pub created_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::guild_role_permissions::Entity")]
    GuildRolePermission,
}

impl Related<super::guild_role_permissions::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildRolePermission.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
