// Tests assert with `unwrap`/`expect` on purpose: a test that cannot fail
// loudly is not testing anything. The workspace bans them in production code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::config::{UpdateWelcomeConfigCommand, UpdateWelcomeConfigUseCase};
use super::message::{BuildWelcomeMessageQuery, BuildWelcomeMessageUseCase, WelcomeKind};
use super::panels::{
    AddPanelOptionCommand, AddPanelOptionUseCase, PublishRolePanelUseCase, ResolvePanelButtonQuery,
    ResolvePanelButtonUseCase, UpsertRolePanelCommand, UpsertRolePanelUseCase,
};
use super::test_support::{
    FakePublishCall, FakePublisher, FakeWelcome, option, panel, seed_config, seed_panel, store,
};
use crate::domain::welcome::{RolePanelOption, WelcomeConfig, WelcomeError};

const GUILD: u64 = 1;

fn enabled_config() -> WelcomeConfig {
    WelcomeConfig {
        welcome_enabled: true,
        welcome_channel_id: Some(4242),
        welcome_message: Some("Welcome {mention} to {guild}!".to_string()),
        welcome_embed_color: Some(5793266),
        ..WelcomeConfig::default_for(GUILD)
    }
}

// --- the join path (this is what the gateway calls) -------------------------

#[tokio::test]
async fn a_disabled_welcome_sends_nothing() {
    let store = store();
    let use_case = BuildWelcomeMessageUseCase::new(FakeWelcome::new(store));

    let message = use_case
        .execute(query(WelcomeKind::Welcome))
        .await
        .expect("a disabled welcome is not an error");

    assert!(message.is_none());
}

#[tokio::test]
async fn enabled_without_a_channel_sends_nothing() {
    let store = store();
    seed_config(
        &store,
        WelcomeConfig {
            welcome_enabled: true,
            welcome_channel_id: None,
            welcome_message: Some("hello {user}".to_string()),
            ..WelcomeConfig::default_for(GUILD)
        },
    );
    let use_case = BuildWelcomeMessageUseCase::new(FakeWelcome::new(store));

    let message = use_case
        .execute(query(WelcomeKind::Welcome))
        .await
        .expect("a misconfigured welcome must not fail a join");

    assert!(message.is_none());
}

#[tokio::test]
async fn a_configured_welcome_is_rendered() {
    let store = store();
    seed_config(&store, enabled_config());
    let use_case = BuildWelcomeMessageUseCase::new(FakeWelcome::new(store));

    let message = use_case
        .execute(query(WelcomeKind::Welcome))
        .await
        .expect("the config is stored")
        .expect("there is a message to send");

    assert_eq!(message.channel_id, 4242);
    assert_eq!(message.content, "Welcome <@555> to Sudobot Test!");
    assert_eq!(message.embed_color, Some(5793266));
    // Direct messages follow their own switch, which is off here.
    assert!(!message.send_dm);
}

#[tokio::test]
async fn a_welcome_without_a_template_falls_back_to_a_default() {
    let store = store();
    seed_config(
        &store,
        WelcomeConfig {
            welcome_enabled: true,
            welcome_channel_id: Some(7),
            // Enabled but never written: a silent no-op would be the worst answer.
            welcome_message: None,
            ..WelcomeConfig::default_for(GUILD)
        },
    );
    let use_case = BuildWelcomeMessageUseCase::new(FakeWelcome::new(store));

    let message = use_case
        .execute(query(WelcomeKind::Welcome))
        .await
        .expect("the config is stored")
        .expect("a default stands in for the missing template");

    assert_eq!(
        message.content,
        "Welcome <@555> to Sudobot Test! You are member #42."
    );
}

