use sea_orm_migration::prelude::*;

use super::m20261018_000002_create_guilds::Guilds;

/// Roles granted automatically, either immediately on join or after a delay.
///
/// Split from the role-panel tables because the trigger and the bookkeeping are
/// different concerns: a panel option is pressed by a member, an autorole is
/// applied by the bot.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AutoRoles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AutoRoles::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(AutoRoles::GuildId).big_integer().not_null())
                    .col(ColumnDef::new(AutoRoles::RoleId).big_integer().not_null())
                    // "join" | "delay" | "human" — how the grant is triggered.
                    .col(
                        ColumnDef::new(AutoRoles::Trigger)
                            .string_len(16)
                            .not_null()
                            .default("join"),
                    )
                    // Seconds to wait before granting; only meaningful for "delay".
                    // A common anti-raid trick: hand out the role only to members
                    // who are still present ten minutes later.
                    .col(
                        ColumnDef::new(AutoRoles::DelaySeconds)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    // Skip bots, or grant only to bots.
                    .col(
                        ColumnDef::new(AutoRoles::AppliesToBots)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(AutoRoles::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(AutoRoles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AutoRoles::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // One rule per role: two rules granting the same role would
                    // race each other on join.
                    .index(
                        Index::create()
                            .name("uq_auto_roles_guild_role")
                            .col(AutoRoles::GuildId)
                            .col(AutoRoles::RoleId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_auto_roles_guild")
                            .from(AutoRoles::Table, AutoRoles::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Pending delayed grants. The bot sweeps this table on an interval rather
        // than sleeping per member, so a restart does not lose scheduled grants.
        manager
            .create_table(
                Table::create()
                    .table(AutoRolePending::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AutoRolePending::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AutoRolePending::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutoRolePending::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutoRolePending::AutoRoleId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutoRolePending::RoleId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutoRolePending::DueAt)
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutoRolePending::Attempts)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(AutoRolePending::LastError).text().null())
                    .col(
                        ColumnDef::new(AutoRolePending::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    // A member leaving and rejoining must not queue the same grant twice.
                    .index(
                        Index::create()
                            .name("uq_auto_role_pending_rule_member")
                            .col(AutoRolePending::AutoRoleId)
                            .col(AutoRolePending::DiscordUserId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_auto_role_pending_rule")
                            .from(AutoRolePending::Table, AutoRolePending::AutoRoleId)
                            .to(AutoRoles::Table, AutoRoles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // The sweep query: everything due, oldest first.
        manager
            .create_index(
                Index::create()
                    .name("idx_auto_role_pending_due")
                    .table(AutoRolePending::Table)
                    .col(AutoRolePending::DueAt)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AutoRolePending::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AutoRoles::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum AutoRoles {
    Table,
    Id,
    GuildId,
    RoleId,
    Trigger,
    DelaySeconds,
    AppliesToBots,
    IsActive,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum AutoRolePending {
    Table,
    Id,
    GuildId,
    DiscordUserId,
    AutoRoleId,
    RoleId,
    DueAt,
    Attempts,
    LastError,
    CreatedAt,
}
