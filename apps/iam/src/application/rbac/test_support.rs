// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Hand-written fakes for the RBAC use cases.
//!
//! The role and membership tables already exist in
//! [`crate::application::guild_member::test_support`], and the last-admin guard
//! needs exactly those two repositories, so they are re-exported here rather than
//! copied — a second member fake would be a second thing to keep in step with the
//! trait. What this module adds is the permission catalog.

use uuid::Uuid;

use crate::domain::errors::RepositoryError;
use crate::domain::rbac::{Permission, PermissionRepository};

pub use crate::application::guild_member::test_support::{
    FakeGuildMembers, FakeGuildRoles, FakeStore, store,
};

/// The global permission catalog, served from the domain's own constant.
#[derive(Clone, Default)]
pub struct FakePermissionCatalog;

impl FakePermissionCatalog {
    pub fn new() -> Self {
        Self
    }
}

impl PermissionRepository for FakePermissionCatalog {
    async fn find_by_name(&self, name: &str) -> Result<Option<Permission>, RepositoryError> {
        Ok(crate::domain::rbac::names::CATALOG
            .iter()
            .find(|(candidate, _, _)| *candidate == name)
            .map(|(name, group, description)| Permission {
                id: Uuid::new_v4(),
                name: (*name).to_string(),
                group: (*group).to_string(),
                description: (*description).to_string(),
            }))
    }

    async fn list(&self) -> Result<Vec<Permission>, RepositoryError> {
        Ok(crate::domain::rbac::names::CATALOG
            .iter()
            .map(|(name, group, description)| Permission {
                id: Uuid::new_v4(),
                name: (*name).to_string(),
                group: (*group).to_string(),
                description: (*description).to_string(),
            })
            .collect())
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
