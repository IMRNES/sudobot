use crate::domain::errors::{Page, PaginationParams};
use crate::domain::guild::{Guild, GuildError, GuildRepository};

/// Lists guilds the bot is in, for the platform-admin view.
///
/// Ordinary users get their guild list from `GetMeUseCase` instead: it is the
/// guilds *they* administer, which is a different question with a different
/// answer for almost everyone.
pub struct ListGuildsUseCase<G> {
    guilds: G,
}

impl<G: GuildRepository> ListGuildsUseCase<G> {
    pub fn new(guilds: G) -> Self {
        Self { guilds }
    }

    pub async fn execute(&self, params: &PaginationParams) -> Result<Page<Guild>, GuildError> {
        Ok(self.guilds.list(params).await?)
    }

    /// The guild count shown on the landing page.
    pub async fn count(&self) -> Result<u64, GuildError> {
        Ok(self.guilds.count().await?)
    }
}
