use anyhow::Result;

/// HTTP listener settings for the `gateway` binary.
#[derive(Debug, Clone)]
pub struct AppServer {
    pub host: String,
    pub port: u16,
    /// Origins allowed to call the API from a browser.
    ///
    /// Empty means "development default": localhost on any port, which Vite picks
    /// freely. In production this must be set explicitly.
    pub cors_origins: Vec<String>,
}

impl AppServer {
    pub fn from_env() -> Result<Self> {
        let cors_origins = crate::optional("CORS_ORIGINS", "")
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(str::to_string)
            .collect();

        Ok(Self {
            host: crate::optional("APP_HOST", "0.0.0.0"),
            port: crate::parse("APP_PORT", "8080")?,
            cors_origins,
        })
    }

    /// The address the server binds to.
    pub fn bind_address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
