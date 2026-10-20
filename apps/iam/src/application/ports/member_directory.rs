//! Reading the names the bot has seen for members.
//!
//! The bot records a member's display name whenever they speak, and that cache is
//! the only place the API knows what to call someone. Both leaderboards need it —
//! a balance table of `1504073757256318976` is not a leaderboard anyone can read —
//! so the lookup is a port rather than a method on one feature's repository.
//!
//! This is a *port*, not a source of truth: a member who has never spoken has no
//! entry, and callers must render `None` as an id rather than inventing a name.

use std::collections::HashMap;
use std::future::Future;

use crate::domain::errors::RepositoryError;

pub trait MemberDirectory: Send + Sync {
    /// The cached names for these members, keyed by discord id.
    ///
    /// One call for a whole page. A missing key means no name is on record, which
    /// is ordinary — most members never speak — and never an error.
    fn names(
        &self,
        guild_id: u64,
        discord_user_ids: &[u64],
    ) -> impl Future<Output = Result<HashMap<u64, String>, RepositoryError>> + Send;
}