#[tokio::test]
async fn the_goodbye_path_is_configured_independently_of_the_welcome() {
    let store = store();
    let mut config = enabled_config();
    config.goodbye_enabled = true;
    config.goodbye_channel_id = Some(99);
    config.goodbye_message = Some("{user} left {guild}.".to_string());
    config.goodbye_embed_color = Some(1);
    seed_config(&store, config);

    let use_case = BuildWelcomeMessageUseCase::new(FakeWelcome::new(store));

    let goodbye = use_case
        .execute(query(WelcomeKind::Goodbye))
        .await
        .expect("the config is stored")
        .expect("goodbye is enabled");

    assert_eq!(goodbye.channel_id, 99);
    assert_eq!(goodbye.content, "ada left Sudobot Test.");
    assert_eq!(goodbye.embed_color, Some(1));
    // A goodbye is never direct-messaged: the member has already left.
    assert!(!goodbye.send_dm);
}

#[tokio::test]
async fn the_welcome_can_also_be_direct_messaged() {
    let store = store();
    let mut config = enabled_config();
    config.welcome_dm_enabled = true;
    seed_config(&store, config);

    let use_case = BuildWelcomeMessageUseCase::new(FakeWelcome::new(store));

    let message = use_case
        .execute(query(WelcomeKind::Welcome))
        .await
        .expect("the config is stored")
        .expect("there is a message to send");

    assert!(message.send_dm);
}

fn query(kind: WelcomeKind) -> BuildWelcomeMessageQuery {
    BuildWelcomeMessageQuery {
        guild_id: GUILD,
        guild_name: "Sudobot Test".to_string(),
        member_count: 42,
        discord_user_id: 555,
        username: "ada".to_string(),
        kind,
    }
}

// --- the configuration patch ------------------------------------------------

#[tokio::test]
async fn a_bare_patch_field_leaves_the_stored_value_alone() {
    let store = store();
    seed_config(&store, enabled_config());
    let use_case = UpdateWelcomeConfigUseCase::new(FakeWelcome::new(store));

    let stored = use_case
        .execute(UpdateWelcomeConfigCommand {
            guild_id: GUILD,
            welcome_enabled: Some(false),
            // Outer `None` — "not part of this patch".
            welcome_channel_id: None,
            welcome_message: None,
            welcome_embed_color: None,
            welcome_dm_enabled: None,
            goodbye_enabled: None,
            goodbye_channel_id: None,
            goodbye_message: None,
            goodbye_embed_color: None,
        })
        .await
        .expect("the patch only touches one field");

    assert!(!stored.welcome_enabled);
    assert_eq!(stored.welcome_channel_id, Some(4242));
    assert_eq!(
        stored.welcome_message.as_deref(),
        Some("Welcome {mention} to {guild}!")
    );
}

#[tokio::test]
async fn a_double_none_clears_the_field() {
    let store = store();
    seed_config(&store, enabled_config());
    let use_case = UpdateWelcomeConfigUseCase::new(FakeWelcome::new(store));

    let stored = use_case
        .execute(UpdateWelcomeConfigCommand {
            guild_id: GUILD,
            welcome_enabled: None,
            // Outer `Some(inner)`, inner `None` — "set it to null".
            welcome_channel_id: Some(None),
            welcome_message: None,
            welcome_embed_color: Some(None),
            welcome_dm_enabled: None,
            goodbye_enabled: None,
            goodbye_channel_id: None,
            goodbye_message: None,
            goodbye_embed_color: None,
        })
        .await
        .expect("clearing a channel is a valid patch");

    assert_eq!(stored.welcome_channel_id, None);
    assert_eq!(stored.welcome_embed_color, None);
    // The fields that were not part of the patch survived.
    assert_eq!(
        stored.welcome_message.as_deref(),
        Some("Welcome {mention} to {guild}!")
    );
}

#[tokio::test]
async fn a_template_with_a_typo_is_refused_at_save_time() {
    let store = store();
    let use_case = UpdateWelcomeConfigUseCase::new(FakeWelcome::new(store));

    let error = use_case
        .execute(UpdateWelcomeConfigCommand {
            guild_id: GUILD,
            welcome_enabled: None,
            welcome_channel_id: None,
            welcome_message: Some("welcome {membercount}!".to_string()),
            welcome_embed_color: None,
            welcome_dm_enabled: None,
            goodbye_enabled: None,
            goodbye_channel_id: None,
            goodbye_message: None,
            goodbye_embed_color: None,
        })
        .await
        .expect_err("a placeholder the renderer does not know would post literally");

    assert!(matches!(error, WelcomeError::UnknownPlaceholder(_)));
}

