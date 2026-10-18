use sea_orm_migration::prelude::*;

use super::m20261018_000002_create_guilds::Guilds;

/// The permission catalog, per-guild roles, and the matrix joining them.
///
/// Two layers, matching how Discord itself thinks: a *permission* is a capability
/// (`economy:write`), a *role* is a named bundle of capabilities inside one guild.
/// Keeping roles in a table rather than an enum is what lets the dashboard edit
/// the matrix without a deploy.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Global catalog: the same permission names exist for every guild, so
        // they are seeded once by bootstrap rather than per guild.
        manager
            .create_table(
                Table::create()
                    .table(Permissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Permissions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Permissions::Name)
                            .string_len(64)
                            .not_null()
                            .unique_key(),
                    )
                    .col(ColumnDef::new(Permissions::Group).string_len(32).not_null())
                    .col(ColumnDef::new(Permissions::Description).text().not_null())
                    .col(
                        ColumnDef::new(Permissions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(GuildRoles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GuildRoles::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(GuildRoles::GuildId).big_integer().not_null())
                    .col(ColumnDef::new(GuildRoles::Slug).string_len(32).not_null())
                    .col(ColumnDef::new(GuildRoles::Name).string_len(64).not_null())
                    .col(ColumnDef::new(GuildRoles::Description).text().null())
                    // System roles are seeded with the guild and cannot be
                    // deleted from the dashboard; only their matrix is editable.
                    .col(
                        ColumnDef::new(GuildRoles::IsSystem)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(GuildRoles::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(GuildRoles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(GuildRoles::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_guild_roles_guild_slug")
                            .col(GuildRoles::GuildId)
                            .col(GuildRoles::Slug)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_guild_roles_guild")
                            .from(GuildRoles::Table, GuildRoles::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Junction table. Both columns are the primary key: a permission is
        // either granted to a role or it is not, and duplicates would be
        // meaningless rows.
        manager
            .create_table(
                Table::create()
                    .table(GuildRolePermissions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(GuildRolePermissions::GuildRoleId)
                            .uuid()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(GuildRolePermissions::PermissionId)
                            .uuid()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(GuildRolePermissions::GuildRoleId)
                            .col(GuildRolePermissions::PermissionId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_grp_role")
                            .from(
                                GuildRolePermissions::Table,
                                GuildRolePermissions::GuildRoleId,
                            )
                            .to(GuildRoles::Table, GuildRoles::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_grp_permission")
                            .from(
                                GuildRolePermissions::Table,
                                GuildRolePermissions::PermissionId,
                            )
                            .to(Permissions::Table, Permissions::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(GuildRolePermissions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(GuildRoles::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Permissions::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Permissions {
    Table,
    Id,
    Name,
    Group,
    Description,
    CreatedAt,
}

#[derive(DeriveIden)]
pub enum GuildRoles {
    Table,
    Id,
    GuildId,
    Slug,
    Name,
    Description,
    IsSystem,
    Position,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum GuildRolePermissions {
    Table,
    GuildRoleId,
    PermissionId,
}
