use std::future::Future;
use uuid::Uuid;

use super::entity::{RolePanel, RolePanelOption, RolePanelWithOptions, WelcomeConfig};
use crate::domain::errors::{Page, PaginationParams, RepositoryError};

pub trait WelcomeRepository: Send + Sync {
    /// The guild's configuration, or the defaults when it has never been set.
    fn find_config(
        &self,
        guild_id: u64,
    ) -> impl Future<Output = Result<WelcomeConfig, RepositoryError>> + Send;

    fn upsert_config(
        &self,
        config: WelcomeConfig,
    ) -> impl Future<Output = Result<WelcomeConfig, RepositoryError>> + Send;

    fn find_panel(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> impl Future<Output = Result<Option<RolePanelWithOptions>, RepositoryError>> + Send;

    fn find_panel_by_id(
        &self,
        id: Uuid,
    ) -> impl Future<Output = Result<Option<RolePanelWithOptions>, RepositoryError>> + Send;

    fn list_panels(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> impl Future<Output = Result<Page<RolePanel>, RepositoryError>> + Send;

    fn upsert_panel(
        &self,
        panel: RolePanel,
    ) -> impl Future<Output = Result<RolePanel, RepositoryError>> + Send;

    /// Records where the panel was posted, so later edits update that message.
    fn mark_published(
        &self,
        id: Uuid,
        channel_id: u64,
        message_id: u64,
    ) -> impl Future<Output = Result<RolePanel, RepositoryError>> + Send;

    fn delete_panel(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    /// Looks a button up by the id Discord sends back on press.
    ///
    /// This is the hot path for every button click in every guild, which is why
    /// `custom_id` carries a unique index.
    fn find_option_by_custom_id(
        &self,
        custom_id: &str,
    ) -> impl Future<Output = Result<Option<RolePanelOption>, RepositoryError>> + Send;

    fn list_options(
        &self,
        panel_id: Uuid,
    ) -> impl Future<Output = Result<Vec<RolePanelOption>, RepositoryError>> + Send;

    fn create_option(
        &self,
        option: RolePanelOption,
    ) -> impl Future<Output = Result<RolePanelOption, RepositoryError>> + Send;

    fn delete_option(&self, id: Uuid) -> impl Future<Output = Result<(), RepositoryError>> + Send;

    fn count_options(
        &self,
        panel_id: Uuid,
    ) -> impl Future<Output = Result<usize, RepositoryError>> + Send;
}
