// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::matrix::{ReplaceRolePermissionsCommand, ReplaceRolePermissionsUseCase};
use super::permissions::ListPermissionsUseCase;
use super::roles::{
    CreateGuildRoleCommand, CreateGuildRoleUseCase, DeleteGuildRoleUseCase, GetGuildRoleUseCase,
    ListGuildRolesUseCase, MemberRoleCounts,
};
use super::test_support::{
    FakeGuildMembers, FakeGuildRoles, FakePermissionCatalog, FakeStore, store,
};
use crate::domain::guild_member::GuildMember;
use crate::domain::rbac::{GuildRole, RbacError, names};

const GUILD: u64 = 1;

fn lock(store: &Arc<Mutex<FakeStore>>) -> std::sync::MutexGuard<'_, FakeStore> {
    store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn role(state: &mut FakeStore, id: Uuid, slug: &str, is_system: bool, permissions: &[&str]) {
    state.roles.push(GuildRole {
        id,
        guild_id: GUILD,
        slug: slug.to_string(),
        name: slug.to_string(),
        description: None,
        is_system,
        position: 0,
    });
    state.role_permissions.insert(
        id,
        permissions.iter().map(|name| name.to_string()).collect(),
    );
}

fn member(state: &mut FakeStore, user_id: Uuid, guild_role_id: Uuid) {
    let now = chrono::Utc::now();
    state.members.push(GuildMember {
        id: Uuid::new_v4(),
        guild_id: GUILD,
        user_id,
        guild_role_id,
        discord_permissions: 0,
        discord_nick: None,
        synced_at: now,
        created_at: now,
        updated_at: now,
    });
}

// --- the last-admin guard ---------------------------------------------------

#[tokio::test]
async fn dropping_the_last_dashboard_manage_grant_is_refused() {
    let admin_role = Uuid::new_v4();
    let viewer_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            admin_role,
            "admin",
            true,
            &[names::DASHBOARD_MANAGE, names::MEMBERS_READ],
        );
        role(
            &mut state,
            viewer_role,
            "viewer",
            true,
            &[names::MEMBERS_READ],
        );
        // The only administrator in the guild holds the role being edited.
        member(&mut state, Uuid::new_v4(), admin_role);
    }

    let use_case = ReplaceRolePermissionsUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let error = use_case
        .execute(ReplaceRolePermissionsCommand {
            guild_id: GUILD,
            role_id: admin_role,
            permissions: vec![names::MEMBERS_READ.to_string()],
        })
        .await
        .expect_err("this is the edit that loses the guild");

    assert!(matches!(
        error,
        RbacError::WouldRemoveLastAdmin(permission) if permission == names::DASHBOARD_MANAGE
    ));

    // The refusal must leave the stored matrix exactly as it was.
    let state = lock(&store);
    let stored = state
        .role_permissions
        .get(&admin_role)
        .cloned()
        .unwrap_or_default();
    assert!(stored.iter().any(|name| name == names::DASHBOARD_MANAGE));
}

#[tokio::test]
async fn getting_a_role_with_counts_reports_members_holding_it() {
    let role_id = Uuid::new_v4();
    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            role_id,
            "moderator",
            false,
            &[names::MEMBERS_READ],
        );
        member(&mut state, Uuid::new_v4(), role_id);
        member(&mut state, Uuid::new_v4(), role_id);
    }

    let use_case = GetGuildRoleUseCase::with_counts(
        FakeGuildRoles::new(store.clone()),
        MemberRoleCounts::new(FakeGuildMembers::new(store)),
    );

    let view = use_case
        .execute(GUILD, role_id)
        .await
        .expect("the role should be readable");

    assert_eq!(view.member_count, 2);
}

