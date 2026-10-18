//! Discord guilds the bot is installed in.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "guilds")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: i64,
    pub name: String,
    pub icon_url: Option<String>,
    pub owner_discord_id: Option<i64>,
    pub member_count: i32,
    pub bot_present: bool,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::guild_roles::Entity")]
    GuildRole,
    #[sea_orm(has_many = "super::guild_members::Entity")]
    GuildMember,
    #[sea_orm(has_many = "super::guild_member_profiles::Entity")]
    GuildMemberProfile,
    #[sea_orm(has_many = "super::economy_accounts::Entity")]
    EconomyAccount,
    #[sea_orm(has_many = "super::economy_shop_items::Entity")]
    EconomyShopItem,
    #[sea_orm(has_many = "super::leveling_profiles::Entity")]
    LevelingProfile,
    #[sea_orm(has_many = "super::role_panels::Entity")]
    RolePanel,
    #[sea_orm(has_many = "super::auto_roles::Entity")]
    AutoRole,
    #[sea_orm(has_one = "super::leveling_configs::Entity")]
    LevelingConfig,
    #[sea_orm(has_one = "super::welcome_configs::Entity")]
    WelcomeConfig,
}

impl Related<super::guild_roles::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildRole.def()
    }
}

impl Related<super::guild_members::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::GuildMember.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
