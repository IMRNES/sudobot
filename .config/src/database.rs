use anyhow::{Context, Result};

/// PostgreSQL connection settings.
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    /// Seconds to wait for a free connection before failing a request.
    pub acquire_timeout_secs: u64,
    /// Seconds a pooled connection may sit idle before being closed.
    pub idle_timeout_secs: u64,
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self> {
        let url = crate::required("DATABASE_URL")?;

        // Fail here, not on the first request. A typo in the scheme produces a
        // clearer message at startup than a pool that never resolves.
        let scheme_is_valid = url.starts_with("postgres://") || url.starts_with("postgresql://");
        if !scheme_is_valid {
            anyhow::bail!(
                "`DATABASE_URL` must start with postgres:// or postgresql:// (got `{url}`)"
            );
        }

        let max_connections: u32 = crate::parse("DATABASE_MAX_CONNECTIONS", "10")?;
        if max_connections == 0 {
            anyhow::bail!("`DATABASE_MAX_CONNECTIONS` must be at least 1");
        }

        Ok(Self {
            url,
            max_connections,
            min_connections: crate::parse("DATABASE_MIN_CONNECTIONS", "1")?,
            acquire_timeout_secs: crate::parse("DATABASE_ACQUIRE_TIMEOUT", "8")?,
            idle_timeout_secs: crate::parse("DATABASE_IDLE_TIMEOUT", "300")?,
        })
    }

    /// The same URL with the password replaced, safe to write to a log line.
    pub fn redacted_url(&self) -> String {
        redact_password(&self.url)
            .context("DATABASE_URL is not a valid URL")
            .unwrap_or_else(|_| "<unparseable>".to_string())
    }
}

/// Strips the password from a `postgres://user:pass@host/db` style URL.
fn redact_password(url: &str) -> Result<String> {
    let (scheme, rest) = url.split_once("://").context("missing scheme separator")?;
    let (credentials, host) = match rest.rsplit_once('@') {
        Some((credentials, host)) => (Some(credentials), host),
        None => (None, rest),
    };

    match credentials {
        Some(credentials) => {
            let user = credentials.split(':').next().unwrap_or(credentials);
            Ok(format!("{scheme}://{user}:***@{host}"))
        }
        None => Ok(url.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_only_the_password() {
        let redacted = redact_password("postgres://sudobot:s3cret@localhost:5432/sudobot").unwrap();
        assert_eq!(redacted, "postgres://sudobot:***@localhost:5432/sudobot");
        assert!(!redacted.contains("s3cret"));
    }

    #[test]
    fn tolerates_urls_without_credentials() {
        let redacted = redact_password("postgres://localhost/sudobot").unwrap();
        assert_eq!(redacted, "postgres://localhost/sudobot");
    }
}