#[tokio::test]
async fn the_last_admin_guard_counts_members_not_roles() {
    let admin_role = Uuid::new_v4();
    let second_admin_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            admin_role,
            "admin",
            true,
            &[names::DASHBOARD_MANAGE],
        );
        // Another role grants the capability, but nobody holds it: the guild is
        // still one edit away from having no administrator.
        role(
            &mut state,
            second_admin_role,
            "backup",
            false,
            &[names::DASHBOARD_MANAGE],
        );
        member(&mut state, Uuid::new_v4(), admin_role);
    }

    let use_case = ReplaceRolePermissionsUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let error = use_case
        .execute(ReplaceRolePermissionsCommand {
            guild_id: GUILD,
            role_id: admin_role,
            permissions: Vec::new(),
        })
        .await
        .expect_err("an unheld grant is not an administrator");

    assert!(matches!(error, RbacError::WouldRemoveLastAdmin(_)));
}

#[tokio::test]
async fn a_matrix_edit_is_allowed_when_another_member_keeps_the_capability() {
    let admin_role = Uuid::new_v4();
    let second_admin_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            admin_role,
            "admin",
            true,
            &[names::DASHBOARD_MANAGE],
        );
        role(
            &mut state,
            second_admin_role,
            "ops",
            false,
            &[names::DASHBOARD_MANAGE],
        );
        member(&mut state, Uuid::new_v4(), admin_role);
        // A second person holds the other role, so the guild keeps an operator.
        member(&mut state, Uuid::new_v4(), second_admin_role);
    }

    let use_case = ReplaceRolePermissionsUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let view = use_case
        .execute(ReplaceRolePermissionsCommand {
            guild_id: GUILD,
            role_id: admin_role,
            permissions: vec![names::MEMBERS_READ.to_string()],
        })
        .await
        .expect("somebody else still holds dashboard:manage");

    assert_eq!(view.permissions, vec![names::MEMBERS_READ.to_string()]);
    assert_eq!(view.member_count, 1, "one member held the edited role");
}

#[tokio::test]
async fn granting_manage_is_never_blocked() {
    let viewer_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            viewer_role,
            "viewer",
            true,
            &[names::MEMBERS_READ],
        );
        member(&mut state, Uuid::new_v4(), viewer_role);
    }

    let use_case = ReplaceRolePermissionsUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let view = use_case
        .execute(ReplaceRolePermissionsCommand {
            guild_id: GUILD,
            role_id: viewer_role,
            permissions: vec![names::DASHBOARD_MANAGE.to_string()],
        })
        .await
        .expect("adding a capability cannot strand a guild");

    assert_eq!(view.permissions, vec![names::DASHBOARD_MANAGE.to_string()]);
}

#[tokio::test]
async fn an_unknown_permission_name_is_rejected() {
    let some_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(&mut state, some_role, "viewer", false, &[]);
    }

    let use_case = ReplaceRolePermissionsUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let error = use_case
        .execute(ReplaceRolePermissionsCommand {
            guild_id: GUILD,
            role_id: some_role,
            permissions: vec!["dashboard:manag".to_string()],
        })
        .await
        .expect_err("a typo must not be stored as a permission");

    assert!(matches!(error, RbacError::UnknownPermission(name) if name == "dashboard:manag"));
}

// --- deleting roles ---------------------------------------------------------

#[tokio::test]
async fn a_system_role_cannot_be_deleted() {
    let admin_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            admin_role,
            "admin",
            true,
            &[names::DASHBOARD_MANAGE],
        );
    }

    let use_case = DeleteGuildRoleUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let error = use_case
        .execute(GUILD, admin_role)
        .await
        .expect_err("system roles are seeded, not deletable");

    assert!(matches!(error, RbacError::SystemRoleImmutable(slug) if slug == "admin"));
}

#[tokio::test]
async fn a_role_with_members_cannot_be_deleted() {
    let custom_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            custom_role,
            "events",
            false,
            &[names::MEMBERS_READ],
        );
        member(&mut state, Uuid::new_v4(), custom_role);
    }

    let use_case = DeleteGuildRoleUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    let error = use_case
        .execute(GUILD, custom_role)
        .await
        .expect_err("the foreign key would refuse this anyway");

    assert!(matches!(error, RbacError::RoleInUse(slug) if slug == "events"));

    let state = lock(&store);
    assert!(state.roles.iter().any(|role| role.id == custom_role));
}

