use std::{path::PathBuf, time::Duration};

/// Dashboard authentication configuration.
#[derive(Clone, Debug)]
pub enum DashboardAuth {
    /// Bearer token read from the named environment variable.
    BearerFromEnv(String),
    /// Bearer token supplied directly.
    BearerToken(String),
    /// Explicit local-development override that disables auth.
    UnsafeDisabledForLocalDevelopment,
}

impl DashboardAuth {
    /// Creates bearer auth from an environment variable.
    pub fn bearer_from_env(name: impl Into<String>) -> Self {
        Self::BearerFromEnv(name.into())
    }

    /// Creates bearer auth from a direct token.
    pub fn bearer_token(token: impl Into<String>) -> Self {
        Self::BearerToken(token.into())
    }

    /// Disables auth with an intentionally noisy local-development API.
    pub fn unsafe_disabled_for_local_development() -> Self {
        Self::UnsafeDisabledForLocalDevelopment
    }
}

/// Dashboard storage configuration.
#[derive(Clone, Debug)]
pub enum DashboardStorage {
    /// SQLite database at the provided path or URL.
    Sqlite(String),
    /// SQLite database URL from an environment variable, falling back to a local file.
    SqliteFromEnv(String),
    /// In-memory storage.
    Memory,
}

impl DashboardStorage {
    /// Uses SQLite at a path or URL.
    pub fn sqlite(path: impl Into<String>) -> Self {
        Self::Sqlite(path.into())
    }

    /// Uses SQLite from an environment variable, falling back to `nidus-dashboard.sqlite`.
    pub fn sqlite_from_env(name: impl Into<String>) -> Self {
        Self::SqliteFromEnv(name.into())
    }

    /// Uses in-memory storage.
    pub fn memory() -> Self {
        Self::Memory
    }

    pub(crate) fn resolved_sqlite_path(&self) -> Option<String> {
        match self {
            Self::Sqlite(path) => Some(path.clone()),
            Self::SqliteFromEnv(name) => std::env::var(name)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    Some(
                        PathBuf::from("nidus-dashboard.sqlite")
                            .display()
                            .to_string(),
                    )
                }),
            Self::Memory => None,
        }
    }
}

/// Dashboard capture configuration.
#[derive(Clone, Debug)]
pub struct DashboardCapture {
    capture_payloads: bool,
    max_payload_bytes: usize,
    redacted_headers: Vec<String>,
    redacted_fields: Vec<String>,
}

impl DashboardCapture {
    /// Captures metadata only.
    pub fn metadata_only() -> Self {
        Self {
            capture_payloads: false,
            max_payload_bytes: 0,
            redacted_headers: default_redacted_headers(),
            redacted_fields: default_redacted_fields(),
        }
    }

    /// Enables bounded payload capture.
    pub fn payloads() -> Self {
        Self {
            capture_payloads: true,
            max_payload_bytes: 16 * 1024,
            redacted_headers: default_redacted_headers(),
            redacted_fields: default_redacted_fields(),
        }
    }

    /// Replaces redacted header names.
    pub fn redact_headers<I, S>(mut self, headers: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.redacted_headers = headers.into_iter().map(Into::into).collect();
        self
    }

    /// Replaces redacted field names.
    pub fn redact_fields<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.redacted_fields = fields.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the maximum captured payload size in bytes.
    pub fn max_payload_bytes(mut self, bytes: usize) -> Self {
        self.max_payload_bytes = bytes;
        self
    }

    pub(crate) fn captures_payloads(&self) -> bool {
        self.capture_payloads
    }

    pub(crate) fn payload_byte_cap(&self) -> usize {
        self.max_payload_bytes
    }

    pub(crate) fn redacted_fields(&self) -> &[String] {
        &self.redacted_fields
    }
}

impl Default for DashboardCapture {
    fn default() -> Self {
        Self::metadata_only()
    }
}

fn default_redacted_headers() -> Vec<String> {
    ["authorization", "cookie", "x-api-key"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn default_redacted_fields() -> Vec<String> {
    ["password", "token", "secret"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Dashboard retention configuration.
#[derive(Clone, Copy, Debug)]
pub struct DashboardRetention {
    max_age: Duration,
    max_events: usize,
}

impl DashboardRetention {
    /// Retains records for the provided number of days.
    pub fn days(days: u64) -> Self {
        Self {
            max_age: Duration::from_secs(days.saturating_mul(24 * 60 * 60)),
            max_events: 100_000,
        }
    }

    /// Sets the maximum number of retained events.
    pub fn max_events(mut self, max_events: usize) -> Self {
        self.max_events = max_events;
        self
    }

    /// Returns the max retained event count.
    pub fn max_event_count(&self) -> usize {
        self.max_events
    }

    pub(crate) fn max_age(&self) -> Duration {
        self.max_age
    }
}

impl Default for DashboardRetention {
    fn default() -> Self {
        Self::days(7)
    }
}