// --- panels -----------------------------------------------------------------

#[tokio::test]
async fn a_panel_takes_at_most_twenty_five_buttons() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    let existing: Vec<RolePanelOption> = (0..RolePanelOption::MAX_PER_PANEL)
        .map(|position| option(panel_id, GUILD, position, position as u64))
        .collect();
    seed_panel(&store, panel, existing);

    let use_case = AddPanelOptionUseCase::new(FakeWelcome::new(store));

    let error = use_case
        .execute(AddPanelOptionCommand {
            guild_id: GUILD,
            panel_id,
            label: "One too many".to_string(),
            emoji: None,
            role_id: 900,
            style: 2,
            behavior: "toggle".to_string(),
        })
        .await
        .expect_err("Discord rejects a message with more than 25 components");

    assert!(matches!(error, WelcomeError::TooManyOptions(1)));
}

#[tokio::test]
async fn an_added_button_gets_a_stable_custom_id() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    seed_panel(&store, panel, Vec::new());

    let use_case = AddPanelOptionUseCase::new(FakeWelcome::new(store));

    let created = use_case
        .execute(AddPanelOptionCommand {
            guild_id: GUILD,
            panel_id,
            label: "Announcements".to_string(),
            emoji: Some("📢".to_string()),
            role_id: 456,
            style: 2,
            behavior: "toggle".to_string(),
        })
        .await
        .expect("the first button always fits");

    assert_eq!(created.position, 0);
    assert_eq!(
        created.custom_id,
        RolePanelOption::build_custom_id(panel_id, 0)
    );
}

#[tokio::test]
async fn an_unknown_button_behaviour_is_refused() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    seed_panel(&store, panel, Vec::new());

    let use_case = AddPanelOptionUseCase::new(FakeWelcome::new(store));

    let error = use_case
        .execute(AddPanelOptionCommand {
            guild_id: GUILD,
            panel_id,
            label: "Nope".to_string(),
            emoji: None,
            role_id: 1,
            style: 2,
            behavior: "toggle-please".to_string(),
        })
        .await
        .expect_err("an unknown behaviour would be a button that does nothing");

    assert!(matches!(error, WelcomeError::UnknownBehavior(_)));
}

#[tokio::test]
async fn a_button_press_resolves_to_its_panel() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    let first = option(panel_id, GUILD, 0, 456);
    let custom_id = first.custom_id.clone();
    seed_panel(&store, panel, vec![first]);

    let use_case = ResolvePanelButtonUseCase::new(FakeWelcome::new(store));

    let resolved = use_case
        .execute(ResolvePanelButtonQuery { custom_id })
        .await
        .expect("the lookup succeeds")
        .expect("the button exists");

    assert_eq!(resolved.panel_id, panel_id);
    assert_eq!(resolved.guild_id, GUILD);
    assert_eq!(resolved.option.role_id, 456);
}

#[tokio::test]
async fn a_button_on_a_deleted_panel_is_none_not_an_error() {
    let store = store();
    let use_case = ResolvePanelButtonUseCase::new(FakeWelcome::new(store));

    let resolved = use_case
        .execute(ResolvePanelButtonQuery {
            custom_id: RolePanelOption::build_custom_id(uuid::Uuid::new_v4(), 0),
        })
        .await
        .expect("an old button is ordinary, not a failure");

    assert!(resolved.is_none());
}

#[tokio::test]
async fn publishing_posts_a_message_and_records_where_it_landed() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    seed_panel(&store, panel, vec![option(panel_id, GUILD, 0, 456)]);

    let publisher = FakePublisher::new();
    let use_case = PublishRolePanelUseCase::new(FakeWelcome::new(store), publisher.clone());

    let published = use_case
        .execute(GUILD, panel_id, 777)
        .await
        .expect("the panel has a button and a channel");

    assert!(published.panel.is_published);
    assert_eq!(published.panel.channel_id, Some(777));
    // The message id comes from Discord, not from the caller.
    assert_eq!(published.panel.message_id, Some(1001));
    assert_eq!(published.options.len(), 1);

    assert_eq!(
        publisher.calls(),
        vec![FakePublishCall::Post {
            channel_id: 777,
            options: 1,
        }]
    );
}

