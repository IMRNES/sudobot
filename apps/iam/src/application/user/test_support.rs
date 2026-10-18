// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Hand-written fakes for the account use cases.
//!
//! An in-memory table rather than a mock: the rule under test (the last super
//! admin) is about what the *other* rows look like, so a fake that stores them is
//! the only one that can express it. Behind `cfg(test)`, no database involved.

use std::sync::Mutex;

use uuid::Uuid;

use crate::domain::errors::{Page, PaginationParams, RepositoryError};
use crate::domain::user::{NewUser, User, UserPatch, UserRepository};

/// Accounts, in memory.
pub struct FakeUsers {
    users: Mutex<Vec<User>>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A user row with sane defaults; tests override what they care about.
pub fn user(is_super_admin: bool) -> User {
    let now = chrono::Utc::now();
    User {
        id: Uuid::new_v4(),
        discord_id: 1,
        username: "tester".to_string(),
        global_name: None,
        avatar_url: None,
        email: None,
        is_super_admin,
        created_at: now,
        updated_at: now,
    }
}

impl FakeUsers {
    pub fn new(users: Vec<User>) -> Self {
        Self {
            users: Mutex::new(users),
        }
    }
}

impl UserRepository for FakeUsers {
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepositoryError> {
        Ok(lock(&self.users).iter().find(|user| user.id == id).cloned())
    }

    async fn find_by_discord_id(&self, discord_id: u64) -> Result<Option<User>, RepositoryError> {
        Ok(lock(&self.users)
            .iter()
            .find(|user| user.discord_id == discord_id)
            .cloned())
    }

    async fn create(&self, user: NewUser) -> Result<User, RepositoryError> {
        let now = chrono::Utc::now();
        let created = User {
            id: user.id,
            discord_id: user.discord_id,
            username: user.username,
            global_name: user.global_name,
            avatar_url: user.avatar_url,
            email: user.email,
            is_super_admin: false,
            created_at: now,
            updated_at: now,
        };
        lock(&self.users).push(created.clone());
        Ok(created)
    }

    async fn update(&self, id: Uuid, patch: UserPatch) -> Result<User, RepositoryError> {
        let mut users = lock(&self.users);
        let user = users
            .iter_mut()
            .find(|user| user.id == id)
            .ok_or_else(|| RepositoryError::not_found("user"))?;
        if let Some(username) = patch.username {
            user.username = username;
        }
        user.updated_at = chrono::Utc::now();
        Ok(user.clone())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepositoryError> {
        let mut users = lock(&self.users);
        let before = users.len();
        users.retain(|user| user.id != id);
        if users.len() == before {
            return Err(RepositoryError::not_found("user"));
        }
        Ok(())
    }

    async fn list(&self, params: &PaginationParams) -> Result<Page<User>, RepositoryError> {
        let users = lock(&self.users).clone();
        let total = users.len() as u64;
        let offset = params.offset() as usize;
        let data = users
            .into_iter()
            .skip(offset)
            .take(params.per_page as usize)
            .collect();
        Ok(Page::new(data, params, total))
    }

    async fn set_super_admin(
        &self,
        id: Uuid,
        is_super_admin: bool,
    ) -> Result<User, RepositoryError> {
        let mut users = lock(&self.users);
        let user = users
            .iter_mut()
            .find(|user| user.id == id)
            .ok_or_else(|| RepositoryError::not_found("user"))?;
        user.is_super_admin = is_super_admin;
        user.updated_at = chrono::Utc::now();
        Ok(user.clone())
    }
}
