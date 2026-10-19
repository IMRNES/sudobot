//! Welcome configuration and role panels, backed by SeaORM.

use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
};
use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::welcome::{
    OptionBehavior, RolePanel, RolePanelOption, RolePanelWithOptions, WelcomeConfig,
    WelcomeRepository,
};

use super::entities::{role_panel_options, role_panels, welcome_configs};
use super::error::{map_db_err, not_found};
use super::mapping::{from_utc, snowflake, snowflake_from_db, to_utc};
use super::pagination::paginate_models;

const CONFIG: &str = "welcome config";
const PANEL: &str = "role panel";
const OPTION: &str = "role panel option";
const PANEL_CONFLICT: &str = "a panel with that slug already exists in this guild";
const OPTION_CONFLICT: &str = "that button id is already in use";

/// Welcome configuration and role panels, backed by SeaORM.
#[derive(Clone)]
pub struct SeaOrmWelcomeRepository {
    db: DatabaseConnection,
}

impl SeaOrmWelcomeRepository {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// A panel together with its buttons, in button order.
    async fn with_options(
        &self,
        model: role_panels::Model,
    ) -> Result<RolePanelWithOptions, RepositoryError> {
        let options = role_panel_options::Entity::find()
            .filter(role_panel_options::Column::PanelId.eq(model.id))
            .order_by_asc(role_panel_options::Column::Position)
            .order_by_asc(role_panel_options::Column::Id)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(OPTION, OPTION_CONFLICT, error))?;

        Ok(RolePanelWithOptions {
            panel: RolePanel::from(model),
            options: options.into_iter().map(RolePanelOption::from).collect(),
        })
    }
}

