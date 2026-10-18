// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

use super::list::ListGuildMembersUseCase;
use super::remove::RemoveGuildMemberUseCase;
use super::update_role::{UpdateGuildMemberRoleCommand, UpdateGuildMemberRoleUseCase};
use crate::application::guild_member::test_support::{
    FakeGuildMembers, FakeGuildRoles, FakeGuilds, store,
};
use crate::domain::errors::PaginationParams;
use crate::domain::guild_member::GuildMemberError;
use crate::domain::rbac::names;

const GUILD: u64 = 1;

/// A guild whose only administrator is `admin_user`, plus a second member on a
/// role that grants nothing.
fn guild_with_one_admin() -> (
    std::sync::Arc<std::sync::Mutex<super::test_support::FakeStore>>,
    Uuid,
    Uuid,
    Uuid,
    Uuid,
) {
    let admin_user = Uuid::new_v4();
    let other_user = Uuid::new_v4();
    let admin_role = Uuid::new_v4();
    let viewer_role = Uuid::new_v4();

    let store = store();
    {
        let mut state = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.roles.push(crate::domain::rbac::GuildRole {
            id: admin_role,
            guild_id: GUILD,
            slug: "admin".to_string(),
            name: "Admin".to_string(),
            description: None,
            is_system: true,
            position: 0,
        });
        state.roles.push(crate::domain::rbac::GuildRole {
            id: viewer_role,
            guild_id: GUILD,
            slug: "viewer".to_string(),
            name: "Viewer".to_string(),
            description: None,
            is_system: true,
            position: 1,
        });
        state
            .role_permissions
            .insert(admin_role, vec![names::DASHBOARD_MANAGE.to_string()]);
        state.role_permissions.insert(viewer_role, Vec::new());
        let now = chrono::Utc::now();
        for (user_id, role_id) in [(admin_user, admin_role), (other_user, viewer_role)] {
            state
                .members
                .push(crate::domain::guild_member::GuildMember {
                    id: Uuid::new_v4(),
                    guild_id: GUILD,
                    user_id,
                    guild_role_id: role_id,
                    discord_permissions: 0,
                    discord_nick: None,
                    synced_at: now,
                    created_at: now,
                    updated_at: now,
                });
        }
    }

    (store, admin_user, other_user, admin_role, viewer_role)
}

#[tokio::test]
async fn moving_the_last_admin_onto_a_role_without_manage_is_refused() {
    let (store, admin_user, other_user, _admin_role, viewer_role) = guild_with_one_admin();

    let use_case = UpdateGuildMemberRoleUseCase::new(
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
        FakeGuilds::new(store.clone()),
    );

    let error = use_case
        .execute(UpdateGuildMemberRoleCommand {
            guild_id: GUILD,
            actor_user_id: other_user,
            target_user_id: admin_user,
            guild_role_id: viewer_role,
        })
        .await
        .expect_err("the guild would be left with nobody able to administer it");

    assert!(matches!(
        error,
        GuildMemberError::WouldRemoveLastHolder(permission) if permission == names::DASHBOARD_MANAGE
    ));

    // The refusal must not have written anything.
    let state = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let untouched = state
        .members
        .iter()
        .find(|member| member.user_id == admin_user)
        .is_some_and(|member| member.guild_role_id != viewer_role);
    assert!(untouched, "a refused change must leave the row alone");
}

#[tokio::test]
async fn a_second_admin_can_be_demoted() {
    let (store, admin_user, other_user, admin_role, viewer_role) = guild_with_one_admin();
    {
        let mut state = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // The second member becomes an administrator too, so the move below is safe.
        state
            .role_permissions
            .insert(admin_role, vec![names::DASHBOARD_MANAGE.to_string()]);
        if let Some(member) = state
            .members
            .iter_mut()
            .find(|member| member.user_id == other_user)
        {
            member.guild_role_id = admin_role;
        }
    }

    let use_case = UpdateGuildMemberRoleUseCase::new(
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
        FakeGuilds::new(store.clone()),
    );

    use_case
        .execute(UpdateGuildMemberRoleCommand {
            guild_id: GUILD,
            actor_user_id: other_user,
            target_user_id: admin_user,
            guild_role_id: viewer_role,
        })
        .await
        .expect("one administrator remains, so the change is allowed");

    let state = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert!(
        state
            .members
            .iter()
            .find(|member| member.user_id == admin_user)
            .is_some_and(|member| member.guild_role_id == viewer_role)
    );
}

#[tokio::test]
async fn nobody_changes_their_own_role() {
    let (store, admin_user, _other_user, _admin_role, viewer_role) = guild_with_one_admin();

    let use_case = UpdateGuildMemberRoleUseCase::new(
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
        FakeGuilds::new(store),
    );

    let error = use_case
        .execute(UpdateGuildMemberRoleCommand {
            guild_id: GUILD,
            actor_user_id: admin_user,
            target_user_id: admin_user,
            guild_role_id: viewer_role,
        })
        .await
        .expect_err("editing your own membership is how an admin locks themselves out");

    assert!(matches!(error, GuildMemberError::CannotChangeSelf));
}

#[tokio::test]
async fn a_role_from_another_guild_is_not_assignable() {
    let (store, _admin, other_user, _admin_role, _viewer_role) = guild_with_one_admin();
    let foreign_role = Uuid::new_v4();
    {
        let mut state = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.roles.push(crate::domain::rbac::GuildRole {
            id: foreign_role,
            guild_id: 999,
            slug: "admin".to_string(),
            name: "Admin".to_string(),
            description: None,
            is_system: true,
            position: 0,
        });
    }

    let use_case = UpdateGuildMemberRoleUseCase::new(
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
        FakeGuilds::new(store.clone()),
    );

    let error = use_case
        .execute(UpdateGuildMemberRoleCommand {
            guild_id: GUILD,
            actor_user_id: Uuid::new_v4(),
            target_user_id: other_user,
            guild_role_id: foreign_role,
        })
        .await
        .expect_err("a role belonging to another guild must not be assignable");

    assert!(matches!(error, GuildMemberError::NotFound));
}

#[tokio::test]
async fn removing_the_last_admin_is_refused() {
    let (store, admin_user, other_user, _admin_role, _viewer_role) = guild_with_one_admin();

    let use_case = RemoveGuildMemberUseCase::new(
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
    );

    let error = use_case
        .execute(GUILD, other_user, admin_user)
        .await
        .expect_err("removing the only administrator would strand the guild");

    assert!(matches!(error, GuildMemberError::WouldRemoveLastHolder(_)));

    let state = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(state.members.len(), 2, "the member must still be there");
}

#[tokio::test]
async fn removing_a_member_without_the_capability_succeeds() {
    let (store, _admin, other_user, _admin_role, _viewer_role) = guild_with_one_admin();

    let use_case = RemoveGuildMemberUseCase::new(
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
    );

    use_case
        .execute(GUILD, Uuid::new_v4(), other_user)
        .await
        .expect("a viewer holds nothing the guild cannot lose");

    let state = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(state.members.len(), 1);
}

#[tokio::test]
async fn listing_members_returns_the_joined_role() {
    let (store, admin_user, _other_user, _admin_role, _viewer_role) = guild_with_one_admin();

    let use_case = ListGuildMembersUseCase::new(FakeGuildMembers::new(store));

    let page = use_case
        .execute(GUILD, &PaginationParams::default())
        .await
        .expect("the fake store always answers");

    assert_eq!(page.total, 2);
    let admin = page
        .data
        .iter()
        .find(|row| row.member.user_id == admin_user)
        .expect("the administrator is in the page");
    assert_eq!(admin.role.slug, "admin");
}
