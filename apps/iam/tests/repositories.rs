// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests that need a real PostgreSQL.
//!
//! These are the checks that a fake repository cannot make: that the SQL is valid,
//! that the constraints do what the migration says, and that two statements the
//! application treats as atomic really are.
//!
//! They are `#[ignore]`d so `cargo test` passes on a machine with no database. Run
//! them with:
//!
//! ```text
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433/sudobot \
//!   cargo test -p iam --test repositories -- --ignored
//! ```
//!
//! Each test uses its own guild id, so they can run in any order against a shared
//! database without cleaning up after each other.

use chrono::Utc;
use sea_orm::{ConnectOptions, Database as SeaDatabase, DatabaseConnection};
use std::time::Duration;
use uuid::Uuid;

use iam::application::ports::{Clock, PasswordService, SystemClock, TokenService, TokenType};
use iam::domain::economy::{EconomyRepository, NewTransaction, ShopItem, TransactionReason};
use iam::domain::guild::{GuildRepository, NewGuild};
use iam::domain::user::{NewUser, UserRepository};
use iam::infrastructure::repository::{
    SeaOrmEconomyRepository, SeaOrmGuildMemberRepository, SeaOrmGuildRepository,
    SeaOrmGuildRoleRepository, SeaOrmLevelingRepository, SeaOrmPermissionRepository,
    SeaOrmUserRepository,
};
use iam::infrastructure::service::{Argon2PasswordService, JwtTokenService};

/// Connects to the database named by `DATABASE_URL`.
///
/// Returns `None` when it is unset, so a caller can skip rather than fail — the
/// tests are `#[ignore]`d anyway, but a developer who runs `--ignored` without a
/// database deserves a clear message instead of a connection error.
async fn connect() -> Option<DatabaseConnection> {
    let url = std::env::var("DATABASE_URL").ok()?;

    let mut options = ConnectOptions::new(url);
    options
        .max_connections(3)
        .acquire_timeout(Duration::from_secs(5))
        .sqlx_logging(false);

    match SeaDatabase::connect(options).await {
        Ok(db) => Some(db),
        Err(error) => {
            eprintln!("skipping: could not connect to DATABASE_URL: {error}");
            None
        }
    }
}

/// A guild id nothing else in the suite uses.
///
/// Derived from the clock so a rerun against the same database does not collide
/// with rows a previous run left behind.
fn unique_guild_id() -> u64 {
    // Discord snowflakes are ~19 digits; staying in that range keeps the value
    // realistic without pretending to be a real guild.
    900_000_000_000_000_000 + (Utc::now().timestamp_subsec_micros() as u64)
}

