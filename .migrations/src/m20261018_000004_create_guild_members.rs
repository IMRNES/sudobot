use sea_orm_migration::prelude::*;

use super::m20261018_000001_create_users::Users;
use super::m20261018_000002_create_guilds::Guilds;
use super::m20261018_000003_create_rbac::GuildRoles;

/// Which user may administer which guild — the multi-tenant boundary.
///
/// Every guild-scoped query filters on `guild_id`, and every write is gated by the
/// role this row points at.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(GuildMembers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GuildMembers::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(GuildMembers::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(GuildMembers::UserId).uuid().not_null())
                    .col(ColumnDef::new(GuildMembers::GuildRoleId).uuid().not_null())
                    // Discord's own permission bits, captured at login, so the
                    // dashboard can explain why a role was granted.
                    .col(
                        ColumnDef::new(GuildMembers::DiscordPermissions)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(GuildMembers::DiscordNick)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(GuildMembers::SyncedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(GuildMembers::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(GuildMembers::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // One row per user per guild: this unique pair is what the
                    // authorization lookup relies on.
                    .index(
                        Index::create()
                            .name("uq_guild_members_guild_user")
                            .col(GuildMembers::GuildId)
                            .col(GuildMembers::UserId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_guild_members_guild")
                            .from(GuildMembers::Table, GuildMembers::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_guild_members_user")
                            .from(GuildMembers::Table, GuildMembers::UserId)
                            .to(Users::Table, Users::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    // Restrict, not cascade: deleting a role that people still
                    // hold should fail loudly rather than silently demote them.
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_guild_members_role")
                            .from(GuildMembers::Table, GuildMembers::GuildRoleId)
                            .to(GuildRoles::Table, GuildRoles::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_guild_members_user")
                    .table(GuildMembers::Table)
                    .col(GuildMembers::UserId)
                    .to_owned(),
            )
            .await?;

        // Bot-side member state: XP totals, wallet balances and welcome flags are
        // all per guild per Discord member, so this is the row the bot reads and
        // writes on every message.
        manager
            .create_table(
                Table::create()
                    .table(GuildMemberProfiles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GuildMemberProfiles::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::DisplayName)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::JoinedAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::LastSeenAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    // Bot users must never earn XP or money.
                    .col(
                        ColumnDef::new(GuildMemberProfiles::IsBot)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::IsBlacklisted)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(GuildMemberProfiles::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_guild_member_profiles_guild_user")
                            .col(GuildMemberProfiles::GuildId)
                            .col(GuildMemberProfiles::DiscordUserId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_guild_member_profiles_guild")
                            .from(GuildMemberProfiles::Table, GuildMemberProfiles::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GuildMemberProfiles::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(GuildMembers::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum GuildMembers {
    Table,
    Id,
    GuildId,
    UserId,
    GuildRoleId,
    DiscordPermissions,
    DiscordNick,
    SyncedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum GuildMemberProfiles {
    Table,
    Id,
    GuildId,
    DiscordUserId,
    DisplayName,
    JoinedAt,
    LastSeenAt,
    IsBot,
    IsBlacklisted,
    CreatedAt,
    UpdatedAt,
}
