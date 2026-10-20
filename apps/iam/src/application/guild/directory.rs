//! Reading a guild's roles and channels for the dashboard's pickers.
//!
//! The configuration screens store snowflakes, and a snowflake answers no
//! question a human has. This use case is what turns them back into a name, so
//! the dashboard can render "@Neophyte" and "#general" instead of two numbers.
//!
//! It is a read with no domain rules of its own — Discord owns the list, and the
//! API deliberately does not cache the names in its own schema: a role renamed in
//! Discord must show its new name on the next page load, and a second copy of a
//! name is a second copy that can be wrong.

use crate::application::ports::guild_directory::{
    DirectoryChannel, DirectoryError, DirectoryRole, GuildDirectorySource,
};

/// One guild's roles and channels.
pub struct GuildDirectoryUseCase<D> {
    directory: D,
}

impl<D: GuildDirectorySource> GuildDirectoryUseCase<D> {
    pub fn new(directory: D) -> Self {
        Self { directory }
    }

    /// Reads the guild's roles and channels.
    ///
    /// The two lists are ordered here rather than by the source, because the order
    /// is a presentation concern: roles descend by position so the ladder reads
    /// top-down as it does in Discord, and channels sort by name so a picker is
    /// scannable. Discord returns roles highest-first already, but relying on that
    /// would make the dashboard's order depend on an ordering Discord never
    /// promised.
    pub async fn execute(&self, guild_id: u64) -> Result<GuildDirectoryView, DirectoryError> {
        let directory = self.directory.fetch(guild_id).await?;

        let mut roles = directory.roles;
        roles.sort_by(|left, right| {
            right
                .position
                .cmp(&left.position)
                .then_with(|| left.name.cmp(&right.name))
        });

        let mut channels = directory.channels;
        channels.sort_by(|left, right| left.name.cmp(&right.name));

        Ok(GuildDirectoryView { roles, channels })
    }
}

/// A guild's directory, in the order a dashboard renders it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuildDirectoryView {
    pub roles: Vec<DirectoryRole>,
    pub channels: Vec<DirectoryChannel>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    /// A source that answers from a fixed snapshot, or refuses.
    struct FakeDirectory {
        outcome: Result<crate::application::ports::guild_directory::GuildDirectory, DirectoryError>,
    }

    impl GuildDirectorySource for FakeDirectory {
        fn is_configured(&self) -> bool {
            true
        }

        async fn fetch(
            &self,
            _guild_id: u64,
        ) -> Result<crate::application::ports::guild_directory::GuildDirectory, DirectoryError>
        {
            match &self.outcome {
                Ok(directory) => Ok(directory.clone()),
                Err(DirectoryError::NotConfigured) => Err(DirectoryError::NotConfigured),
                Err(other) => Err(DirectoryError::Api(other.to_string())),
            }
        }
    }

    fn role(id: u64, name: &str, position: i32) -> DirectoryRole {
        DirectoryRole {
            id,
            name: name.to_string(),
            color: 0,
            position,
            hoist: false,
            mentionable: false,
            managed: false,
        }
    }

    fn channel(id: u64, name: &str) -> DirectoryChannel {
        DirectoryChannel {
            id,
            name: name.to_string(),
            kind: 0,
            parent_id: None,
        }
    }

    fn use_case(
        roles: Vec<DirectoryRole>,
        channels: Vec<DirectoryChannel>,
    ) -> GuildDirectoryUseCase<FakeDirectory> {
        GuildDirectoryUseCase::new(FakeDirectory {
            outcome: Ok(crate::application::ports::guild_directory::GuildDirectory {
                roles,
                channels,
            }),
        })
    }

    #[tokio::test]
    async fn roles_come_back_highest_first() {
        let case = use_case(
            vec![role(1, "Neophyte", 2), role(2, "IMMORTAL", 22)],
            vec![],
        );

        let view = case.execute(1).await.unwrap();
        assert_eq!(
            view.roles
                .iter()
                .map(|role| role.name.as_str())
                .collect::<Vec<_>>(),
            vec!["IMMORTAL", "Neophyte"],
        );
    }

    #[tokio::test]
    async fn equal_positions_fall_back_to_the_name() {
        // Discord hands many managed roles position 0; without a tiebreak their
        // order would vary between calls and the picker would reshuffle itself.
        let case = use_case(vec![role(3, "zeta", 0), role(1, "alpha", 0)], vec![]);

        let view = case.execute(1).await.unwrap();
        assert_eq!(
            view.roles
                .iter()
                .map(|role| role.name.as_str())
                .collect::<Vec<_>>(),
            vec!["alpha", "zeta"],
        );
    }

    #[tokio::test]
    async fn channels_come_back_in_name_order() {
        let case = use_case(
            vec![],
            vec![channel(1, "welcome"), channel(2, "announcements")],
        );

        let view = case.execute(1).await.unwrap();
        assert_eq!(
            view.channels
                .iter()
                .map(|channel| channel.name.as_str())
                .collect::<Vec<_>>(),
            vec!["announcements", "welcome"],
        );
    }

    #[tokio::test]
    async fn an_unconfigured_installation_propagates_the_refusal() {
        let case = GuildDirectoryUseCase::new(FakeDirectory {
            outcome: Err(DirectoryError::NotConfigured),
        });

        let outcome = case.execute(1).await;
        assert!(outcome.is_err());
    }
}
