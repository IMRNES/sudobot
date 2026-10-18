use thiserror::Error;
use uuid::Uuid;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum GuildMemberError {
    #[error("membership not found")]
    NotFound,

    #[error("the guild owner cannot be demoted")]
    CannotDemoteOwner,

    #[error("you cannot change your own membership role")]
    CannotChangeSelf,

    #[error("cannot remove the last member holding `{0}`")]
    WouldRemoveLastHolder(String),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl GuildMemberError {
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::NotFound
                | Self::CannotDemoteOwner
                | Self::CannotChangeSelf
                | Self::WouldRemoveLastHolder(_)
        )
    }
}

/// Refuses a role change the actor is not allowed to make.
///
/// Two rules, both learned the hard way by every dashboard that omitted them:
/// nobody edits their own membership (that is how an admin accidentally locks
/// themselves out mid-session), and the guild owner stays where they are.
pub fn ensure_can_change_membership(
    actor_user_id: Uuid,
    target_user_id: Uuid,
    target_is_owner: bool,
) -> Result<(), GuildMemberError> {
    if actor_user_id == target_user_id {
        return Err(GuildMemberError::CannotChangeSelf);
    }
    if target_is_owner {
        return Err(GuildMemberError::CannotDemoteOwner);
    }
    Ok(())
}

/// Refuses the change that would leave a guild with nobody able to administer it.
pub fn ensure_holder_remains(
    remaining_holders: usize,
    permission: &str,
) -> Result<(), GuildMemberError> {
    if remaining_holders == 0 {
        tracing::warn!(permission, "refused a membership change leaving no holder");
        return Err(GuildMemberError::WouldRemoveLastHolder(
            permission.to_string(),
        ));
    }
    Ok(())
}