impl WelcomeRepository for SeaOrmWelcomeRepository {
    async fn find_config(&self, guild_id: u64) -> Result<WelcomeConfig, RepositoryError> {
        let model = welcome_configs::Entity::find_by_id(snowflake(guild_id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(CONFIG, PANEL_CONFLICT, error))?;

        // No row means the guild has never configured a welcome, which is the
        // defaults — not a missing record the caller has to handle.
        Ok(model
            .map(WelcomeConfig::from)
            .unwrap_or_else(|| WelcomeConfig::default_for(guild_id)))
    }

    async fn upsert_config(&self, config: WelcomeConfig) -> Result<WelcomeConfig, RepositoryError> {
        let model = welcome_configs::ActiveModel {
            guild_id: sea_orm::ActiveValue::Set(snowflake(config.guild_id)),
            welcome_enabled: sea_orm::ActiveValue::Set(config.welcome_enabled),
            welcome_channel_id: sea_orm::ActiveValue::Set(config.welcome_channel_id.map(snowflake)),
            welcome_message: sea_orm::ActiveValue::Set(config.welcome_message),
            welcome_embed_color: sea_orm::ActiveValue::Set(config.welcome_embed_color),
            embed_title: sea_orm::ActiveValue::Set(config.embed_title.clone()),
            embed_description: sea_orm::ActiveValue::Set(config.embed_description.clone()),
            embed_author: sea_orm::ActiveValue::Set(config.embed_author.clone()),
            embed_image_url: sea_orm::ActiveValue::Set(config.embed_image_url.clone()),
            embed_thumbnail_url: sea_orm::ActiveValue::Set(config.embed_thumbnail_url.clone()),
            welcome_dm_enabled: sea_orm::ActiveValue::Set(config.welcome_dm_enabled),
            goodbye_enabled: sea_orm::ActiveValue::Set(config.goodbye_enabled),
            goodbye_channel_id: sea_orm::ActiveValue::Set(config.goodbye_channel_id.map(snowflake)),
            goodbye_message: sea_orm::ActiveValue::Set(config.goodbye_message),
            goodbye_embed_color: sea_orm::ActiveValue::Set(config.goodbye_embed_color),
            updated_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
        };

        welcome_configs::Entity::insert(model)
            .on_conflict(
                OnConflict::column(welcome_configs::Column::GuildId)
                    .update_columns([
                        welcome_configs::Column::WelcomeEnabled,
                        welcome_configs::Column::WelcomeChannelId,
                        welcome_configs::Column::WelcomeMessage,
                        welcome_configs::Column::WelcomeEmbedColor,
                        // The card columns belong here too: `on_conflict` only
                        // writes the columns it is given, so a column that is `Set`
                        // above but absent from this list is silently discarded on
                        // every update to an existing row.
                        welcome_configs::Column::EmbedTitle,
                        welcome_configs::Column::EmbedDescription,
                        welcome_configs::Column::EmbedAuthor,
                        welcome_configs::Column::EmbedImageUrl,
                        welcome_configs::Column::EmbedThumbnailUrl,
                        welcome_configs::Column::WelcomeDmEnabled,
                        welcome_configs::Column::GoodbyeEnabled,
                        welcome_configs::Column::GoodbyeChannelId,
                        welcome_configs::Column::GoodbyeMessage,
                        welcome_configs::Column::GoodbyeEmbedColor,
                        welcome_configs::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(CONFIG, PANEL_CONFLICT, error))?;

        self.find_config(config.guild_id).await
    }

    async fn find_panel(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<Option<RolePanelWithOptions>, RepositoryError> {
        let model = role_panels::Entity::find()
            .filter(role_panels::Column::GuildId.eq(snowflake(guild_id)))
            .filter(role_panels::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(PANEL, PANEL_CONFLICT, error))?;

        match model {
            Some(model) => Ok(Some(self.with_options(model).await?)),
            None => Ok(None),
        }
    }

    async fn find_panel_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<RolePanelWithOptions>, RepositoryError> {
        let model = role_panels::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(PANEL, PANEL_CONFLICT, error))?;

        match model {
            Some(model) => Ok(Some(self.with_options(model).await?)),
            None => Ok(None),
        }
    }

    async fn list_panels(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<RolePanel>, RepositoryError> {
        let query = role_panels::Entity::find()
            .filter(role_panels::Column::GuildId.eq(snowflake(guild_id)))
            .order_by_asc(role_panels::Column::Slug);

        paginate_models(
            query,
            &self.db,
            params,
            PANEL,
            PANEL_CONFLICT,
            RolePanel::from,
        )
        .await
    }

    async fn upsert_panel(&self, panel: RolePanel) -> Result<RolePanel, RepositoryError> {
        let now = from_utc(Utc::now());
        let guild_id = snowflake(panel.guild_id);
        // Kept for the read-back below: the value is moved into the insert.
        let slug = panel.slug.clone();

        let model = role_panels::ActiveModel {
            id: sea_orm::ActiveValue::Set(panel.id),
            guild_id: sea_orm::ActiveValue::Set(guild_id),
            slug: sea_orm::ActiveValue::Set(panel.slug),
            title: sea_orm::ActiveValue::Set(panel.title),
            description: sea_orm::ActiveValue::Set(panel.description),
            channel_id: sea_orm::ActiveValue::Set(panel.channel_id.map(snowflake)),
            message_id: sea_orm::ActiveValue::Set(panel.message_id.map(snowflake)),
            is_published: sea_orm::ActiveValue::Set(panel.is_published),
            created_at: sea_orm::ActiveValue::Set(now),
            updated_at: sea_orm::ActiveValue::Set(now),
        };

        // The natural key is (guild, slug), not the id: the dashboard saves a panel
        // by slug, and creating "the welcome panel" twice must edit the first one
        // rather than fail on the unique index.
        role_panels::Entity::insert(model)
            .on_conflict(
                OnConflict::columns([role_panels::Column::GuildId, role_panels::Column::Slug])
                    .update_columns([
                        role_panels::Column::Title,
                        role_panels::Column::Description,
                        role_panels::Column::ChannelId,
                        role_panels::Column::IsPublished,
                        role_panels::Column::UpdatedAt,
                    ])
                    .to_owned(),
            )
            .exec_without_returning(&self.db)
            .await
            .map_err(|error| map_db_err(PANEL, PANEL_CONFLICT, error))?;

        let stored = role_panels::Entity::find()
            .filter(role_panels::Column::GuildId.eq(guild_id))
            .filter(role_panels::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(PANEL, PANEL_CONFLICT, error))?
            .ok_or_else(|| not_found(PANEL))?;

        Ok(stored.into())
    }

    async fn mark_published(
        &self,
        id: Uuid,
        channel_id: u64,
        message_id: u64,
    ) -> Result<RolePanel, RepositoryError> {
        let active = role_panels::ActiveModel {
            id: sea_orm::ActiveValue::Set(id),
            channel_id: sea_orm::ActiveValue::Set(Some(snowflake(channel_id))),
            message_id: sea_orm::ActiveValue::Set(Some(snowflake(message_id))),
            is_published: sea_orm::ActiveValue::Set(true),
            updated_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
            ..Default::default()
        };

        let updated = role_panels::Entity::update(active)
            .exec(&self.db)
            .await
            .map_err(|error| match error {
                sea_orm::DbErr::RecordNotUpdated => not_found(PANEL),
                other => map_db_err(PANEL, PANEL_CONFLICT, other),
            })?;

        Ok(updated.into())
    }

    async fn delete_panel(&self, id: Uuid) -> Result<(), RepositoryError> {
        // The options cascade, so this is one statement and no orphan buttons.
        let result = role_panels::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(PANEL, PANEL_CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(PANEL));
        }

        Ok(())
    }

    async fn find_option_by_custom_id(
        &self,
        custom_id: &str,
    ) -> Result<Option<RolePanelOption>, RepositoryError> {
        // The hot path for every button press in every guild, which is what the
        // unique index on `custom_id` exists for.
        let model = role_panel_options::Entity::find()
            .filter(role_panel_options::Column::CustomId.eq(custom_id))
            .one(&self.db)
            .await
            .map_err(|error| map_db_err(OPTION, OPTION_CONFLICT, error))?;

        Ok(model.map(RolePanelOption::from))
    }

    async fn list_options(&self, panel_id: Uuid) -> Result<Vec<RolePanelOption>, RepositoryError> {
        let models = role_panel_options::Entity::find()
            .filter(role_panel_options::Column::PanelId.eq(panel_id))
            .order_by_asc(role_panel_options::Column::Position)
            .order_by_asc(role_panel_options::Column::Id)
            .all(&self.db)
            .await
            .map_err(|error| map_db_err(OPTION, OPTION_CONFLICT, error))?;

        Ok(models.into_iter().map(RolePanelOption::from).collect())
    }

    async fn create_option(
        &self,
        option: RolePanelOption,
    ) -> Result<RolePanelOption, RepositoryError> {
        let model = role_panel_options::ActiveModel {
            id: sea_orm::ActiveValue::Set(option.id),
            panel_id: sea_orm::ActiveValue::Set(option.panel_id),
            guild_id: sea_orm::ActiveValue::Set(snowflake(option.guild_id)),
            custom_id: sea_orm::ActiveValue::Set(option.custom_id),
            label: sea_orm::ActiveValue::Set(option.label),
            emoji: sea_orm::ActiveValue::Set(option.emoji),
            role_id: sea_orm::ActiveValue::Set(snowflake(option.role_id)),
            style: sea_orm::ActiveValue::Set(option.style),
            position: sea_orm::ActiveValue::Set(option.position),
            behavior: sea_orm::ActiveValue::Set(option.behavior.as_str().to_string()),
            created_at: sea_orm::ActiveValue::Set(from_utc(Utc::now())),
        };

        let created = role_panel_options::Entity::insert(model)
            .exec_with_returning(&self.db)
            .await
            .map_err(|error| map_db_err(OPTION, OPTION_CONFLICT, error))?;

        Ok(created.into())
    }

    async fn delete_option(&self, id: Uuid) -> Result<(), RepositoryError> {
        let result = role_panel_options::Entity::delete_by_id(id)
            .exec(&self.db)
            .await
            .map_err(|error| map_db_err(OPTION, OPTION_CONFLICT, error))?;

        if result.rows_affected == 0 {
            return Err(not_found(OPTION));
        }

        Ok(())
    }

    async fn count_options(&self, panel_id: Uuid) -> Result<usize, RepositoryError> {
        // The next `Position` is the count, so this is read on every option add.
        let total = role_panel_options::Entity::find()
            .filter(role_panel_options::Column::PanelId.eq(panel_id))
            .count(&self.db)
            .await
            .map_err(|error| map_db_err(OPTION, OPTION_CONFLICT, error))?;

        Ok(total as usize)
    }
}

impl From<welcome_configs::Model> for WelcomeConfig {
    fn from(model: welcome_configs::Model) -> Self {
        Self {
            guild_id: snowflake_from_db(model.guild_id),
            welcome_enabled: model.welcome_enabled,
            welcome_channel_id: model.welcome_channel_id.map(snowflake_from_db),
            welcome_message: model.welcome_message,
            welcome_embed_color: model.welcome_embed_color,
            embed_title: model.embed_title,
            embed_description: model.embed_description,
            embed_author: model.embed_author,
            embed_image_url: model.embed_image_url,
            embed_thumbnail_url: model.embed_thumbnail_url,
            welcome_dm_enabled: model.welcome_dm_enabled,
            goodbye_enabled: model.goodbye_enabled,
            goodbye_channel_id: model.goodbye_channel_id.map(snowflake_from_db),
            goodbye_message: model.goodbye_message,
            goodbye_embed_color: model.goodbye_embed_color,
            updated_at: to_utc(model.updated_at),
        }
    }
}

impl From<role_panels::Model> for RolePanel {
    fn from(model: role_panels::Model) -> Self {
        Self {
            id: model.id,
            guild_id: snowflake_from_db(model.guild_id),
            slug: model.slug,
            title: model.title,
            description: model.description,
            channel_id: model.channel_id.map(snowflake_from_db),
            message_id: model.message_id.map(snowflake_from_db),
            is_published: model.is_published,
            created_at: to_utc(model.created_at),
            updated_at: to_utc(model.updated_at),
        }
    }
}

impl From<role_panel_options::Model> for RolePanelOption {
    fn from(model: role_panel_options::Model) -> Self {
        Self {
            id: model.id,
            panel_id: model.panel_id,
            guild_id: snowflake_from_db(model.guild_id),
            custom_id: model.custom_id,
            label: model.label,
            emoji: model.emoji,
            role_id: snowflake_from_db(model.role_id),
            style: model.style,
            position: model.position,
            // The column is a free-form `varchar` with `toggle` as its default, so a
            // value this build does not recognise — written by a newer version, or
            // by hand — falls back to that same default rather than failing the read
            // of the whole panel.
            behavior: OptionBehavior::parse(&model.behavior).unwrap_or(OptionBehavior::Toggle),
        }
    }
}
