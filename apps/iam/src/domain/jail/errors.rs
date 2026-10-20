use thiserror::Error;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum JailError {
    #[error("this server has no jail configured; set the jail role and channel first")]
    NotConfigured,

    #[error("that member is not in jail")]
    NotImprisoned(u64),

    #[error("that member is already in jail")]
    AlreadyImprisoned(u64),

    #[error("the jail role and the jail channel must be different")]
    SameRoleAndChannel,

    #[error("a sentence must be longer than zero seconds")]
    InvalidDuration,

    #[error("a bail price must be greater than zero; omit it to disable bail")]
    InvalidBailAmount,

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl JailError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::Repository(_))
    }
}
