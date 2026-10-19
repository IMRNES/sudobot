// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

use super::admin::SetSuperAdminUseCase;
use super::errors::UserAdminError;
use super::test_support::{FakeUsers, user};
use crate::application::auth::DashboardMembershipUseCase;
use crate::application::guild_member::test_support::{
    FakeGuildMembers, FakeGuildRoles, FakeGuilds, store,
};
use crate::application::ports::SystemClock;
use crate::domain::guild::Guild;
use crate::domain::rbac::{GuildRole, names};

/// The companion to the matrix guard: a guild cannot lose its last
/// `dashboard:manage` holder, and the platform cannot lose its last operator.
#[tokio::test]
async fn the_last_super_admin_cannot_be_demoted() {
    let only = user(true);
    let id = only.id;
    let use_case = SetSuperAdminUseCase::new(FakeUsers::new(vec![only, user(false)]));

    let error = use_case
        .execute(id, false)
        .await
        .expect_err("demoting the only operator would leave the platform unadministrable");

    assert!(matches!(error, UserAdminError::LastSuperAdmin));
}

#[tokio::test]
async fn a_super_admin_can_be_demoted_when_another_remains() {
    let first = user(true);
    let id = first.id;
    let use_case = SetSuperAdminUseCase::new(FakeUsers::new(vec![first, user(true)]));

    let updated = use_case
        .execute(id, false)
        .await
        .expect("another operator is left, so the demotion is allowed");

    assert!(!updated.is_super_admin);
}

#[tokio::test]
async fn promoting_is_never_blocked_by_the_last_admin_rule() {
    let member = user(false);
    let id = member.id;
    let use_case = SetSuperAdminUseCase::new(FakeUsers::new(vec![member]));

    let updated = use_case
        .execute(id, true)
        .await
        .expect("promotion cannot empty the operator set");

    assert!(updated.is_super_admin);
}

#[tokio::test]
async fn the_dashboard_use_case_records_a_membership_against_the_admin_role() {
    let account = user(false);
    let user_id = account.id;
    let store = store();
    {
        let now = chrono::Utc::now();
        let mut state = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.guild = Some(Guild {
            id: 7,
            name: "Sudobot Test".to_string(),
            icon_url: None,
            owner_discord_id: None,
            member_count: 12,
            bot_present: true,
            created_at: now,
            updated_at: now,
        });
        // The admin role, as `bootstrap` would have created it. The use case must
        // find and reuse it rather than creating a second one.
        state.roles.push(GuildRole {
            id: Uuid::new_v4(),
            guild_id: 7,
            slug: "admin".to_string(),
            name: "Admin".to_string(),
            description: None,
            is_system: true,
            position: 0,
        });
    }

    let use_case = DashboardMembershipUseCase::new(
        FakeGuilds::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
        SystemClock,
    );

    let view = use_case
        .execute(user_id, 7)
        .await
        .expect("the guild exists and the admin role is present");

    assert_eq!(view.id, 7);
    assert_eq!(view.role_slug, "admin");

    let state = store
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(state.members.len(), 1, "one membership row was written");
    assert_eq!(state.members[0].user_id, user_id);
    assert_eq!(state.members[0].guild_id, 7);
    // The whitelist role is the authority, so the Discord bitfield stays zero --
    // claiming an administrator bit here would assert something Discord never said.
    assert_eq!(state.members[0].discord_permissions, 0);
}

#[tokio::test]
async fn a_guild_the_bot_is_not_in_is_reported_as_a_configuration_problem() {
    // `store()` starts with no guild: the deployment names a guild it was never
    // invited to. That is a 503 and a named variable, not a permissions denial.
    let store = store();
    let use_case = DashboardMembershipUseCase::new(
        FakeGuilds::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store),
        SystemClock,
    );

    let error = use_case
        .execute(Uuid::new_v4(), 404)
        .await
        .expect_err("an unknown guild cannot be administered");

    assert!(matches!(
        error,
        crate::domain::auth::AuthError::DashboardGuildUnavailable(404)
    ));
}

#[tokio::test]
async fn the_admin_role_is_created_when_bootstrap_never_ran() {
    // A guild that predates the seeding must not lock its staff out: the role is
    // created with its default matrix instead of failing.
    let store = store();
    {
        let now = chrono::Utc::now();
        let mut state = store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.guild = Some(Guild {
            id: 7,
            name: "Sudobot Test".to_string(),
            icon_url: None,
            owner_discord_id: None,
            member_count: 1,
            bot_present: true,
            created_at: now,
            updated_at: now,
        });
    }

    let use_case = DashboardMembershipUseCase::new(
        FakeGuilds::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store.clone()),
        SystemClock,
    );

    let view = use_case
        .execute(Uuid::new_v4(), 7)
        .await
        .expect("a guild without system roles still admits staff");

    assert_eq!(view.role_slug, "admin");
    assert!(
        view.permissions
            .contains(&names::DASHBOARD_MANAGE.to_string()),
        "the created role carries the admin matrix, not an empty one"
    );
}
