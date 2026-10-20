use sea_orm_migration::prelude::*;

use super::m20261018_000002_create_guilds::Guilds;

/// The jail: a role that denies every channel but one, and the sentences served on it.
///
/// Two tables rather than columns on `guilds`. The configuration is per guild and
/// the sentences are per (guild, member), so folding them together would either
/// duplicate the config per prisoner or need a join table anyway.
///
/// The role and channel ids are stored explicitly, never derived. A server can call
/// its jail role "prisoner", "jailed", "muted" or "timeout" and its channel anything
/// at all, so matching on a name breaks the first time somebody renames it, and
/// inferring the channel from "the one everyone can see" is not answerable from
/// Discord's API.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(JailConfigs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(JailConfigs::GuildId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(JailConfigs::JailRoleId).big_integer().null())
                    .col(
                        ColumnDef::new(JailConfigs::JailChannelId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(JailConfigs::DefaultSentenceSeconds)
                            .big_integer()
                            .null(),
                    )
                    // Null disables bail entirely rather than making it free: a free
                    // bail button would turn every sentence into an instant release.
                    .col(ColumnDef::new(JailConfigs::BailAmount).big_integer().null())
                    .col(
                        ColumnDef::new(JailConfigs::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_jail_configs_guild")
                            .from(JailConfigs::Table, JailConfigs::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(JailSentences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(JailSentences::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(JailSentences::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(JailSentences::JailedBy)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(JailSentences::Reason).text().not_null())
                    // Null is an indefinite sentence: it ends when a moderator says
                    // so, never on a timer. Storing a far-future date instead would
                    // be indistinguishable from a very long but finite sentence.
                    .col(
                        ColumnDef::new(JailSentences::ExpiresAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(JailSentences::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .primary_key(
                        Index::create()
                            .col(JailSentences::GuildId)
                            .col(JailSentences::DiscordUserId),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_jail_sentences_guild")
                            .from(JailSentences::Table, JailSentences::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // The sweeper asks "what has expired across every guild" on a timer, so the
        // deadline needs its own index. Without one, that query scans every live
        // sentence on every tick, forever.
        manager
            .create_index(
                Index::create()
                    .name("idx_jail_sentences_expires_at")
                    .table(JailSentences::Table)
                    .col(JailSentences::ExpiresAt)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(JailSentences::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(JailConfigs::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum JailConfigs {
    Table,
    GuildId,
    JailRoleId,
    JailChannelId,
    DefaultSentenceSeconds,
    BailAmount,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum JailSentences {
    Table,
    GuildId,
    DiscordUserId,
    JailedBy,
    Reason,
    ExpiresAt,
    CreatedAt,
}
