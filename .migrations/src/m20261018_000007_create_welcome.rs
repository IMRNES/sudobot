use sea_orm_migration::prelude::*;

use super::m20261018_000002_create_guilds::Guilds;

/// Welcome / goodbye messages and the button-role panels members self-assign from.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WelcomeConfigs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WelcomeConfigs::GuildId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WelcomeConfigs::WelcomeEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WelcomeConfigs::WelcomeChannelId)
                            .big_integer()
                            .null(),
                    )
                    // Template with {user}, {mention}, {guild}, {memberCount} placeholders.
                    .col(ColumnDef::new(WelcomeConfigs::WelcomeMessage).text().null())
                    .col(
                        ColumnDef::new(WelcomeConfigs::WelcomeEmbedColor)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WelcomeConfigs::WelcomeDmEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WelcomeConfigs::GoodbyeEnabled)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(WelcomeConfigs::GoodbyeChannelId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(WelcomeConfigs::GoodbyeMessage).text().null())
                    .col(
                        ColumnDef::new(WelcomeConfigs::GoodbyeEmbedColor)
                            .integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WelcomeConfigs::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_welcome_configs_guild")
                            .from(WelcomeConfigs::Table, WelcomeConfigs::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // One panel per guild by default; `slug` leaves room for several later
        // without a schema change.
        manager
            .create_table(
                Table::create()
                    .table(RolePanels::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RolePanels::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RolePanels::GuildId).big_integer().not_null())
                    .col(ColumnDef::new(RolePanels::Slug).string_len(48).not_null())
                    .col(ColumnDef::new(RolePanels::Title).string_len(128).not_null())
                    .col(ColumnDef::new(RolePanels::Description).text().null())
                    .col(ColumnDef::new(RolePanels::ChannelId).big_integer().null())
                    // The message the bot posted, so the dashboard can edit it in place.
                    .col(ColumnDef::new(RolePanels::MessageId).big_integer().null())
                    .col(
                        ColumnDef::new(RolePanels::IsPublished)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(RolePanels::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(RolePanels::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_role_panels_guild_slug")
                            .col(RolePanels::GuildId)
                            .col(RolePanels::Slug)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_role_panels_guild")
                            .from(RolePanels::Table, RolePanels::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Each option is one button on the panel. `custom_id` is what Discord
        // sends back when pressed, so it must be stable and globally unique.
        manager
            .create_table(
                Table::create()
                    .table(RolePanelOptions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RolePanelOptions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(RolePanelOptions::PanelId).uuid().not_null())
                    .col(
                        ColumnDef::new(RolePanelOptions::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RolePanelOptions::CustomId)
                            .string_len(100)
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(RolePanelOptions::Label)
                            .string_len(80)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RolePanelOptions::Emoji)
                            .string_len(64)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(RolePanelOptions::RoleId)
                            .big_integer()
                            .not_null(),
                    )
                    // Discord button styles: 1 primary, 2 secondary, 3 success, 4 danger.
                    .col(
                        ColumnDef::new(RolePanelOptions::Style)
                            .small_integer()
                            .not_null()
                            .default(2),
                    )
                    .col(
                        ColumnDef::new(RolePanelOptions::Position)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    // "toggle" | "add" | "remove" | "exclusive"
                    .col(
                        ColumnDef::new(RolePanelOptions::Behavior)
                            .string_len(16)
                            .not_null()
                            .default("toggle"),
                    )
                    .col(
                        ColumnDef::new(RolePanelOptions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_role_panel_options_panel")
                            .from(RolePanelOptions::Table, RolePanelOptions::PanelId)
                            .to(RolePanels::Table, RolePanels::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_role_panel_options_panel")
                    .table(RolePanelOptions::Table)
                    .col(RolePanelOptions::PanelId)
                    .col(RolePanelOptions::Position)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Options before panels — they hold the foreign key. Written out because
        // the drop statement is typed on the iden enum.
        manager
            .drop_table(Table::drop().table(RolePanelOptions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(RolePanels::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WelcomeConfigs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum WelcomeConfigs {
    Table,
    GuildId,
    WelcomeEnabled,
    WelcomeChannelId,
    WelcomeMessage,
    WelcomeEmbedColor,
    WelcomeDmEnabled,
    GoodbyeEnabled,
    GoodbyeChannelId,
    GoodbyeMessage,
    GoodbyeEmbedColor,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum RolePanels {
    Table,
    Id,
    GuildId,
    Slug,
    Title,
    Description,
    ChannelId,
    MessageId,
    IsPublished,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum RolePanelOptions {
    Table,
    Id,
    PanelId,
    GuildId,
    CustomId,
    Label,
    Emoji,
    RoleId,
    Style,
    Position,
    Behavior,
    CreatedAt,
}