#[tokio::test]
async fn an_unused_custom_role_is_deleted() {
    let custom_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(&mut state, custom_role, "events", false, &[]);
    }

    let use_case = DeleteGuildRoleUseCase::new(
        FakeGuildRoles::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
    );

    use_case
        .execute(GUILD, custom_role)
        .await
        .expect("nothing points at it");

    assert!(lock(&store).roles.is_empty());
}

// --- the catalog and role reads --------------------------------------------

#[tokio::test]
async fn the_catalog_lists_every_named_permission() {
    let use_case = ListPermissionsUseCase::new(FakePermissionCatalog::new());

    let permissions = use_case.execute().await.expect("the catalog is a constant");

    assert_eq!(permissions.len(), names::CATALOG.len());
    assert!(
        permissions
            .iter()
            .any(|permission| permission.name == names::DASHBOARD_MANAGE)
    );
}

#[tokio::test]
async fn a_role_from_another_guild_is_not_addressable() {
    let foreign_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        state.roles.push(GuildRole {
            id: foreign_role,
            guild_id: 999,
            slug: "admin".to_string(),
            name: "Admin".to_string(),
            description: None,
            is_system: true,
            position: 0,
        });
    }

    let use_case = GetGuildRoleUseCase::new(FakeGuildRoles::new(store));

    let error = use_case
        .execute(GUILD, foreign_role)
        .await
        .expect_err("a role id from another guild must not resolve here");

    assert!(matches!(error, RbacError::RoleNotFound));
}

#[tokio::test]
async fn listing_roles_returns_each_matrix() {
    let admin_role = Uuid::new_v4();
    let viewer_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = lock(&store);
        role(
            &mut state,
            admin_role,
            "admin",
            true,
            &[names::DASHBOARD_MANAGE],
        );
        role(
            &mut state,
            viewer_role,
            "viewer",
            true,
            &[names::MEMBERS_READ],
        );
        member(&mut state, Uuid::new_v4(), viewer_role);
    }

    let use_case = ListGuildRolesUseCase::with_counts(
        FakeGuildRoles::new(store.clone()),
        super::roles::MemberRoleCounts::new(FakeGuildMembers::new(store)),
    );

    let mut roles = use_case.execute(GUILD).await.expect("two roles exist");
    roles.sort_by(|left, right| left.role.slug.cmp(&right.role.slug));

    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0].role.slug, "admin");
    assert_eq!(
        roles[0].permissions,
        vec![names::DASHBOARD_MANAGE.to_string()]
    );
    assert_eq!(roles[0].member_count, 0);
    assert_eq!(roles[1].member_count, 1);
}

// --- creating roles ---------------------------------------------------------

#[tokio::test]
async fn a_slug_is_normalized_and_a_duplicate_refused() {
    let store = store();
    {
        let mut state = lock(&store);
        role(&mut state, Uuid::new_v4(), "events", false, &[]);
    }

    let use_case = CreateGuildRoleUseCase::new(FakeGuildRoles::new(store.clone()));

    let created = use_case
        .execute(CreateGuildRoleCommand {
            guild_id: GUILD,
            slug: "  Mod-Team  ".to_string(),
            name: "Mod Team".to_string(),
            description: None,
            position: 10,
        })
        .await
        .expect("a new slug is accepted");

    assert_eq!(created.slug, "mod-team");
    assert!(!created.is_system);

    let error = use_case
        .execute(CreateGuildRoleCommand {
            guild_id: GUILD,
            slug: "Events".to_string(),
            name: "Events".to_string(),
            description: None,
            position: 0,
        })
        .await
        .expect_err("`Events` normalizes onto the existing `events`");

    assert!(matches!(error, RbacError::Repository(_)));
}

#[tokio::test]
async fn an_invalid_slug_is_rejected() {
    let store = store();
    let use_case = CreateGuildRoleUseCase::new(FakeGuildRoles::new(store));

    let error = use_case
        .execute(CreateGuildRoleCommand {
            guild_id: GUILD,
            slug: "not a slug!".to_string(),
            name: "Nope".to_string(),
            description: None,
            position: 0,
        })
        .await
        .expect_err("slugs appear in URLs and permission checks");

    assert!(matches!(error, RbacError::UnknownPermission(_)));
}
