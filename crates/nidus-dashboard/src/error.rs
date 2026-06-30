use thiserror::Error;

/// Dashboard result type.
pub type Result<T> = std::result::Result<T, DashboardError>;

/// Errors returned by dashboard setup, storage, and routing.
#[derive(Debug, Error)]
pub enum DashboardError {
    /// Dashboard authentication was not configured.
    #[error("dashboard authentication is required")]
    MissingAuth,

    /// Dashboard path was empty or invalid.
    #[error("dashboard path must start with `/` and must not end with `/`")]
    InvalidPath,

    /// Storage failed.
    #[error("dashboard storage error: {0}")]
    Storage(String),
}
