// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Hand-written fakes for the guild-member use cases.
//!
//! In-memory rather than mocked: these tests are about rules (who may be changed,
//! what the guild must keep), and a fake that stores rows lets a test set up a
//! guild's whole membership and assert on the state afterwards. No database, no
//! mock framework, and the fakes live behind `cfg(test)` so they never reach a
//! release build.

use std::collections::HashMap;
use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::guild::{Guild, GuildPatch, GuildRepository, NewGuild};
use crate::domain::guild_member::{
    GuildMember, GuildMemberRepository, GuildMemberWithRole, NewGuildMember,
};
use crate::domain::rbac::{
    GuildRole, GuildRoleRepository, Permission, PermissionRepository, PermissionSet,
};

/// One guild's worth of state, in memory.
pub struct FakeStore {
    pub members: Vec<GuildMember>,
    pub roles: Vec<GuildRole>,
    pub role_permissions: HashMap<Uuid, Vec<String>>,
    pub guild: Option<Guild>,
}

impl Default for FakeStore {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeStore {
    pub fn new() -> Self {
        Self {
            members: Vec::new(),
            roles: Vec::new(),
            role_permissions: HashMap::new(),
            guild: None,
        }
    }

    pub fn with_role(mut self, id: Uuid, slug: &str, permissions: &[&str]) -> Self {
        self.roles.push(GuildRole {
            id,
            guild_id: 1,
            slug: slug.to_string(),
            name: slug.to_string(),
            description: None,
            is_system: false,
            position: 0,
        });
        self.role_permissions.insert(
            id,
            permissions.iter().map(|name| name.to_string()).collect(),
        );
        self
    }

