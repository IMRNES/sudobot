use thiserror::Error;

use crate::domain::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("the authorization code was rejected by discord")]
    InvalidOAuthCode,

    #[error("discord did not return a usable profile")]
    ProfileUnavailable,

    #[error("the refresh token is not valid")]
    InvalidRefreshToken,

    #[error("the refresh token has expired; sign in again")]
    RefreshTokenExpired,

    #[error("the refresh token has already been used")]
    RefreshTokenReused,

    #[error("the access token is not valid")]
    InvalidAccessToken,

    #[error("the access token has expired")]
    AccessTokenExpired,

    #[error("state does not match; the login attempt may have been tampered with")]
    StateMismatch,

    #[error("the login attempt has expired; start again")]
    StateExpired,

    #[error("no account matches that identity")]
    UserNotFound,

    #[error("failed to reach discord: {0}")]
    DiscordUnavailable(String),

    /// Dashboard login is disabled because this installation has no Discord OAuth
    /// client secret. Not a Discord failure: nothing was attempted.
    #[error("dashboard login is not configured on this installation")]
    LoginNotConfigured,

    #[error("you are not a member of guild {0}")]
    NotAGuildMember(u64),

    /// The caller signed in successfully but holds no whitelisted role in the one
    /// guild this dashboard serves.
    ///
    /// A 403 rather than a 401: the credentials were fine and re-authenticating
    /// would produce the same answer. What is missing is authority.
    #[error("this dashboard is limited to staff; you do not hold an allowed role in that server")]
    NotWhitelisted,

    /// `DASHBOARD_GUILD_ID` names a guild the bot is not actually in.
    ///
    /// Distinct from a refusal: the configuration is wrong, not the caller, and
    /// retrying after an invite fixes it.
    #[error("the configured dashboard guild {0} is unknown to this deployment")]
    DashboardGuildUnavailable(u64),

    #[error(transparent)]
    Repository(#[from] RepositoryError),
}

impl AuthError {
    /// True when the caller could fix the request rather than retry it.
    ///
    /// Everything Discord-side is a client error too: a bad code or a stale state
    /// is the browser's problem to restart, not a server fault.
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::DiscordUnavailable(_) | Self::Repository(_))
    }

    /// Variants that mean the session is over and the client should sign in again.
    pub fn requires_reauth(&self) -> bool {
        matches!(
            self,
            Self::InvalidRefreshToken
                | Self::RefreshTokenExpired
                | Self::RefreshTokenReused
                | Self::InvalidAccessToken
                | Self::AccessTokenExpired
        )
    }
}

/// A random, URL-safe string used as the OAuth `state` parameter.
///
/// Generated from the OS random source, not from a counter or a timestamp: the
/// state value is the only thing preventing an attacker from feeding the callback
/// an authorization code they obtained elsewhere.
pub fn generate_state() -> String {
    use uuid::Uuid;
    // Two v4 UUIDs give 256 bits of randomness without another dependency.
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// How long a login attempt may sit between the consent screen and the callback.
///
/// Short on purpose: a state value that stays valid for hours is a state value
/// worth stealing.
pub const OAUTH_STATE_TTL_SECS: i64 = 600;

/// Confirms a callback's state matches the one the login started with.
pub fn validate_state(
    expected: &str,
    received: &str,
    issued_at: chrono::DateTime<chrono::Utc>,
    now: chrono::DateTime<chrono::Utc>,
) -> Result<(), AuthError> {
    if now.signed_duration_since(issued_at).num_seconds() > OAUTH_STATE_TTL_SECS {
        return Err(AuthError::StateExpired);
    }

    // Constant-time-ish comparison: length first, then a byte-wise fold, so the
    // comparison does not return early on the first differing byte.
    if expected.len() != received.len() {
        return Err(AuthError::StateMismatch);
    }
    let mismatch = expected
        .bytes()
        .zip(received.bytes())
        .fold(0u8, |acc, (a, b)| acc | (a ^ b));

    if mismatch != 0 {
        tracing::warn!("oauth state mismatch");
        return Err(AuthError::StateMismatch);
    }

    Ok(())
}

/// Hashes a refresh token for storage.
///
/// Not a password hash: the token is already high-entropy random, so a fast hash
/// is correct here. Argon2 would only make every refresh cost a hundred
/// milliseconds of CPU for no added security.
pub fn hash_refresh_token(token: &str) -> String {
    use std::fmt::Write;

    // SHA-256 over the token bytes.
    let digest = simple_sha256(token.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// A small SHA-256, kept here to avoid a dependency for one hash.
///
/// Implemented to the specification and covered by the standard test vectors
/// below; this is not a place for cleverness.
fn simple_sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Pad to a multiple of 64 bytes: 0x80, zeros, then the length in bits.
    let mut message = input.to_vec();
    let bit_length = (input.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());

    // The padding above guarantees a whole number of 64-byte blocks and 4-byte
    // words, so indexing fixed-size arrays is exact and states that invariant.
    for chunk in message.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (index, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[index] = u32::from_be_bytes(*word);
        }
        for index in 16..64 {
            let s0 = w[index - 15].rotate_right(7)
                ^ w[index - 15].rotate_right(18)
                ^ (w[index - 15] >> 3);
            let s1 = w[index - 2].rotate_right(17)
                ^ w[index - 2].rotate_right(19)
                ^ (w[index - 2] >> 10);
            w[index] = w[index - 16]
                .wrapping_add(s0)
                .wrapping_add(w[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[index])
                .wrapping_add(w[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (index, word) in h.iter().enumerate() {
        out[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
// Assertions in a test are meant to be loud; the workspace ban applies to the
// production code above this module.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    // `TimeZone` brings `with_ymd_and_hms`; `Utc` itself is not re-exported by the
    // module's own imports, and the test uses `Utc::now()`.
    use chrono::{TimeZone, Utc};

    /// The standard SHA-256 vectors. If these fail, the hash is not SHA-256 and
    /// stored refresh tokens would not match on the next deploy.
    #[test]
    fn sha256_matches_the_published_vectors() {
        let cases = [
            (
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
                "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            ),
        ];

        for (input, expected) in cases {
            assert_eq!(hash_refresh_token(input), expected, "input: {input:?}");
        }
    }

    #[test]
    fn state_comparison_rejects_a_different_value() {
        let issued = Utc::now();
        assert!(validate_state("abc", "abd", issued, issued).is_err());
        assert!(validate_state("abc", "abc", issued, issued).is_ok());
    }

    #[test]
    fn state_expires() {
        let issued = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let later = Utc.with_ymd_and_hms(2026, 1, 1, 1, 0, 0).unwrap();
        assert!(matches!(
            validate_state("abc", "abc", issued, later),
            Err(AuthError::StateExpired)
        ));
    }

    #[test]
    fn generated_states_are_unique() {
        assert_ne!(generate_state(), generate_state());
        assert_eq!(generate_state().len(), 64);
    }
}
