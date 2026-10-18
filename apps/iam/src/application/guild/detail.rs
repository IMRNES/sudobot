use crate::domain::guild::{Guild, GuildError, GuildRepository};

/// Reads one guild.
pub struct GuildDetailUseCase<G> {
    guilds: G,
}

impl<G: GuildRepository> GuildDetailUseCase<G> {
    pub fn new(guilds: G) -> Self {
        Self { guilds }
    }

    pub async fn execute(&self, guild_id: u64) -> Result<Guild, GuildError> {
        self.guilds
            .find_by_id(guild_id)
            .await?
            .ok_or(GuildError::NotFound)
    }
}
