// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use uuid::Uuid;

use super::admin::SetSuperAdminUseCase;
use super::errors::UserAdminError;
use super::me::GetMeUseCase;
use super::test_support::{FakeUsers, user};
use crate::application::guild_member::test_support::{
    FakeGuildMembers, FakeGuildRoles, FakeGuilds, store,
};
use crate::domain::guild::Guild;
use crate::domain::guild_member::GuildMember;
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
async fn me_reports_each_guild_with_its_role_and_permissions() {
    let account = user(false);
    let user_id = account.id;
    let role_id = Uuid::new_v4();
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
        state.roles.push(GuildRole {
            id: role_id,
            guild_id: 7,
            slug: "moderator".to_string(),
            name: "Moderator".to_string(),
            description: None,
            is_system: true,
            position: 1,
        });
        state
            .role_permissions
            .insert(role_id, vec![names::MEMBERS_READ.to_string()]);
        state.members.push(GuildMember {
            id: Uuid::new_v4(),
            guild_id: 7,
            user_id,
            guild_role_id: role_id,
            discord_permissions: 0,
            discord_nick: None,
            synced_at: now,
            created_at: now,
            updated_at: now,
        });
    }

    let use_case = GetMeUseCase::new(
        FakeUsers::new(vec![account.clone()]),
        FakeGuilds::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store),
    );

    let view = use_case.execute(user_id).await.expect("the account exists");

    assert_eq!(view.user.id, account.id);
    assert_eq!(view.guilds.len(), 1);
    let guild = &view.guilds[0];
    assert_eq!(guild.id, 7);
    assert_eq!(guild.role_slug, "moderator");
    assert_eq!(guild.role_name, "Moderator");
    assert_eq!(guild.permissions, vec![names::MEMBERS_READ.to_string()]);
    assert!(guild.bot_present);
}

#[tokio::test]
async fn me_for_an_unknown_account_is_not_found() {
    let store = store();
    let use_case = GetMeUseCase::new(
        FakeUsers::new(Vec::new()),
        FakeGuilds::new(store.clone()),
        FakeGuildMembers::new(store.clone()),
        FakeGuildRoles::new(store),
    );

    let error = use_case
        .execute(Uuid::new_v4())
        .await
        .expect_err("a deleted account has no /me");

    assert!(matches!(error, crate::domain::user::UserError::NotFound));
}
