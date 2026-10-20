//! The member-name cache, read by the leaderboards.
//!
//! The rows live in `guild_member_profiles`, which the leveling repository already
//! owns — the bot writes a name there on every message because that is the same row
//! an XP grant touches. Re-declaring the query against the same table here would
//! put the column names in two places, so this adapter delegates to the repository
//! that already knows the table.

use std::collections::HashMap;
use std::future::Future;

use crate::application::ports::MemberDirectory;
use crate::domain::errors::RepositoryError;
use crate::domain::leveling::LevelingRepository;

use super::SeaOrmLevelingRepository;

impl MemberDirectory for SeaOrmLevelingRepository {
    fn names(
        &self,
        guild_id: u64,
        discord_user_ids: &[u64],
    ) -> impl Future<Output = Result<HashMap<u64, String>, RepositoryError>> + Send {
        self.member_names(guild_id, discord_user_ids)
    }
}