/// Publishing twice must edit the message that exists rather than post a second
/// one: an admin fixing a label would otherwise leave two panels in the channel.
#[tokio::test]
async fn publishing_an_already_published_panel_edits_it() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    seed_panel(&store, panel, vec![option(panel_id, GUILD, 0, 456)]);

    let publisher = FakePublisher::new();
    let use_case = PublishRolePanelUseCase::new(FakeWelcome::new(store), publisher.clone());

    let first = use_case
        .execute(GUILD, panel_id, 777)
        .await
        .expect("posted");
    let second = use_case
        .execute(GUILD, panel_id, 777)
        .await
        .expect("edited");

    assert_eq!(
        first.panel.message_id, second.panel.message_id,
        "the same message is rewritten"
    );
    assert_eq!(
        publisher.calls(),
        vec![
            FakePublishCall::Post {
                channel_id: 777,
                options: 1,
            },
            FakePublishCall::Edit {
                channel_id: 777,
                message_id: first.panel.message_id.expect("recorded"),
                options: 1,
            },
        ]
    );
}

/// A panel with no buttons would post an empty message that does nothing when a
/// member clicks it, so it is refused before Discord sees it.
#[tokio::test]
async fn publishing_a_panel_without_buttons_is_refused() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    seed_panel(&store, panel, vec![]);

    let publisher = FakePublisher::new();
    let use_case = PublishRolePanelUseCase::new(FakeWelcome::new(store), publisher.clone());

    let error = use_case
        .execute(GUILD, panel_id, 777)
        .await
        .expect_err("an empty panel is not postable");

    assert!(matches!(error, WelcomeError::EmptyPanel));
    assert!(
        publisher.calls().is_empty(),
        "nothing reaches Discord when the panel is unpublishable"
    );
}

/// Without a bot token the publish cannot work, and saying so is better than a 500.
#[tokio::test]
async fn publishing_without_a_token_reports_an_unavailable_installation() {
    let store = store();
    let panel = panel(GUILD, "self-roles");
    let panel_id = panel.id;
    seed_panel(&store, panel, vec![option(panel_id, GUILD, 0, 456)]);

    let use_case =
        PublishRolePanelUseCase::new(FakeWelcome::new(store), FakePublisher::unconfigured());

    let error = use_case
        .execute(GUILD, panel_id, 777)
        .await
        .expect_err("no token, no publish");

    assert!(matches!(error, WelcomeError::Publisher(_)));
}

#[tokio::test]
async fn upserting_a_panel_twice_does_not_create_two() {
    let store = store();
    let use_case = UpsertRolePanelUseCase::new(FakeWelcome::new(store.clone()));

    let command = |title: &str| UpsertRolePanelCommand {
        guild_id: GUILD,
        slug: "self-roles".to_string(),
        title: title.to_string(),
        description: None,
        channel_id: Some(100),
    };

    let first = use_case
        .execute(command("Pick your roles"))
        .await
        .expect("saved");
    let second = use_case
        .execute(command("Choose your roles"))
        .await
        .expect("saved");

    assert_eq!(first.panel.id, second.panel.id, "the slug is the identity");
    assert_eq!(second.panel.title, "Choose your roles");
}

#[tokio::test]
async fn a_panel_from_another_guild_is_not_found() {
    let store = store();
    let other = panel(999, "self-roles");
    seed_panel(&store, other, Vec::new());

    let use_case = super::panels::GetRolePanelUseCase::new(FakeWelcome::new(store));

    let error = use_case
        .execute(GUILD, "self-roles")
        .await
        .expect_err("a slug is only unique inside its guild");

    assert!(matches!(error, WelcomeError::PanelNotFound));
}
