use std::future::Future;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PasswordError {
    #[error("password hashing failed: {0}")]
    HashFailed(String),

    #[error("the stored password hash is malformed: {0}")]
    MalformedHash(String),
}

/// Hashing and verification, without naming the algorithm at the call site.
///
/// The use case says "hash this"; whether that is Argon2id today or something
/// else after the next OWASP update is infrastructure's business. The error type
/// carries no variant for "wrong password": a mismatch is not an error, it is a
/// `false`, and conflating the two is how "user not found" leaks into timing.
pub trait PasswordService: Send + Sync {
    fn hash(&self, password: &str) -> impl Future<Output = Result<String, PasswordError>> + Send;

    fn verify(
        &self,
        password: &str,
        hash: &str,
    ) -> impl Future<Output = Result<bool, PasswordError>> + Send;
}
