use crate::domain::errors::{Page, PaginationParams};
use crate::domain::guild::{Guild, GuildError, GuildRepository};

/// Lists guilds the bot is in, for the platform-admin view.
///
/// Distinct from the dashboard's own view, which is not a list at all: the
/// dashboard serves one guild, taken from configuration, and its callers have
/// already been admitted by the whitelist gate.
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
