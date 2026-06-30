use thiserror::Error;

/// Dashboard result type.
pub type Result<T> = std::result::Result<T, DashboardError>;

/// Errors returned by dashboard setup, storage, and routing.
#[derive(Debug, Error)]
pub enum DashboardError {
    /// Dashboard authentication was not configured.
    #[error("dashboard authentication is required")]
    MissingAuth,

    /// Dashboard authentication configuration was invalid.
    #[error("dashboard bearer token must not be empty")]
    InvalidAuth,

    /// Dashboard path was empty or invalid.
    #[error("dashboard path must start with `/` and must not end with `/`")]
    InvalidPath,

    /// Storage failed.
    #[error("dashboard storage error: {0}")]
    Storage(String),

    /// SQLite storage failed.
    #[cfg(feature = "sqlite")]
    #[error("dashboard sqlite error: {0}")]
    Sqlite(#[from] sqlx::Error),

    /// Serialization failed.
    #[error("dashboard serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
