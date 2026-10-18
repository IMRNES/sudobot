use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zod_rs::prelude::{Schema, ZodSchema};

use crate::domain::rbac::{GuildRole, Permission};

/// One entry in the permission catalog the matrix editor renders.
#[derive(Debug, Serialize)]
pub struct PermissionResponse {
    pub id: Uuid,
    pub name: String,
    pub group: String,
    pub description: String,
}

impl From<Permission> for PermissionResponse {
    fn from(permission: Permission) -> Self {
        Self {
            id: permission.id,
            name: permission.name,
            group: permission.group,
            description: permission.description,
        }
    }
}

/// A guild role and the permissions it grants.
#[derive(Debug, Serialize)]
pub struct RoleResponse {
    pub id: Uuid,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub is_system: bool,
    pub position: i32,
    pub member_count: u64,
    /// Permission names, not ids: the matrix is edited by name and a client should
    /// not have to join two lists to render one checkbox.
    pub permissions: Vec<String>,
}

impl RoleResponse {
    pub fn new(role: GuildRole, permissions: Vec<String>, member_count: u64) -> Self {
        Self {
            id: role.id,
            slug: role.slug,
            name: role.name,
            description: role.description,
            is_system: role.is_system,
            position: role.position,
            member_count,
            permissions,
        }
    }
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct CreateRoleRequest {
    #[zod(min_length(1), max_length(32))]
    pub slug: String,
    #[zod(min_length(1), max_length(64))]
    pub name: String,
    #[zod(max_length(500))]
    pub description: Option<String>,
    pub position: Option<i32>,
}

#[derive(Debug, Deserialize, ZodSchema)]
pub struct UpdateRoleRequest {
    #[zod(min_length(1), max_length(64))]
    pub name: Option<String>,
    #[zod(max_length(500))]
    pub description: Option<String>,
    pub position: Option<i32>,
}

/// Replaces a role's whole permission matrix.
///
/// The dashboard submits the complete set rather than a diff: the matrix is edited
/// as a unit, and diffing client state against server state invites a lost update
/// when two admins edit at once.
#[derive(Debug, Deserialize, ZodSchema)]
pub struct ReplacePermissionsRequest {
    #[zod(max_length(64))]
    pub permissions: Vec<String>,
}

crate::validated_request!(CreateRoleRequest);

crate::validated_request!(UpdateRoleRequest);

crate::validated_request!(ReplacePermissionsRequest);
