use anyhow::Result;

/// Which deployment this process is running in.
///
/// Kept as an enum rather than a raw string so the "is this production?" question
/// has one answer, checked in one place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Staging,
    Production,
}

impl Environment {
    pub fn from_env() -> Result<Self> {
        let raw = crate::optional("APP_ENV", "development");

        match raw.to_ascii_lowercase().as_str() {
            "development" | "dev" | "local" => Ok(Self::Development),
            "staging" => Ok(Self::Staging),
            "production" | "prod" => Ok(Self::Production),
            other => Err(anyhow::anyhow!(
                "`APP_ENV` must be one of development|staging|production (got `{other}`)"
            )),
        }
    }

    pub fn is_production(self) -> bool {
        matches!(self, Self::Production)
    }

    /// True when running locally: enables permissive CORS and pretty logs.
    pub fn is_development(self) -> bool {
        matches!(self, Self::Development)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Staging => "staging",
            Self::Production => "production",
        }
    }
}