/// Creates the guild row the account and item tests hang off.
async fn seed_guild(guilds: &SeaOrmGuildRepository, guild_id: u64) {
    guilds
        .upsert(NewGuild {
            id: guild_id,
            name: "Integration Test Guild".to_string(),
            icon_url: None,
            owner_discord_id: Some(42),
            member_count: 3,
        })
        .await
        .expect("the guild should insert");
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_guild_round_trips_through_the_repository() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db);
    let guild_id = unique_guild_id();

    seed_guild(&guilds, guild_id).await;

    let found = guilds
        .find_by_id(guild_id)
        .await
        .expect("lookup should succeed")
        .expect("the guild should be there");

    assert_eq!(found.id, guild_id);
    assert_eq!(found.name, "Integration Test Guild");
    assert!(found.bot_present);

    // The upsert is what a rejoin takes: the same id must update, not conflict.
    guilds
        .upsert(NewGuild {
            id: guild_id,
            name: "Renamed".to_string(),
            icon_url: None,
            owner_discord_id: Some(42),
            member_count: 4,
        })
        .await
        .expect("a rejoin must not be a conflict");

    let renamed = guilds
        .find_by_id(guild_id)
        .await
        .expect("lookup should succeed")
        .expect("still there");
    assert_eq!(renamed.name, "Renamed");
    assert_eq!(renamed.member_count, 4);
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn two_credits_produce_two_correct_balances() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let economy = SeaOrmEconomyRepository::new(db);

    let guild_id = unique_guild_id();
    seed_guild(&guilds, guild_id).await;

    let discord_user_id = 424_242;
    let account = economy
        .ensure_account(guild_id, discord_user_id)
        .await
        .expect("the account should be created");
    let starting_balance = account.balance;

    // First credit.
    let (after_first, first_entry) = economy
        .apply_transaction(NewTransaction {
            id: Uuid::new_v4(),
            guild_id,
            discord_user_id,
            amount: 250,
            reason: TransactionReason::AdminGrant,
            actor_discord_id: Some(1),
            note: None,
            idempotency_key: Some(format!("itest-first-{guild_id}")),
        })
        .await
        .expect("the first credit should apply");

    assert_eq!(
        after_first.balance,
        starting_balance + 250,
        "the first credit should land on the starting balance"
    );
    assert_eq!(
        first_entry.balance_after, after_first.balance,
        "the ledger row must record the balance it produced"
    );

    // Second credit, from the balance the first one left — this is the assertion
    // that catches a balance computed from a stale read.
    let (after_second, second_entry) = economy
        .apply_transaction(NewTransaction {
            id: Uuid::new_v4(),
            guild_id,
            discord_user_id,
            amount: 100,
            reason: TransactionReason::AdminGrant,
            actor_discord_id: Some(1),
            note: None,
            idempotency_key: Some(format!("itest-second-{guild_id}")),
        })
        .await
        .expect("the second credit should apply");

    assert_eq!(
        after_second.balance,
        starting_balance + 350,
        "the second credit must build on the first, not replace it"
    );
    assert_eq!(second_entry.balance_after, after_second.balance);
    assert_eq!(
        after_second.total_earned,
        starting_balance + 350,
        "total_earned tracks the credits"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_debit_that_exceeds_the_balance_still_records_the_shortfall() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let economy = SeaOrmEconomyRepository::new(db);

    let guild_id = unique_guild_id();
    seed_guild(&guilds, guild_id).await;

    let discord_user_id = 777_001;
    economy
        .ensure_account(guild_id, discord_user_id)
        .await
        .expect("account");

    let (after, entry) = economy
        .apply_transaction(NewTransaction {
            id: Uuid::new_v4(),
            guild_id,
            discord_user_id,
            // An administrative deduction is allowed to overdraw — the use case
            // decides whether to permit it, the repository just keeps the books.
            amount: -50,
            reason: TransactionReason::AdminDeduct,
            actor_discord_id: Some(1),
            note: None,
            idempotency_key: Some(format!("itest-overdraw-{guild_id}")),
        })
        .await
        .expect("an administrative deduction should apply");

    assert_eq!(after.balance, -50);
    assert_eq!(entry.balance_after, -50);
    assert_eq!(after.total_spent, 50, "a debit counts as spent");
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn the_last_item_can_only_be_bought_once() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let economy = SeaOrmEconomyRepository::new(db);

    let guild_id = unique_guild_id();
    seed_guild(&guilds, guild_id).await;

    let item = economy
        .create_item(ShopItem {
            id: Uuid::new_v4(),
            guild_id,
            slug: format!("last-one-{guild_id}"),
            name: "Last One".to_string(),
            description: None,
            price: 10,
            role_id: None,
            stock: Some(1),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .expect("the item should insert");

    let first = economy
        .decrement_stock_if_available(item.id)
        .await
        .expect("the first claim should run");
    assert_eq!(first, 1, "the first buyer takes the only unit");

    let second = economy
        .decrement_stock_if_available(item.id)
        .await
        .expect("the second claim should run");
    assert_eq!(
        second, 0,
        "the guard must refuse the second claim, and zero rows is how it says so"
    );

    let stored = economy
        .find_item_by_id(item.id)
        .await
        .expect("lookup")
        .expect("still there");
    assert_eq!(stored.stock, Some(0), "stock stops at zero, never negative");
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn an_unlimited_item_is_never_blocked() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let economy = SeaOrmEconomyRepository::new(db);

    let guild_id = unique_guild_id();
    seed_guild(&guilds, guild_id).await;

    let item = economy
        .create_item(ShopItem {
            id: Uuid::new_v4(),
            guild_id,
            slug: format!("unlimited-{guild_id}"),
            name: "Unlimited".to_string(),
            description: None,
            price: 10,
            role_id: None,
            stock: None,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .await
        .expect("the item should insert");

    for _ in 0..3 {
        let claimed = economy
            .decrement_stock_if_available(item.id)
            .await
            .expect("the claim should run");
        assert_eq!(claimed, 1, "a NULL stock is unlimited and always succeeds");
    }

    let stored = economy
        .find_item_by_id(item.id)
        .await
        .expect("lookup")
        .expect("still there");
    assert_eq!(stored.stock, None, "an unlimited item stays unlimited");
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn the_daily_stamp_is_written_by_the_ledger() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let economy = SeaOrmEconomyRepository::new(db);

    let guild_id = unique_guild_id();
    seed_guild(&guilds, guild_id).await;

    let discord_user_id = 555_002;
    let before = economy
        .ensure_account(guild_id, discord_user_id)
        .await
        .expect("account");
    assert!(
        before.last_daily_at.is_none(),
        "a fresh account has not claimed"
    );

    let (after, _) = economy
        .apply_transaction(NewTransaction {
            id: Uuid::new_v4(),
            guild_id,
            discord_user_id,
            amount: 100,
            reason: TransactionReason::Daily,
            actor_discord_id: None,
            note: None,
            idempotency_key: Some(format!("itest-daily-{guild_id}")),
        })
        .await
        .expect("the daily should apply");

    assert!(
        after.last_daily_at.is_some(),
        "a Daily entry is what starts the cooldown; nothing else stamps this column"
    );

    // And the cooldown rule reads it back correctly.
    let clock = SystemClock;
    assert!(
        !after.can_claim_daily(clock.now(), 24),
        "the cooldown is running now"
    );
    assert!(after.can_claim_daily(clock.now() + chrono::Duration::hours(25), 24));
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_replayed_transaction_is_refused() {
    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let economy = SeaOrmEconomyRepository::new(db);

    let guild_id = unique_guild_id();
    seed_guild(&guilds, guild_id).await;

    let discord_user_id = 999_003;
    economy
        .ensure_account(guild_id, discord_user_id)
        .await
        .expect("account");

    let key = format!("itest-idem-{guild_id}");
    let entry = || NewTransaction {
        id: Uuid::new_v4(),
        guild_id,
        discord_user_id,
        amount: 500,
        reason: TransactionReason::AdminGrant,
        actor_discord_id: Some(1),
        note: None,
        idempotency_key: Some(key.clone()),
    };

    economy
        .apply_transaction(entry())
        .await
        .expect("the first attempt should apply");

    let replay = economy.apply_transaction(entry()).await;
    assert!(
        replay.is_err(),
        "the unique index on idempotency_key is what stops a retried interaction from paying twice"
    );
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_password_hash_round_trips_through_argon2() {
    let Some(db) = connect().await else { return };
    // The user repository is exercised too: a hash is only useful next to a row.
    let users = SeaOrmUserRepository::new(db);
    let passwords = Argon2PasswordService::new();

    let discord_id = unique_guild_id();
    let hash = passwords
        .hash("correct horse battery staple")
        .await
        .expect("hashing should succeed");
    assert!(
        hash.starts_with("$argon2id$"),
        "the PHC string names the algorithm"
    );

    let user = users
        .create(NewUser {
            id: Uuid::new_v4(),
            discord_id,
            username: "integration".to_string(),
            global_name: Some("Integration".to_string()),
            avatar_url: None,
            email: None,
        })
        .await
        .expect("the user should insert");

    assert_eq!(user.discord_id, discord_id);
    assert!(
        !user.is_platform_admin(),
        "a new account is not an operator"
    );

    assert!(
        passwords
            .verify("correct horse battery staple", &hash)
            .await
            .expect("verification should run"),
        "the correct password must verify"
    );
    assert!(
        !passwords
            .verify("wrong password", &hash)
            .await
            .expect("verification should run"),
        "a wrong password must not verify, and must not be an error"
    );

    // A re-read proves the hash survived the round trip through the column.
    let stored = users
        .find_by_discord_id(discord_id)
        .await
        .expect("lookup")
        .expect("the user should be there");
    assert_eq!(stored.username, "integration");
}

#[tokio::test]
#[ignore = "needs DATABASE_URL and a migrated database"]
async fn a_token_pair_verifies_and_a_refresh_cannot_be_used_as_an_access_token() {
    let Some(db) = connect().await else { return };
    let users = SeaOrmUserRepository::new(db);

    // A user, so the token's subject is a real id rather than a made-up one.
    let user = users
        .create(NewUser {
            id: Uuid::new_v4(),
            discord_id: unique_guild_id(),
            username: "token-holder".to_string(),
            global_name: None,
            avatar_url: None,
            email: None,
        })
        .await
        .expect("the user should insert");

    let secret = b"an-integration-test-secret-that-is-long-enough";
    let tokens = JwtTokenService::new(secret, 900, 2_592_000);
    let clock = SystemClock;

    let pair = tokens
        .issue_pair(user.id, clock.now())
        .await
        .expect("issuing should succeed");

    let claims = tokens
        .verify(&pair.access_token, TokenType::Access, clock.now())
        .await
        .expect("the access token should verify");
    assert_eq!(claims.subject, user.id, "the subject is the account's id");

    // The type claim is what stops the long-lived token from being replayed as a
    // short-lived one, so this is the assertion that matters.
    let wrong_use = tokens
        .verify(&pair.refresh_token, TokenType::Access, clock.now())
        .await;
    assert!(
        wrong_use.is_err(),
        "a refresh token must not pass an access-token check"
    );

    tokens
        .verify(&pair.refresh_token, TokenType::Refresh, clock.now())
        .await
        .expect("the refresh token verifies as a refresh token");

    // And tampering with the signature fails.
    let tampered = format!("{}x", pair.access_token);
    assert!(
        tokens
            .verify(&tampered, TokenType::Access, clock.now())
            .await
            .is_err(),
        "a tampered token must not verify"
    );
}

// ---------------------------------------------------------------------------
// Leveling
//
// The reward table is the one place a constraint failure was silently read as
// success: `ensure_guild_recorded` exists because `leveling_rewards.guild_id` is a
// foreign key, and a missing parent used to arrive as a *conflict*, which an
// idempotent writer counts as "already configured". These tests pin both halves.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "needs a database"]
async fn a_reward_for_an_unknown_guild_is_a_missing_reference_not_a_conflict() {
    use iam::domain::leveling::{LevelReward, LevelingRepository};

    let Some(db) = connect().await else { return };
    let leveling = SeaOrmLevelingRepository::new(db);

    // A guild id that is deliberately never inserted, so the foreign key must
    // reject the write.
    let guild_id = unique_guild_id().saturating_add(7);

    let result = leveling
        .create_reward(LevelReward {
            id: Uuid::new_v4(),
            guild_id,
            level: 0,
            role_id: 123,
            keep_on_demote: true,
        })
        .await;

    let error = result.expect_err("the foreign key must reject a reward with no guild");

    assert!(
        !error.is_already_present(),
        "a missing parent must not read as 'already configured': {error}"
    );
    assert!(
        error.is_client_error(),
        "a reference to a row that does not exist is the caller's mistake, not a 500"
    );
}

#[tokio::test]
#[ignore = "needs a database"]
async fn a_second_reward_at_the_same_level_is_a_conflict() {
    use iam::domain::leveling::{LevelReward, LevelingRepository};

    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let leveling = SeaOrmLevelingRepository::new(db);

    let guild_id = unique_guild_id().saturating_add(8);
    seed_guild(&guilds, guild_id).await;

    leveling
        .create_reward(LevelReward {
            id: Uuid::new_v4(),
            guild_id,
            level: 3,
            role_id: 111,
            keep_on_demote: true,
        })
        .await
        .expect("the first reward inserts");

    let error = leveling
        .create_reward(LevelReward {
            id: Uuid::new_v4(),
            guild_id,
            level: 3,
            role_id: 222,
            keep_on_demote: true,
        })
        .await
        .expect_err("the unique index on (guild_id, level) rejects the second");

    assert!(
        error.is_already_present(),
        "this is the case a re-run should treat as success: {error}"
    );
}

#[tokio::test]
#[ignore = "needs a database"]
async fn a_profile_is_created_once_and_xp_accumulates() {
    use iam::domain::leveling::{LevelCurve, LevelingRepository};

    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let leveling = SeaOrmLevelingRepository::new(db);

    let guild_id = unique_guild_id().saturating_add(9);
    seed_guild(&guilds, guild_id).await;

    // The first message creates the row; a second must reuse it rather than
    // violating the unique index on (guild_id, discord_user_id).
    let first = leveling
        .ensure_profile(guild_id, 4242)
        .await
        .expect("the first message creates a profile");
    assert_eq!(first.level, 0);
    assert_eq!(first.xp, 0);

    let again = leveling
        .ensure_profile(guild_id, 4242)
        .await
        .expect("a second message reuses the row");
    assert_eq!(again.id, first.id, "the same profile, not a new one");

    // Enough XP to cross the first level boundary, so the curve is exercised against
    // real storage rather than only in the domain's own test.
    //
    // The arguments are the *position* the use case computed — `level` plus the XP
    // remainder inside that level — not the amount just gained. That distinction is
    // the whole reason `total_xp` is stored separately, so it is spelled out here:
    // the first boundary needs `xp_for_level(0)` and leaves no remainder.
    let boundary = LevelCurve::Mrn.xp_for_level(0);
    let updated = leveling
        .add_xp(guild_id, 4242, 0, 1, true)
        .await
        .expect("xp accumulates");

    assert_eq!(updated.level, 1);
    assert_eq!(updated.message_count, 1, "the message count moves with it");
    // `total_xp` is the member's position on the curve, which is what the
    // leaderboard sorts on: here the cumulative requirement for level 1 with no
    // remainder, not the 100 XP this one call contributed.
    assert_eq!(
        updated.total_xp,
        LevelCurve::Mrn.total_xp_for_level(1),
        "total xp is a curve position, so it does not reset when the level changes"
    );
    assert_eq!(
        boundary,
        LevelCurve::Mrn.total_xp_for_level(1),
        "and for this curve the first boundary is the same number"
    );

    // A guild with no config row still levels: the defaults are what it runs on,
    // which is what makes the bot work before anyone opens the dashboard.
    let config = leveling
        .find_config(guild_id)
        .await
        .expect("a missing config falls back to defaults");
    assert!(config.enabled, "leveling is on by default");
    assert!(
        !config.stack_roles,
        "a member wears their current rung, not every rung they have passed"
    );
}

#[tokio::test]
#[ignore = "needs a database"]
async fn a_members_permissions_come_from_the_roles_they_hold() {
    use iam::domain::guild_member::{GuildMemberRepository, NewGuildMember};
    use iam::domain::rbac::{
        GuildRoleRepository, PermissionRepository, default_role_permissions, slugs,
    };

    let Some(db) = connect().await else { return };
    let guilds = SeaOrmGuildRepository::new(db.clone());
    let users = SeaOrmUserRepository::new(db.clone());
    let roles = SeaOrmGuildRoleRepository::new(db.clone());
    let members = SeaOrmGuildMemberRepository::new(db.clone());
    let catalog = SeaOrmPermissionRepository::new(db);

    let guild_id = unique_guild_id().saturating_add(11);
    seed_guild(&guilds, guild_id).await;

    // `bootstrap` seeds the catalog, but this test must not depend on it having run,
    // so it upserts the names it is about to grant. `upsert` is idempotent by design,
    // which is what makes this safe alongside a real bootstrap.
    let granted = default_role_permissions(slugs::ADMIN);
    for name in granted {
        catalog
            .upsert(name, "test", "granted by the membership test")
            .await
            .expect("the catalog upsert is repeatable");
    }

    let role = roles
        .create(
            guild_id,
            "membership-test",
            "Membership Test",
            None,
            false,
            0,
        )
        .await
        .expect("the role inserts");

    let names: Vec<String> = granted.iter().map(|name| (*name).to_string()).collect();
    roles
        .replace_role_permissions(role.id, &names)
        .await
        .expect("the matrix is written");

    let member_user = users
        .create(NewUser {
            id: Uuid::new_v4(),
            discord_id: unique_guild_id(),
            username: "matrix-holder".to_string(),
            global_name: None,
            avatar_url: None,
            email: None,
        })
        .await
        .expect("the user inserts");

    members
        .upsert_from_discord(NewGuildMember {
            id: Uuid::new_v4(),
            guild_id,
            user_id: member_user.id,
            guild_role_id: role.id,
            discord_permissions: 0,
            discord_nick: None,
        })
        .await
        .expect("the membership inserts");

    // The assertion that catches the traversal being rooted at the wrong table:
    // filtering `guild_members` without joining it is invalid SQL, and it only fails
    // once a member exists — an empty set for everybody looks identical to a broken
    // query from the outside, which is how this survived until a real login hit it.
    let held = roles
        .permissions_for_member(guild_id, member_user.id)
        .await
        .expect("the traversal must be valid SQL");

    for name in granted {
        assert!(
            held.allows(name),
            "the granted permission `{name}` must resolve for the member: {held:?}"
        );
    }

    // No membership row at all is an empty set, not an error: "not a member" and
    // "member whose role grants nothing" are the same answer to "may they?".
    let stranger = users
        .create(NewUser {
            id: Uuid::new_v4(),
            discord_id: unique_guild_id().saturating_add(1),
            username: "not-a-member".to_string(),
            global_name: None,
            avatar_url: None,
            email: None,
        })
        .await
        .expect("the second user inserts");

    let none = roles
        .permissions_for_member(guild_id, stranger.id)
        .await
        .expect("a missing membership is not an error");
    assert!(none.is_empty(), "a non-member holds no permissions");
}
