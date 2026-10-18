use sea_orm_migration::prelude::*;

use super::m20261018_000002_create_guilds::Guilds;

/// XP, levels and the per-guild leveling configuration.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(LevelingConfigs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LevelingConfigs::GuildId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::XpMin)
                            .integer()
                            .not_null()
                            .default(15),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::XpMax)
                            .integer()
                            .not_null()
                            .default(25),
                    )
                    // Seconds between two XP grants for the same member. Without
                    // this, a fast typist out-levels everyone.
                    .col(
                        ColumnDef::new(LevelingConfigs::CooldownSecs)
                            .integer()
                            .not_null()
                            .default(60),
                    )
                    // Multiplier applied to voice-channel time.
                    .col(
                        ColumnDef::new(LevelingConfigs::VoiceXpMultiplier)
                            .double()
                            .not_null()
                            .default(1.0),
                    )
                    // "mrn" | "mro" — Minecraft-style curve by default.
                    .col(
                        ColumnDef::new(LevelingConfigs::Curve)
                            .string_len(8)
                            .not_null()
                            .default("mrn"),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::AnnounceChannelId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::StackRoles)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::IgnoredChannelIds)
                            .array(ColumnType::BigInteger)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::IgnoredRoleIds)
                            .array(ColumnType::BigInteger)
                            .null(),
                    )
                    .col(
                        ColumnDef::new(LevelingConfigs::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_leveling_configs_guild")
                            .from(LevelingConfigs::Table, LevelingConfigs::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(LevelingProfiles::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LevelingProfiles::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::DiscordUserId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::Xp)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::Level)
                            .integer()
                            .not_null()
                            .default(0),
                    )
                    // Total XP ever earned, which does not drop when the level curve
                    // is recalculated or a penalty is applied.
                    .col(
                        ColumnDef::new(LevelingProfiles::TotalXp)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::MessageCount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::VoiceSeconds)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::LastXpAt)
                            .timestamp_with_time_zone()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(LevelingProfiles::UpdatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_leveling_profiles_guild_user")
                            .col(LevelingProfiles::GuildId)
                            .col(LevelingProfiles::DiscordUserId)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_leveling_profiles_guild")
                            .from(LevelingProfiles::Table, LevelingProfiles::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_leveling_profiles_leaderboard")
                    .table(LevelingProfiles::Table)
                    .col(LevelingProfiles::GuildId)
                    .col(LevelingProfiles::Xp)
                    .to_owned(),
            )
            .await?;

        // Reward roles: reaching `level` in this guild grants `role_id`.
        manager
            .create_table(
                Table::create()
                    .table(LevelingRewards::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LevelingRewards::Id)
                            .uuid()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LevelingRewards::GuildId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(LevelingRewards::Level).integer().not_null())
                    .col(
                        ColumnDef::new(LevelingRewards::RoleId)
                            .big_integer()
                            .not_null(),
                    )
                    // When false, the role is removed again if the member drops below
                    // the level (after a reset or penalty).
                    .col(
                        ColumnDef::new(LevelingRewards::KeepOnDemote)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(LevelingRewards::CreatedAt)
                            .timestamp_with_time_zone()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .index(
                        Index::create()
                            .name("uq_leveling_rewards_guild_level")
                            .col(LevelingRewards::GuildId)
                            .col(LevelingRewards::Level)
                            .unique(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_leveling_rewards_guild")
                            .from(LevelingRewards::Table, LevelingRewards::GuildId)
                            .to(Guilds::Table, Guilds::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Children first, then the config the others do not reference. Each is
        // written out because the drop statement is typed on the iden enum.
        manager
            .drop_table(Table::drop().table(LevelingRewards::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(LevelingProfiles::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(LevelingConfigs::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
pub enum LevelingConfigs {
    Table,
    GuildId,
    Enabled,
    XpMin,
    XpMax,
    CooldownSecs,
    VoiceXpMultiplier,
    Curve,
    AnnounceChannelId,
    StackRoles,
    IgnoredChannelIds,
    IgnoredRoleIds,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum LevelingProfiles {
    Table,
    Id,
    GuildId,
    DiscordUserId,
    Xp,
    Level,
    TotalXp,
    MessageCount,
    VoiceSeconds,
    LastXpAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum LevelingRewards {
    Table,
    Id,
    GuildId,
    Level,
    RoleId,
    KeepOnDemote,
    CreatedAt,
}
