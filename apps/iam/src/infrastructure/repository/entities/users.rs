//! Dashboard accounts.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    #[sea_orm(unique)]
    pub discord_id: i64,
    pub username: String,
    pub global_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email: Option<String>,
    pub is_super_admin: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::refresh_tokens::Entity")]
    RefreshToken,
    #[sea_orm(has_many = "super::guild_members::Entity")]
    GuildMember,
}

impl Related<super::refresh_tokens::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RefreshToken.def()
    }
}

impl Related<super::guild_members::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildMember.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
