use sea_orm_migration::prelude::*;

/// The role the level ladder must sit above.
///
/// The ladder was positioned just above `@everyone`, on the reasoning that a
/// cosmetic title belongs at the bottom of the hierarchy. That is wrong for any
/// server with a base member role: a guild whose members all hold "Users" wants the
/// ladder **above** it, otherwise every badge renders below the plain member role
/// and the climb reads upside down in the member list.
///
/// Stored rather than guessed. Discord has no "this is the default member role"
/// flag — a server can call it anything, or grant it to only some members — so
/// guessing from the name would break on the next server and guessing from who
/// holds it would re-derive the answer on every deploy. An explicit role id is the
/// only form that is both correct and inspectable.
///
/// Nullable, and null means "just above `@everyone`", which keeps a guild that
/// never set it working exactly as it did before.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("leveling_configs"))
                    .add_column(
                        ColumnDef::new(Alias::new("ladder_base_role_id"))
                            .big_integer()
                            .null(),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("leveling_configs"))
                    .drop_column(Alias::new("ladder_base_role_id"))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}