    pub fn with_member(mut self, id: Uuid, user_id: Uuid, guild_role_id: Uuid) -> Self {
        self.members.push(GuildMember {
            id,
            guild_id: 1,
            user_id,
            guild_role_id,
            discord_permissions: 0,
            discord_nick: None,
            synced_at: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        });
        self
    }
}

pub fn store() -> std::sync::Arc<Mutex<FakeStore>> {
    std::sync::Arc::new(Mutex::new(FakeStore::new()))
}

/// A lock poisoned by a panicking test is re-taken rather than propagated: the
/// assertion that failed is the one worth reporting.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone)]
pub struct FakeGuildMembers {
    store: std::sync::Arc<Mutex<FakeStore>>,
}

impl FakeGuildMembers {
    pub fn new(store: std::sync::Arc<Mutex<FakeStore>>) -> Self {
        Self { store }
    }
}

#[derive(Clone)]
pub struct FakeGuildRoles {
    store: std::sync::Arc<Mutex<FakeStore>>,
}

impl FakeGuildRoles {
    pub fn new(store: std::sync::Arc<Mutex<FakeStore>>) -> Self {
        Self { store }
    }
}

#[derive(Clone)]
pub struct FakeGuilds {
    store: std::sync::Arc<Mutex<FakeStore>>,
}

impl FakeGuilds {
    pub fn new(store: std::sync::Arc<Mutex<FakeStore>>) -> Self {
        Self { store }
    }
}

impl GuildMemberRepository for FakeGuildMembers {
    async fn find(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> Result<Option<GuildMember>, RepositoryError> {
        Ok(lock(&self.store)
            .members
            .iter()
            .find(|member| member.guild_id == guild_id && member.user_id == user_id)
            .cloned())
    }

    async fn find_with_role(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> Result<Option<GuildMemberWithRole>, RepositoryError> {
        let store = lock(&self.store);
        let Some(member) = store
            .members
            .iter()
            .find(|member| member.guild_id == guild_id && member.user_id == user_id)
        else {
            return Ok(None);
        };
        let Some(role) = store
            .roles
            .iter()
            .find(|role| role.id == member.guild_role_id)
        else {
            return Err(RepositoryError::not_found("guild_role"));
        };
        Ok(Some(GuildMemberWithRole {
            member: member.clone(),
            role: role.clone(),
        }))
    }

    async fn list_for_guild(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<GuildMemberWithRole>, RepositoryError> {
        let store = lock(&self.store);
        let mut data = Vec::new();
        for member in store
            .members
            .iter()
            .filter(|member| member.guild_id == guild_id)
        {
            let role = store
                .roles
                .iter()
                .find(|role| role.id == member.guild_role_id)
                .cloned()
                .ok_or_else(|| RepositoryError::not_found("guild_role"))?;
            data.push(GuildMemberWithRole {
                member: member.clone(),
                role,
            });
        }
        let total = data.len() as u64;
        Ok(Page::new(data, params, total))
    }

    async fn list_for_user(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<GuildMemberWithRole>, RepositoryError> {
        let store = lock(&self.store);
        let mut data = Vec::new();
        for member in store
            .members
            .iter()
            .filter(|member| member.user_id == user_id)
        {
            let role = store
                .roles
                .iter()
                .find(|role| role.id == member.guild_role_id)
                .cloned()
                .ok_or_else(|| RepositoryError::not_found("guild_role"))?;
            data.push(GuildMemberWithRole {
                member: member.clone(),
                role,
            });
        }
        Ok(data)
    }

    async fn create(&self, member: NewGuildMember) -> Result<GuildMember, RepositoryError> {
        let now = chrono::Utc::now();
        let created = GuildMember {
            id: member.id,
            guild_id: member.guild_id,
            user_id: member.user_id,
            guild_role_id: member.guild_role_id,
            discord_permissions: member.discord_permissions,
            discord_nick: member.discord_nick,
            synced_at: now,
            created_at: now,
            updated_at: now,
        };
        lock(&self.store).members.push(created.clone());
        Ok(created)
    }

    async fn upsert_from_discord(
        &self,
        member: NewGuildMember,
    ) -> Result<GuildMember, RepositoryError> {
        self.create(member).await
    }

    async fn update_role(
        &self,
        guild_id: u64,
        user_id: Uuid,
        guild_role_id: Uuid,
    ) -> Result<GuildMember, RepositoryError> {
        let mut store = lock(&self.store);
        let member = store
            .members
            .iter_mut()
            .find(|member| member.guild_id == guild_id && member.user_id == user_id)
            .ok_or_else(|| RepositoryError::not_found("guild_member"))?;
        member.guild_role_id = guild_role_id;
        member.updated_at = chrono::Utc::now();
        Ok(member.clone())
    }

    async fn delete(&self, guild_id: u64, user_id: Uuid) -> Result<(), RepositoryError> {
        let mut store = lock(&self.store);
        let before = store.members.len();
        store
            .members
            .retain(|member| !(member.guild_id == guild_id && member.user_id == user_id));
        if store.members.len() == before {
            return Err(RepositoryError::not_found("guild_member"));
        }
        Ok(())
    }

    async fn count_holders_of(
        &self,
        guild_id: u64,
        permission: &str,
    ) -> Result<u64, RepositoryError> {
        let store = lock(&self.store);
        let holders = store
            .members
            .iter()
            .filter(|member| {
                member.guild_id == guild_id
                    && store
                        .role_permissions
                        .get(&member.guild_role_id)
                        .is_some_and(|names| names.iter().any(|name| name == permission))
            })
            .count();
        Ok(holders as u64)
    }
}

impl GuildRoleRepository for FakeGuildRoles {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<GuildRole>, RepositoryError> {
        Ok(lock(&self.store)
            .roles
            .iter()
            .find(|role| role.id == id)
            .cloned())
    }

    async fn find_by_slug(
        &self,
        guild_id: u64,
        slug: &str,
    ) -> Result<Option<GuildRole>, RepositoryError> {
        Ok(lock(&self.store)
            .roles
            .iter()
            .find(|role| role.guild_id == guild_id && role.slug == slug)
            .cloned())
    }

    async fn list_for_guild(&self, guild_id: u64) -> Result<Vec<GuildRole>, RepositoryError> {
        Ok(lock(&self.store)
            .roles
            .iter()
            .filter(|role| role.guild_id == guild_id)
            .cloned()
            .collect())
    }

    async fn list(
        &self,
        guild_id: u64,
        params: &PaginationParams,
    ) -> Result<Page<GuildRole>, RepositoryError> {
        let roles = self.list_for_guild(guild_id).await?;
        let total = roles.len() as u64;
        Ok(Page::new(roles, params, total))
    }

    async fn create(
        &self,
        guild_id: u64,
        slug: &str,
        name: &str,
        description: Option<&str>,
        is_system: bool,
        position: i32,
    ) -> Result<GuildRole, RepositoryError> {
        let mut store = lock(&self.store);
        if store
            .roles
            .iter()
            .any(|role| role.guild_id == guild_id && role.slug == slug)
        {
            return Err(RepositoryError::conflict(format!(
                "role `{slug}` already exists"
            )));
        }
        let role = GuildRole {
            id: Uuid::new_v4(),
            guild_id,
            slug: slug.to_string(),
            name: name.to_string(),
            description: description.map(str::to_string),
            is_system,
            position,
        };
        store.roles.push(role.clone());
        store.role_permissions.insert(role.id, Vec::new());
        Ok(role)
    }

    async fn update(
        &self,
        id: Uuid,
        name: Option<&str>,
        description: Option<&str>,
        position: Option<i32>,
    ) -> Result<GuildRole, RepositoryError> {
        let mut store = lock(&self.store);
        let role = store
            .roles
            .iter_mut()
            .find(|role| role.id == id)
            .ok_or_else(|| RepositoryError::not_found("guild_role"))?;
        if let Some(name) = name {
            role.name = name.to_string();
        }
        if let Some(description) = description {
            role.description = Some(description.to_string());
        }
        if let Some(position) = position {
            role.position = position;
        }
        Ok(role.clone())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut store = lock(&self.store);
        let before = store.roles.len();
        store.roles.retain(|role| role.id != id);
        if store.roles.len() == before {
            return Err(RepositoryError::not_found("guild_role"));
        }
        store.role_permissions.remove(&id);
        Ok(())
    }

    async fn permissions_for_role(&self, role_id: Uuid) -> Result<Vec<String>, RepositoryError> {
        Ok(lock(&self.store)
            .role_permissions
            .get(&role_id)
            .cloned()
            .unwrap_or_default())
    }

    async fn replace_role_permissions(
        &self,
        role_id: Uuid,
        permission_names: &[String],
    ) -> Result<(), RepositoryError> {
        lock(&self.store)
            .role_permissions
            .insert(role_id, permission_names.to_vec());
        Ok(())
    }

    async fn permissions_for_member(
        &self,
        guild_id: u64,
        user_id: Uuid,
    ) -> Result<PermissionSet, RepositoryError> {
        let store = lock(&self.store);
        let names = store
            .members
            .iter()
            .find(|member| member.guild_id == guild_id && member.user_id == user_id)
            .and_then(|member| store.role_permissions.get(&member.guild_role_id))
            .cloned()
            .unwrap_or_default();
        Ok(PermissionSet::new(guild_id, names))
    }
}

impl GuildRepository for FakeGuilds {
    async fn find_by_id(&self, id: u64) -> Result<Option<Guild>, RepositoryError> {
        Ok(lock(&self.store)
            .guild
            .clone()
            .filter(|guild| guild.id == id))
    }

    async fn list_active(&self) -> Result<Vec<Guild>, RepositoryError> {
        Ok(lock(&self.store).guild.clone().into_iter().collect())
    }

    async fn list(&self, params: &PaginationParams) -> Result<Page<Guild>, RepositoryError> {
        let guilds = self.list_active().await?;
        let total = guilds.len() as u64;
        Ok(Page::new(guilds, params, total))
    }

    async fn upsert(&self, guild: NewGuild) -> Result<Guild, RepositoryError> {
        let now = chrono::Utc::now();
        let stored = Guild {
            id: guild.id,
            name: guild.name,
            icon_url: guild.icon_url,
            owner_discord_id: guild.owner_discord_id,
            member_count: guild.member_count,
            bot_present: true,
            created_at: now,
            updated_at: now,
        };
        lock(&self.store).guild = Some(stored.clone());
        Ok(stored)
    }

    async fn update(&self, id: u64, patch: GuildPatch) -> Result<Guild, RepositoryError> {
        let mut store = lock(&self.store);
        let guild = store
            .guild
            .as_mut()
            .filter(|guild| guild.id == id)
            .ok_or_else(|| RepositoryError::not_found("guild"))?;
        if let Some(name) = patch.name {
            guild.name = name;
        }
        Ok(guild.clone())
    }

    async fn delete(&self, id: u64) -> Result<(), RepositoryError> {
        let mut store = lock(&self.store);
        if store.guild.as_ref().is_some_and(|guild| guild.id == id) {
            store.guild = None;
            return Ok(());
        }
        Err(RepositoryError::not_found("guild"))
    }

    async fn count(&self) -> Result<u64, RepositoryError> {
        Ok(u64::from(lock(&self.store).guild.is_some()))
    }
}

/// A catalog fake, for the tests that need `PermissionRepository` in scope.
#[derive(Clone, Default)]
pub struct FakePermissions {
    permissions: Vec<Permission>,
}

impl FakePermissions {
    pub fn new() -> Self {
        Self {
            permissions: crate::domain::rbac::names::CATALOG
                .iter()
                .map(|(name, group, description)| Permission {
                    id: Uuid::new_v4(),
                    name: name.to_string(),
                    group: group.to_string(),
                    description: description.to_string(),
                })
                .collect(),
        }
    }
}

impl PermissionRepository for FakePermissions {
    async fn find_by_name(&self, name: &str) -> Result<Option<Permission>, RepositoryError> {
        Ok(self
            .permissions
            .iter()
            .find(|permission| permission.name == name)
            .cloned())
    }

    async fn list(&self) -> Result<Vec<Permission>, RepositoryError> {
        Ok(self.permissions.clone())
    }

    async fn upsert(
        &self,
        name: &str,
        group: &str,
        description: &str,
    ) -> Result<Permission, RepositoryError> {
        Ok(Permission {
            id: Uuid::new_v4(),
            name: name.to_string(),
            group: group.to_string(),
            description: description.to_string(),
        })
    }
}
