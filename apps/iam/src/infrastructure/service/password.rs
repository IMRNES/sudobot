//! Argon2id password hashing, behind the application's `PasswordService` port.

use argon2::{
    Argon2,
    // `phc::PasswordHash` rather than the `password_hash::PasswordHash` re-export:
    // the latter is a deprecated alias since password-hash 0.6.
    password_hash::{PasswordHasher, PasswordVerifier, phc::PasswordHash},
};
use tokio::task;

use crate::application::ports::{PasswordError, PasswordService};

/// Argon2id, with the parameters `Argon2::default()` pins.
///
/// The algorithm is a detail the application never names: a use case says "hash
/// this", and the day the parameters change it changes here and nowhere else.
#[derive(Clone, Copy, Default)]
pub struct Argon2PasswordService;

impl Argon2PasswordService {
    pub fn new() -> Self {
        Self
    }
}

impl PasswordService for Argon2PasswordService {
    async fn hash(&self, password: &str) -> Result<String, PasswordError> {
        // The password is moved into the blocking task: the closure must be
        // `'static`, and a borrow of the caller's `&str` cannot outlive this call.
        let password = password.to_owned();

        // Argon2 is memory-hard by design — one hash costs ~19 MiB and tens of
        // milliseconds of CPU. Running it on the async runtime would stall every
        // other task on that worker thread for the duration.
        task::spawn_blocking(move || {
            Argon2::default()
                .hash_password(password.as_bytes())
                // The error names the parameter that was rejected; it never contains
                // the password, so it is safe in a log line and in the error body.
                .map(|hash| hash.to_string())
                .map_err(|error| PasswordError::HashFailed(error.to_string()))
        })
        .await
        .map_err(|error| PasswordError::HashFailed(format!("hashing task failed: {error}")))?
    }

    async fn verify(&self, password: &str, hash: &str) -> Result<bool, PasswordError> {
        let password = password.to_owned();
        let hash = hash.to_owned();

        task::spawn_blocking(move || {
            // A malformed stored hash is a data problem, not a wrong password: it is
            // reported as `MalformedHash` so it surfaces as a 500 rather than as a
            // silent "wrong password" that would send someone round the reset flow.
            let parsed = PasswordHash::new(&hash)
                .map_err(|error| PasswordError::MalformedHash(error.to_string()))?;

            match Argon2::default().verify_password(password.as_bytes(), &parsed) {
                Ok(()) => Ok(true),
                // The one variant that means "did not match". Everything else —
                // unsupported parameters, an unrecognised algorithm, a failure in
                // the derivation itself — is a real error and must not be flattened
                // into a `false` that a caller could read as "wrong password".
                Err(argon2::password_hash::Error::PasswordInvalid) => Ok(false),
                Err(error) => Err(PasswordError::HashFailed(error.to_string())),
            }
        })
        .await
        .map_err(|error| PasswordError::HashFailed(format!("verification task failed: {error}")))?
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_hash_verifies_against_its_own_password() {
        let service = Argon2PasswordService::new();
        let hash = service.hash("correct horse battery staple").await.unwrap();

        // The PHC string names the algorithm and its parameters, which is what makes
        // a stored hash self-describing.
        assert!(hash.starts_with("$argon2id$v=19$"));
        assert!(
            service
                .verify("correct horse battery staple", &hash)
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn a_wrong_password_is_false_not_an_error() {
        let service = Argon2PasswordService::new();
        let hash = service.hash("hunter2hunter2").await.unwrap();

        assert!(!service.verify("hunter3hunter3", &hash).await.unwrap());
    }

    #[tokio::test]
    async fn the_same_password_hashes_differently_every_time() {
        let service = Argon2PasswordService::new();
        let first = service.hash("same password").await.unwrap();
        let second = service.hash("same password").await.unwrap();

        // Each hash carries a fresh random salt; identical output would mean the salt
        // is not being generated.
        assert_ne!(first, second);
    }

    #[tokio::test]
    async fn a_malformed_hash_is_an_error_not_a_mismatch() {
        let service = Argon2PasswordService::new();

        assert!(matches!(
            service.verify("anything", "not-a-phc-string").await,
            Err(PasswordError::MalformedHash(_))
        ));
    }
}
