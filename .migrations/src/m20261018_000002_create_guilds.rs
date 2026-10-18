use sea_orm_migration::prelude::*;

/// Discord guilds the bot is installed in.
///
/// `guild_id` is Discord's snowflake and is the natural key every other
/// guild-scoped table points at, so it is stored as `BIGINT` without a surrogate.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Guilds::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Guilds::Id)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Guilds::Name).string_len(100).not_null())
                    .col(ColumnDef::new(Guilds::IconUrl).text().null())
                    .col(ColumnDef::new(Guilds::OwnerDiscordId).big_integer().null())
                    .col(
                        ColumnDef::new(Guilds::MemberCount)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    // Flipped by the bot when it joins or leaves; the dashboard
                    // stops offering a guild whose bot is gone.
                    .col(
                        ColumnDef::new(Guilds::BotPresent)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Guilds::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Guilds::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_guilds_bot_present")
                    .table(Guilds::Table)
                    .col(Guilds::BotPresent)
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Guilds::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum Guilds {
    Table,
    Id,
    Name,
    IconUrl,
    OwnerDiscordId,
    MemberCount,
    BotPresent,
    CreatedAt,
    UpdatedAt,
}
