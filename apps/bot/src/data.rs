/// Everything a command or event handler needs, in one cheap-to-clone value.
///
/// The repositories are built from a `DatabaseConnection` that is itself
/// reference-counted, so cloning `Data` copies a handful of pointers rather than
/// opening connections. That is what lets the same value be shared between
/// poise's framework data and the serenity event handler.
#[derive(Clone)]
pub struct Data {
    pub guilds: iam::infrastructure::repository::SeaOrmGuildRepository,
    pub guild_roles: iam::infrastructure::repository::SeaOrmGuildRoleRepository,
    pub auto_roles: iam::infrastructure::repository::SeaOrmAutoRoleRepository,
    pub leveling: iam::infrastructure::repository::SeaOrmLevelingRepository,
    pub economy: iam::infrastructure::repository::SeaOrmEconomyRepository,
    pub welcome: iam::infrastructure::repository::SeaOrmWelcomeRepository,
    /// Jail configuration and the sentences served on it.
    pub jail: iam::infrastructure::repository::SeaOrmJailRepository,
    /// Posts role panels as the bot.
    ///
    /// The same publisher the API uses, so `/rolepanel post` and the dashboard write
    /// the identical message. Holds the token it was built with, so it needs no
    /// access to `http` above.
    pub panel_publisher: iam::infrastructure::service::ReqwestRolePanelPublisher,
    pub clock: iam::application::ports::SystemClock,
    /// Guild to register slash commands to during development, for instant updates.
    pub dev_guild_id: Option<u64>,
    /// Guild whose level ladder is provisioned at startup, so the ladder ships with
    /// the deploy rather than being built by hand in the Discord UI.
    provision_guild_id: Option<u64>,
    /// The REST client the background sweeper uses.
    ///
    /// Held behind an `Arc` because `serenity::Http` is not `Clone`, while `Data`
    /// must be: the framework and the event handler each get their own clone. The
    /// `Arc` means both share one connection pool and one rate limiter rather than
    /// one client hammering Discord from two directions.
    http: std::sync::Arc<poise::serenity_prelude::Http>,
    /// Open voice sessions, shared across clones.
    ///
    /// Behind an `Arc` for the same reason as `http`, and because it is *state*:
    /// `Data` is cloned per event, so a plain field would give every event its own
    /// private, empty map and no session would ever be paid out.
    voice: std::sync::Arc<crate::voice_tracker::VoiceTracker>,
}

impl Data {
    pub fn new(
        db: sea_orm::DatabaseConnection,
        dev_guild_id: Option<u64>,
        provision_guild_id: Option<u64>,
        token: &str,
    ) -> Self {
        use iam::infrastructure::repository::{
            SeaOrmAutoRoleRepository, SeaOrmEconomyRepository, SeaOrmGuildRepository,
            SeaOrmGuildRoleRepository, SeaOrmJailRepository, SeaOrmLevelingRepository,
            SeaOrmWelcomeRepository,
        };

        Self {
            guilds: SeaOrmGuildRepository::new(db.clone()),
            guild_roles: SeaOrmGuildRoleRepository::new(db.clone()),
            auto_roles: SeaOrmAutoRoleRepository::new(db.clone()),
            leveling: SeaOrmLevelingRepository::new(db.clone()),
            economy: SeaOrmEconomyRepository::new(db.clone()),
            welcome: SeaOrmWelcomeRepository::new(db.clone()),
            jail: SeaOrmJailRepository::new(db),
            // Built once with the token the process already has, rather than
            // reaching through `http`: the publisher needs a reqwest client of its
            // own so it can set a timeout and a user agent.
            panel_publisher: iam::infrastructure::service::ReqwestRolePanelPublisher::new(Some(
                token.to_string(),
            ))
            .expect("building the panel publisher"),
            clock: iam::application::ports::SystemClock,
            dev_guild_id,
            provision_guild_id,
            http: std::sync::Arc::new(poise::serenity_prelude::Http::new(token)),
            voice: std::sync::Arc::new(crate::voice_tracker::VoiceTracker::new()),
        }
    }

    /// The guild whose level ladder this process provisions on connect, if any.
    pub fn provision_guild_id(&self) -> Option<u64> {
        self.provision_guild_id
    }

    /// The REST client, as the sweeper needs it.
    pub fn http(&self) -> std::sync::Arc<poise::serenity_prelude::Http> {
        std::sync::Arc::clone(&self.http)
    }

    /// The shared voice-session registry.
    pub fn voice(&self) -> &crate::voice_tracker::VoiceTracker {
        &self.voice
    }
}

/// The gateway events poise does not dispatch to commands.
///
/// The dispatch path is singular and it starts in `main`: poise's
/// `FrameworkOptions::event_handler` is the only thing wired to the client, and it
/// forwards every `FullEvent` to [`crate::events::handle`]. `Data` itself
/// deliberately does **not** implement serenity's `EventHandler`, because
/// `dispatch_model` runs the registered event-handler list *and* the framework on
/// the same event — registering both made every join and every member removal run
/// twice, which posted the welcome message twice.
///
/// What the handlers need from `Data` they take through `FrameworkContext::user_data`,
/// which hands them this same `Arc<Data>`.
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// The event layer's whole contract with the rest of the bot.
    ///
    /// `Data` is cloned per event and the framework hands each clone the *same*
    /// `Arc`ed clients, so a shared field must survive the clone. A voice tracker
    /// held by value would give every event its own empty map and no session would
    /// ever be paid — this test is what stops that regression.
    #[test]
    fn cloning_data_shares_the_voice_tracker_and_the_http_client() {
        let data = Data::new(
            sea_orm::DatabaseConnection::default(),
            None,
            None,
            "a-token-that-is-never-used-because-nothing-here-calls-discord",
        );

        let clone = data.clone();

        // Same allocation, not two equal maps: sessions opened through one clone
        // must be visible to the other, which is the only way an event that opens a
        // session and an event that closes it can agree.
        assert!(std::ptr::eq(data.voice(), clone.voice()));
        assert!(std::sync::Arc::ptr_eq(&data.http(), &clone.http()));
    }

    #[test]
    fn a_fresh_tracker_is_empty() {
        let data = Data::new(
            sea_orm::DatabaseConnection::default(),
            None,
            None,
            "a-token-that-is-never-used-because-nothing-here-calls-discord",
        );

        assert!(data.voice().is_empty());
    }

    /// The provisioning guild is what makes the ladder a deployable artifact: read
    /// it back so a constructor that silently dropped it cannot ship, which would
    /// leave the bot connecting fine and granting no roles forever.
    #[test]
    fn the_provisioning_guild_is_carried_through() {
        let data = Data::new(
            sea_orm::DatabaseConnection::default(),
            None,
            Some(1_450_861_658_061_803_624),
            "a-token-that-is-never-used-because-nothing-here-calls-discord",
        );

        assert_eq!(data.provision_guild_id(), Some(1_450_861_658_061_803_624));
    }

    #[test]
    fn the_dev_guild_is_carried_through() {
        let data = Data::new(
            sea_orm::DatabaseConnection::default(),
            Some(42),
            Some(99),
            "a-token-that-is-never-used-because-nothing-here-calls-discord",
        );

        assert_eq!(data.dev_guild_id, Some(42));
        assert_eq!(data.provision_guild_id(), Some(99));
    }
}
