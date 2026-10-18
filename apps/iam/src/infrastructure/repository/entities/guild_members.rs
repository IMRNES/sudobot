//! Which dashboard user may administer which guild.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "guild_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: Uuid,
    pub guild_id: i64,
    pub user_id: Uuid,
    pub guild_role_id: Uuid,
    pub discord_permissions: i64,
    pub discord_nick: Option<String>,
    pub synced_at: DateTimeWithTimeZone,
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
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id",
        on_delete = "Cascade"
    )]
    User,
    #[sea_orm(
        belongs_to = "super::guild_roles::Entity",
        from = "Column::GuildRoleId",
        to = "super::guild_roles::Column::Id",
        on_delete = "Restrict"
    )]
    GuildRole,
}

impl Related<super::guilds::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Guild.def()
    }
}

impl Related<super::users::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::User.def()
    }
}

impl Related<super::guild_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildRole.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
