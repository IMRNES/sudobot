use sea_orm_migration::prelude::*;

/// Splits the embed card so the welcome and the goodbye can carry different images.
///
/// `m20261019_000002_add_welcome_embed` made the card shared on purpose: "a
/// deployment that wants its logo on a greeting wants it on a farewell too". That
/// holds for the text, but not for the image — a guild reasonably greets with its
/// own banner and says goodbye with something else entirely, and with one shared
/// `embed_image_url` setting the greeting's picture silently replaced the
/// farewell's.
///
/// These five columns are the farewell's own card. `NULL` is meaningful rather
/// than empty: a goodbye with no card of its own keeps the built-in farewell image
/// and posts no title, description, author or thumbnail — which is what a guild
/// that only branded its welcome wants, and it stays reachable after this split
/// instead of being overwritten by the welcome's branding.
///
/// `embed_*` keeps its name and meaning as the welcome's card, so existing rows
/// stay correct without a data migration.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = Alias::new("welcome_configs");

        for column in [
            "goodbye_embed_title",
            "goodbye_embed_description",
            "goodbye_embed_author",
            "goodbye_embed_image_url",
            "goodbye_embed_thumbnail_url",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(table.clone())
                        .add_column(ColumnDef::new(Alias::new(column)).text().null())
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = Alias::new("welcome_configs");

        for column in [
            "goodbye_embed_thumbnail_url",
            "goodbye_embed_image_url",
            "goodbye_embed_author",
            "goodbye_embed_description",
            "goodbye_embed_title",
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(table.clone())
                        .drop_column(Alias::new(column))
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}
