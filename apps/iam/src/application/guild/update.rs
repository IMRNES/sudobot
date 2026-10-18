use tracing::info;

use crate::domain::guild::{Guild, GuildError, GuildPatch, GuildRepository, validate_guild_name};

/// Renames a guild in the dashboard.
///
/// Only the name is editable: everything else on the row is Discord's to say, and
/// the next sync would overwrite a local edit anyway.
#[derive(Debug, Clone)]
pub struct UpdateGuildCommand {
    pub guild_id: u64,
    pub name: Option<String>,
}

pub struct UpdateGuildUseCase<G> {
    guilds: G,
}

impl<G: GuildRepository> UpdateGuildUseCase<G> {
    pub fn new(guilds: G) -> Self {
        Self { guilds }
    }

    pub async fn execute(&self, command: UpdateGuildCommand) -> Result<Guild, GuildError> {
        let patch = GuildPatch {
            name: command
                .name
                .map(|name| validate_guild_name(&name))
                .transpose()?,
            ..Default::default()
        };

        // Nothing to change is not an error: a form submitted unchanged should
        // return the current state rather than a 400 the user cannot act on.
        if patch.is_empty() {
            return self
                .guilds
                .find_by_id(command.guild_id)
                .await?
                .ok_or(GuildError::NotFound);
        }

        let guild = self.guilds.update(command.guild_id, patch).await?;
        info!(guild_id = %guild.id, "guild updated");
        Ok(guild)
    }
}
