use anyhow::{Context, Result};

/// Signing and lifetime settings for the JWT pair the API issues.
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// HMAC secret. Never logged, never serialized into a response.
    secret: Vec<u8>,
    pub access_token_ttl_secs: i64,
    pub refresh_token_ttl_secs: i64,
    /// Where the dashboard lives; the OAuth callback redirects back here.
    pub web_origin: String,
}

impl AuthConfig {
    pub fn from_env() -> Result<Self> {
        let secret = crate::required("JWT_SECRET")?;

        // A short HMAC key is a real weakness, not a style preference. 32 bytes is
        // the output size of the hash, so anything shorter adds no strength.
        if secret.len() < 32 {
            anyhow::bail!(
                "`JWT_SECRET` must be at least 32 bytes (got {}). Generate one with `openssl rand -hex 32`",
                secret.len()
            );
        }

        Ok(Self {
            secret: secret.into_bytes(),
            access_token_ttl_secs: crate::parse("JWT_ACCESS_TTL", "900")?,
            refresh_token_ttl_secs: crate::parse("JWT_REFRESH_TTL", "2592000")?,
            web_origin: crate::optional("WEB_ORIGIN", "http://localhost:5173"),
        })
    }

    pub fn secret(&self) -> &[u8] {
        &self.secret
    }

    /// The OAuth redirect target, validated so a typo cannot silently register a
    /// callback the browser will refuse to follow.
    pub fn web_callback_url(&self) -> Result<String> {
        let base = self.web_origin.trim_end_matches('/').to_string();
        if !base.starts_with("http") {
            anyhow::bail!(
                "`WEB_ORIGIN` must be an absolute http(s) URL (got `{}`)",
                self.web_origin
            );
        }
        Ok(format!("{base}/auth/callback"))
    }
}

/// Discord OAuth2 application credentials.
///
/// The client secret is `Option` because Discord only ever shows it once, in the
/// Developer Portal, and it cannot be read back over the API — so an installation
/// may legitimately not have it. Refusing to start over it would take the whole
/// API down, including the routes that have nothing to do with logging in, to
/// disable one flow. A missing secret disables dashboard login and says so; see
/// [`DiscordOAuthConfig::is_configured`].
#[derive(Debug, Clone)]
pub struct DiscordOAuthConfig {
    pub client_id: String,
    client_secret: Option<String>,
    pub redirect_uri: String,
}

impl DiscordOAuthConfig {
    pub fn from_env(web_origin: &str) -> Result<Self> {
        let client_secret = crate::optional("DISCORD_CLIENT_SECRET", "");
        let redirect_uri = crate::optional(
            "DISCORD_REDIRECT_URI",
            &format!(
                "{}/api/v1/iam/auth/discord/callback",
                web_origin.trim_end_matches('/')
            ),
        );

        Ok(Self {
            client_id: crate::required("DISCORD_CLIENT_ID")?,
            // An empty value is a `.env` key someone left blank, which means
            // "not configured" rather than "the secret is the empty string".
            client_secret: Some(client_secret).filter(|secret| !secret.trim().is_empty()),
            redirect_uri,
        })
    }

    /// The secret, or `None` when this installation has none.
    pub fn client_secret(&self) -> Option<&str> {
        self.client_secret.as_deref()
    }

    /// Whether the consent-code exchange can run at all.
    ///
    /// The consent screen itself needs only the client id and the redirect URI, so
    /// a half-configured installation can still build a URL that will fail at the
    /// exchange. Handlers check this *before* redirecting the browser, so the user
    /// gets an immediate, accurate refusal instead of a Discord error page.
    pub fn is_configured(&self) -> bool {
        self.client_secret.is_some()
    }

    pub fn token_endpoint(&self) -> &'static str {
        "https://discord.com/api/oauth2/token"
    }

    pub fn authorize_endpoint(&self) -> &'static str {
        "https://discord.com/oauth2/authorize"
    }

    /// Builds the consent screen URL the dashboard redirects to.
    pub fn authorize_url(&self, state: &str) -> String {
        let scope = "identify%20guilds";
        format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
            self.authorize_endpoint(),
            self.client_id,
            urlencode(&self.redirect_uri),
            scope,
            state,
        )
    }
}

/// Minimal percent-encoding for the few values we place in a query string.
///
/// Avoids pulling a URL crate in for one function; the inputs are a configured
/// redirect URI and a signed state token, never user-controlled text.
fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

/// Both halves of the auth configuration, kept together so a caller cannot wire
/// one without the other.
#[derive(Debug, Clone)]
pub struct AuthStack {
    pub jwt: AuthConfig,
    pub oauth: DiscordOAuthConfig,
}

impl AuthConfig {
    pub fn with_discord(self) -> Result<AuthStack> {
        let oauth = DiscordOAuthConfig::from_env(&self.web_origin)
            .context("discord oauth configuration")?;
        Ok(AuthStack { jwt: self, oauth })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(urlencode("http://a/b c"), "http%3A%2F%2Fa%2Fb%20c");
    }

    #[test]
    fn leaves_unreserved_characters_alone() {
        assert_eq!(urlencode("aZ0-_.~"), "aZ0-_.~");
    }

    /// Built directly rather than through `from_env`: `unsafe_code` is forbidden,
    /// so a test cannot call `set_var`, and the fields are crate-private precisely
    /// so the constructor's decisions can be checked without an environment.
    fn with_secret(secret: Option<&str>) -> DiscordOAuthConfig {
        DiscordOAuthConfig {
            client_id: "1".to_string(),
            client_secret: secret.map(str::to_string),
            redirect_uri: "https://example.test/cb".to_string(),
        }
    }

    /// Discord shows the client secret once and never returns it over the API, so
    /// an installation can genuinely lack one. That must read as "login is off",
    /// not as a crash and not as "the secret is the empty string" — the latter
    /// would post an empty credential and get back `invalid_client`, which reads
    /// as a wrong password rather than a missing setup.
    #[test]
    fn a_missing_client_secret_disables_login_rather_than_faking_one() {
        assert!(!with_secret(None).is_configured());
        assert_eq!(with_secret(None).client_secret(), None);
    }

    #[test]
    fn a_present_client_secret_enables_login() {
        let config = with_secret(Some("a-real-secret"));
        assert!(config.is_configured());
        assert_eq!(config.client_secret(), Some("a-real-secret"));
    }
}
