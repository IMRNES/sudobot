use sea_orm_migration::prelude::*;

/// Keeps the note an admin typed on an adjustment.
///
/// `reason` holds a category — `admin_grant`, `purchase`, `daily` — because the
/// ledger's rules are keyed on it (only `admin_deduct` may overdraw, for instance).
/// It was also the only place the admin's free text went, and it did not: the
/// typed reason was logged and dropped, so a ledger row read "admin_grant" and
/// nothing explained why somebody was given 5000 currency.
///
/// `note` is that explanation. Nullable, because machine-written rows (a daily
/// claim, a purchase) have nothing to say beyond their category.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(EconomyTransactions::Table)
                    .add_column(ColumnDef::new(EconomyTransactions::Note).text().null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(EconomyTransactions::Table)
                    .drop_column(EconomyTransactions::Note)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum EconomyTransactions {
    Table,
    Note,
}
