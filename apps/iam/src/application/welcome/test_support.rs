// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Hand-written fakes for the welcome and panel use cases.
//!
//! `WelcomeRepository` is fourteen methods, and the tests need six of them, so
//! this fake stores the two tables and answers the rest with the obvious
//! in-memory equivalent. No database and no mock framework.

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::welcome::{
    RolePanel, RolePanelOption, RolePanelWithOptions, WelcomeConfig, WelcomeRepository,
};

/// Panels and configuration, in memory.
#[derive(Default)]
pub struct FakeWelcomeStore {
    pub configs: Vec<WelcomeConfig>,
    pub panels: Vec<RolePanel>,
    pub options: Vec<RolePanelOption>,
}

impl FakeWelcomeStore {
    pub fn new() -> Self {
        Self::default()
    }
}

pub type Store = Arc<Mutex<FakeWelcomeStore>>;

pub fn store() -> Store {
    Arc::new(Mutex::new(FakeWelcomeStore::new()))
}

fn lock(store: &Store) -> std::sync::MutexGuard<'_, FakeWelcomeStore> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Seeds a configuration for one guild.
pub fn seed_config(store: &Store, config: WelcomeConfig) {
    let mut state = lock(store);
    state
        .configs
        .retain(|stored| stored.guild_id != config.guild_id);
    state.configs.push(config);
}

/// Seeds a panel with the options given.
pub fn seed_panel(store: &Store, panel: RolePanel, options: Vec<RolePanelOption>) {
    let mut state = lock(store);
    state.panels.push(panel);
    state.options.extend(options);
}

/// A button, with the custom id the domain builds.
pub fn option(panel_id: Uuid, guild_id: u64, position: usize, role_id: u64) -> RolePanelOption {
    RolePanelOption {
        id: Uuid::new_v4(),
        panel_id,
        guild_id,
        custom_id: RolePanelOption::build_custom_id(panel_id, position),
        label: format!("Role {role_id}"),
        emoji: None,
        role_id,
        style: 2,
        position: position as i32,
        behavior: crate::domain::welcome::OptionBehavior::Toggle,
    }
}

pub fn panel(guild_id: u64, slug: &str) -> RolePanel {
    let now = chrono::Utc::now();
    RolePanel {
        id: Uuid::new_v4(),
        guild_id,
        slug: slug.to_string(),
        title: "Pick your roles".to_string(),
        description: None,
        channel_id: Some(100),
        message_id: None,
        is_published: false,
        created_at: now,
        updated_at: now,
    }
}

#[derive(Clone)]
pub struct FakeWelcome {
    store: Store,
}

impl FakeWelcome {
    pub fn new(store: Store) -> Self {
        Self { store }
    }
}

impl WelcomeRepository for FakeWelcome {
    async fn find_config(&self, guild_id: u64) -> Result<WelcomeConfig, RepositoryError> {
        Ok(lock(&self.store)
            .configs
            .iter()
            .find(|config| config.guild_id == guild_id)
            .cloned()
            .unwrap_or_else(|| WelcomeConfig::default_for(guild_id)))
    }

    async fn upsert_config(&self, config: WelcomeConfig) -> Result<WelcomeConfig, RepositoryError> {
        seed_config(&self.store, config.clone());
        Ok(config)
    }

    async fn find_panel(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<Option<RolePanelWithOptions>, RepositoryError> {
        let state = lock(&self.store);
        let Some(panel) = state
            .panels
            .iter()
            .find(|panel| panel.guild_id == guild_id && panel.slug == slug)
            .cloned()
        else {
            return Ok(None);
        };
        let options = state
            .options
            .iter()
            .filter(|option| option.panel_id == panel.id)
            .cloned()
            .collect();
        Ok(Some(RolePanelWithOptions { panel, options }))
    }

    async fn find_panel_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<RolePanelWithOptions>, RepositoryError> {
        let state = lock(&self.store);
        let Some(panel) = state.panels.iter().find(|panel| panel.id == id).cloned() else {
            return Ok(None);
        };
        let options = state
            .options
            .iter()
            .filter(|option| option.panel_id == panel.id)
            .cloned()
            .collect();
        Ok(Some(RolePanelWithOptions { panel, options }))
    }

    async fn list_panels(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<RolePanel>, RepositoryError> {
        let panels: Vec<RolePanel> = lock(&self.store)
            .panels
            .iter()
            .filter(|panel| panel.guild_id == guild_id)
            .cloned()
            .collect();
        let total = panels.len() as u64;
        Ok(Page::new(panels, params, total))
    }

    async fn upsert_panel(&self, panel: RolePanel) -> Result<RolePanel, RepositoryError> {
        let mut state = lock(&self.store);
        state.panels.retain(|stored| stored.id != panel.id);
        state.panels.push(panel.clone());
        Ok(panel)
    }

    async fn mark_published(
        &self,
        id: Uuid,
        channel_id: u64,
        message_id: u64,
    ) -> Result<RolePanel, RepositoryError> {
        let mut state = lock(&self.store);
        let panel = state
            .panels
            .iter_mut()
            .find(|panel| panel.id == id)
            .ok_or_else(|| RepositoryError::not_found("role_panel"))?;
        panel.channel_id = Some(channel_id);
        panel.message_id = Some(message_id);
        panel.is_published = true;
        panel.updated_at = chrono::Utc::now();
        Ok(panel.clone())
    }

    async fn delete_panel(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut state = lock(&self.store);
        state.panels.retain(|panel| panel.id != id);
        state.options.retain(|option| option.panel_id != id);
        Ok(())
    }

    async fn find_option_by_custom_id(
        &self,
        custom_id: &str,
    ) -> Result<Option<RolePanelOption>, RepositoryError> {
        Ok(lock(&self.store)
            .options
            .iter()
            .find(|option| option.custom_id == custom_id)
            .cloned())
    }

    async fn list_options(&self, panel_id: Uuid) -> Result<Vec<RolePanelOption>, RepositoryError> {
        let mut options: Vec<RolePanelOption> = lock(&self.store)
            .options
            .iter()
            .filter(|option| option.panel_id == panel_id)
            .cloned()
            .collect();
        options.sort_by_key(|option| option.position);
        Ok(options)
    }

    async fn create_option(
        &self,
        option: RolePanelOption,
    ) -> Result<RolePanelOption, RepositoryError> {
        let mut state = lock(&self.store);
        if state
            .options
            .iter()
            .any(|stored| stored.custom_id == option.custom_id)
        {
            return Err(RepositoryError::conflict(
                "that button already exists on the panel",
            ));
        }
        state.options.push(option.clone());
        Ok(option)
    }

    async fn delete_option(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut state = lock(&self.store);
        let before = state.options.len();
        state.options.retain(|option| option.id != id);
        if state.options.len() == before {
            return Err(RepositoryError::not_found("role_panel_option"));
        }
        Ok(())
    }

    async fn count_options(&self, panel_id: Uuid) -> Result<usize, RepositoryError> {
        Ok(lock(&self.store)
            .options
            .iter()
            .filter(|option| option.panel_id == panel_id)
            .count())
    }
}
