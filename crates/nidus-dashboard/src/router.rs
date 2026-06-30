use axum::{Router, middleware, routing::get};

use crate::{
    auth::{DashboardAuthState, require_dashboard_auth},
    config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage},
    error::{DashboardError, Result},
};

/// Embedded Nidus Dashboard.
#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
    auth: DashboardAuthState,
}

impl NidusDashboard {
    /// Creates a dashboard builder.
    pub fn builder() -> NidusDashboardBuilder {
        NidusDashboardBuilder::default()
    }

    /// Returns the configured dashboard path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns an Axum router for the dashboard.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(|| async { "Nidus Dashboard" }))
            .layer(middleware::from_fn_with_state(
                self.auth.clone(),
                require_dashboard_auth,
            ))
    }
}

/// Dashboard builder.
#[derive(Clone, Debug)]
pub struct NidusDashboardBuilder {
    path: String,
    auth: Option<DashboardAuth>,
    storage: DashboardStorage,
    capture: DashboardCapture,
    retention: DashboardRetention,
}

impl Default for NidusDashboardBuilder {
    fn default() -> Self {
        Self {
            path: "/nidus/dashboard".to_owned(),
            auth: None,
            storage: DashboardStorage::sqlite_from_env("NIDUS_DASHBOARD_DATABASE_URL"),
            capture: DashboardCapture::metadata_only(),
            retention: DashboardRetention::default(),
        }
    }
}

impl NidusDashboardBuilder {
    /// Sets the dashboard mount path.
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    /// Sets dashboard authentication.
    pub fn auth(mut self, auth: DashboardAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Sets dashboard storage.
    pub fn storage(mut self, storage: DashboardStorage) -> Self {
        self.storage = storage;
        self
    }

    /// Sets dashboard capture behavior.
    pub fn capture(mut self, capture: DashboardCapture) -> Self {
        self.capture = capture;
        self
    }

    /// Sets dashboard retention behavior.
    pub fn retention(mut self, retention: DashboardRetention) -> Self {
        self.retention = retention;
        self
    }

    /// Builds the dashboard.
    pub fn build(self) -> Result<NidusDashboard> {
        let Some(auth) = self.auth else {
            return Err(DashboardError::MissingAuth);
        };
        if !self.path.starts_with('/') || self.path.ends_with('/') {
            return Err(DashboardError::InvalidPath);
        }
        let auth = DashboardAuthState::from_config(auth)?;
        let _ = self.storage.resolved_sqlite_path();
        let _ = self.capture.captures_payloads();
        let _ = self.capture.payload_byte_cap();
        let _ = self.retention.max_age();
        let _ = self.retention.max_event_count();
        Ok(NidusDashboard {
            path: self.path,
            auth,
        })
    }
}
