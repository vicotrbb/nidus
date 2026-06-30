use axum::{
    Json, Router, middleware,
    response::{Html, IntoResponse, Sse, sse::Event},
    routing::get,
};
use serde::Serialize;

use crate::{
    auth::{DashboardAuthState, require_dashboard_auth},
    collector::DashboardCollector,
    config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage},
    error::{DashboardError, Result},
    storage::MemoryDashboardStorage,
    types::{DashboardOperation, DashboardOperationKind, DashboardOperationStatus},
};

const INDEX_HTML: &str = include_str!("../assets/index.html");
const STYLES_CSS: &str = include_str!("../assets/styles.css");
const APP_JS: &str = include_str!("../assets/app.js");

/// Embedded Nidus Dashboard.
#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
    auth: DashboardAuthState,
    storage: MemoryDashboardStorage,
    collector: DashboardCollector<MemoryDashboardStorage>,
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

    /// Returns the dashboard collector.
    pub fn collector(&self) -> DashboardCollector<MemoryDashboardStorage> {
        self.collector.clone()
    }

    /// Returns the configured dashboard storage.
    pub fn storage(&self) -> MemoryDashboardStorage {
        self.storage.clone()
    }

    /// Returns an Axum router for the dashboard.
    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(index))
            .route("/assets/styles.css", get(styles))
            .route("/assets/app.js", get(app_js))
            .route("/api/overview", get(overview))
            .route("/api/timeline", get(timeline))
            .route("/stream", get(stream))
            .fallback(index)
            .layer(middleware::from_fn_with_state(
                self.auth.clone(),
                require_dashboard_auth,
            ))
    }

    /// Returns an Axum router mounted at the configured dashboard path.
    pub fn mounted_router(&self) -> Router {
        let path = self.path.trim_end_matches('/');
        Router::new()
            .route(path, get(index))
            .route(&format!("{path}/"), get(index))
            .route(&format!("{path}/assets/styles.css"), get(styles))
            .route(&format!("{path}/assets/app.js"), get(app_js))
            .route(&format!("{path}/api/overview"), get(overview))
            .route(&format!("{path}/api/timeline"), get(timeline))
            .route(&format!("{path}/stream"), get(stream))
            .layer(middleware::from_fn_with_state(
                self.auth.clone(),
                require_dashboard_auth,
            ))
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn styles() -> impl IntoResponse {
    ([(http::header::CONTENT_TYPE, "text/css; charset=utf-8")], STYLES_CSS)
}

async fn app_js() -> impl IntoResponse {
    (
        [(http::header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
}

#[derive(Serialize)]
struct OverviewResponse {
    service_name: &'static str,
    metrics: Vec<OverviewMetric>,
}

#[derive(Serialize)]
struct OverviewMetric {
    label: &'static str,
    value: String,
}

async fn overview() -> Json<OverviewResponse> {
    Json(OverviewResponse {
        service_name: "nidus-app",
        metrics: vec![
            OverviewMetric {
                label: "Requests",
                value: "0".to_owned(),
            },
            OverviewMetric {
                label: "Errors",
                value: "0".to_owned(),
            },
            OverviewMetric {
                label: "Events",
                value: "0".to_owned(),
            },
        ],
    })
}

async fn timeline() -> Json<Vec<DashboardOperation>> {
    Json(Vec::new())
}

async fn stream() -> Sse<impl futures_util::Stream<Item = std::result::Result<Event, std::convert::Infallible>>> {
    let event = DashboardOperation {
        id: uuid::Uuid::new_v4().to_string(),
        kind: DashboardOperationKind::Lifecycle,
        name: "dashboard.connected".to_owned(),
        status: DashboardOperationStatus::Success,
        timestamp_ms: time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000,
        duration_ms: None,
        correlation_id: None,
        attributes: Default::default(),
        payload: None,
    };
    let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_owned());
    let stream = tokio_stream::once(Ok(Event::default().data(data)));
    Sse::new(stream)
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
        let storage = MemoryDashboardStorage::new();
        let collector = DashboardCollector::new(storage.clone());
        Ok(NidusDashboard {
            path: self.path,
            auth,
            storage,
            collector,
        })
    }
}
