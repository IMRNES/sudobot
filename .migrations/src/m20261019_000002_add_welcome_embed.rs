use sea_orm_migration::prelude::*;

/// The rich embed the welcome and goodbye messages carry.
///
/// Until now a welcome could only contribute a colour to its embed, which made the
/// card a bare block of text. These five columns are the rest of what a Discord
/// embed can show, and they are the shape the community's previous bot posted: a
/// title, a description, an author line, a large image and a small thumbnail.
///
/// Deliberately **shared** by welcome and goodbye rather than duplicated: this is
/// the server's own branding card, and a deployment that wants its logo on a
/// greeting wants it on a farewell too. Colour stays per-kind, because that is the
/// one field an admin reasonably wants to differ between the two.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let table = Alias::new("welcome_configs");

        for column in [
            "embed_title",
            "embed_description",
            "embed_author",
            "embed_image_url",
            "embed_thumbnail_url",
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
            "embed_thumbnail_url",
            "embed_image_url",
            "embed_author",
            "embed_description",
            "embed_title",
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
