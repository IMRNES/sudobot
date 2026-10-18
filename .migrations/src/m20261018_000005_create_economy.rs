use sea_orm_migration::prelude::*;

use super::m20261018_000002_create_guilds::Guilds;

/// Virtual currency: balances, the transaction ledger, and the shop.
///
/// The ledger is append-only. A balance is a cached projection of it, kept on
/// `economy_accounts` so the common read (show my balance) does not scan history,
/// while every mutation is still individually explainable.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(EconomyAccounts::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EconomyAccounts::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EconomyAccounts::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyAccounts::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    // BIGINT, because an economy that cannot exceed 2^31 units is
                    // a support ticket waiting to happen.
                    .col(
                        ColumnDef::new(EconomyAccounts::Balance)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(EconomyAccounts::TotalEarned)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(EconomyAccounts::TotalSpent)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    // Timestamp of the last daily claim; null means never claimed.
                    .col(
                        ColumnDef::new(EconomyAccounts::LastDailyAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EconomyAccounts::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(EconomyAccounts::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_economy_accounts_guild_user")
                            .col(EconomyAccounts::GuildId)
                            .col(EconomyAccounts::DiscordUserId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_economy_accounts_guild")
                            .from(EconomyAccounts::Table, EconomyAccounts::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Leaderboard reads: top balances within one guild.
        manager
            .create_index(
                Index::create()
                    .name("idx_economy_accounts_leaderboard")
                    .table(EconomyAccounts::Table)
                    .col(EconomyAccounts::GuildId)
                    .col(EconomyAccounts::Balance)
                    .to_owned(),
            )
            .await?;

        // Append-only ledger. `balance_after` is denormalized on purpose: it makes
        // a dispute resolvable from one row instead of replaying history.
        manager
            .create_table(
                Table::create()
                    .table(EconomyTransactions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EconomyTransactions::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EconomyTransactions::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyTransactions::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    // Signed: negative is a debit, positive a credit.
                    .col(
                        ColumnDef::new(EconomyTransactions::Amount)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyTransactions::BalanceAfter)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyTransactions::Reason)
                            .string_len(48)
                            .not_null(),
                    )
                    // Who caused it, when it was not the account owner (admin grant,
                    // transfer counterparty).
                    .col(
                        ColumnDef::new(EconomyTransactions::ActorDiscordId)
                            .big_integer()
                            .null(),
                    )
                    // Idempotency key: a retried bot interaction must not pay twice.
                    .col(
                        ColumnDef::new(EconomyTransactions::IdempotencyKey)
                            .string_len(96)
                            .null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(EconomyTransactions::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_economy_transactions_guild")
                            .from(EconomyTransactions::Table, EconomyTransactions::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_economy_transactions_user_created")
                    .table(EconomyTransactions::Table)
                    .col(EconomyTransactions::GuildId)
                    .col(EconomyTransactions::DiscordUserId)
                    .col(EconomyTransactions::CreatedAt)
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(EconomyShopItems::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EconomyShopItems::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EconomyShopItems::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyShopItems::Slug)
                            .string_len(48)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyShopItems::Name)
                            .string_len(96)
                            .not_null(),
                    )
                    .col(ColumnDef::new(EconomyShopItems::Description).text().null())
                    .col(
                        ColumnDef::new(EconomyShopItems::Price)
                            .big_integer()
                            .not_null(),
                    )
                    // Discord role granted on purchase, when the item represents one.
                    .col(
                        ColumnDef::new(EconomyShopItems::RoleId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(EconomyShopItems::Stock).integer().null())
                    .col(
                        ColumnDef::new(EconomyShopItems::IsActive)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(EconomyShopItems::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(EconomyShopItems::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_economy_shop_items_guild_slug")
                            .col(EconomyShopItems::GuildId)
                            .col(EconomyShopItems::Slug)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_economy_shop_items_guild")
                            .from(EconomyShopItems::Table, EconomyShopItems::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(EconomyPurchases::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(EconomyPurchases::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(EconomyPurchases::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyPurchases::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(EconomyPurchases::ItemId).uuid().not_null())
                    .col(
                        ColumnDef::new(EconomyPurchases::PricePaid)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(EconomyPurchases::TransactionId)
                            .uuid()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(EconomyPurchases::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_economy_purchases_item")
                            .from(EconomyPurchases::Table, EconomyPurchases::ItemId)
                            .to(EconomyShopItems::Table, EconomyShopItems::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_economy_purchases_transaction")
                            .from(EconomyPurchases::Table, EconomyPurchases::TransactionId)
                            .to(EconomyTransactions::Table, EconomyTransactions::Id)
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Each drop is written out because `Table::drop().table(..)` is typed on
        // the iden enum, so a heterogeneous array cannot express the sequence.
        // Order is the reverse of creation: children before the tables they point
        // at, or the foreign keys refuse the drop.
        manager
            .drop_table(Table::drop().table(EconomyPurchases::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(EconomyShopItems::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(EconomyTransactions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(EconomyAccounts::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum EconomyAccounts {
    Table,
    Id,
    GuildId,
    DiscordUserId,
    Balance,
    TotalEarned,
    TotalSpent,
    LastDailyAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum EconomyTransactions {
    Table,
    Id,
    GuildId,
    DiscordUserId,
    Amount,
    BalanceAfter,
    Reason,
    ActorDiscordId,
    IdempotencyKey,
    CreatedAt,
}

#[derive(DeriveIden)]
pub enum EconomyShopItems {
    Table,
    Id,
    GuildId,
    Slug,
    Name,
    Description,
    Price,
    RoleId,
    Stock,
    IsActive,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum EconomyPurchases {
    Table,
    Id,
    GuildId,
    DiscordUserId,
    ItemId,
    PricePaid,
    TransactionId,
    CreatedAt,
}
