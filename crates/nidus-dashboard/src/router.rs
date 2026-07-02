use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
    time::Duration,
};

use axum::{
    Json, Router,
    extract::State,
    middleware,
    response::{Html, IntoResponse, Sse, sse::Event},
    routing::get,
};
use futures_util::{StreamExt, stream};
use serde::Serialize;
use tokio_stream::wrappers::IntervalStream;

#[cfg(feature = "sqlite")]
use crate::storage::SqliteDashboardStorage;
use crate::{
    auth::{DashboardAuthState, require_dashboard_auth},
    collector::DashboardCollector,
    config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage},
    error::{DashboardError, Result},
    storage::{DashboardStorageBackend, DashboardStorageHandle},
    types::{
        DashboardGraphEdge, DashboardGraphEdgeKind, DashboardGraphNode, DashboardGraphNodeKind,
        DashboardGraphResponse, DashboardOperation, DashboardOperationKind,
        DashboardOperationStatus, DashboardRouteSnapshot,
    },
};

const INDEX_HTML: &str = include_str!("../assets/index.html");
const STYLES_CSS: &str = include_str!("../assets/styles.css");
const APP_JS: &str = include_str!("../assets/app.js");
const LOGO_MARK_PNG: &[u8] = include_bytes!("../assets/logo-mark-square-transparent.png");
const FAVICON_BRANDED_32_PNG: &[u8] = include_bytes!("../assets/favicon-branded-32.png");
const FAVICON_BRANDED_192_PNG: &[u8] = include_bytes!("../assets/favicon-branded-192.png");
const APPLE_TOUCH_ICON_PNG: &[u8] = include_bytes!("../assets/apple-touch-icon.png");

/// Embedded Nidus Dashboard.
#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
    auth: DashboardAuthState,
    storage: DashboardStorageHandle,
    collector: DashboardCollector<DashboardStorageHandle>,
    settings: DashboardSettings,
    graph: DashboardGraphState,
}

#[derive(Clone, Debug)]
struct DashboardRuntime {
    storage: DashboardStorageHandle,
    settings: DashboardSettings,
    graph: DashboardGraphState,
}

#[derive(Clone, Debug, Serialize)]
struct DashboardSettings {
    auth_mode: &'static str,
    capture_mode: &'static str,
    storage_mode: &'static str,
    retention_max_events: usize,
}

#[derive(Clone, Debug)]
struct DashboardGraphState {
    snapshot: Arc<RwLock<DashboardGraphResponse>>,
}

impl DashboardGraphState {
    fn new() -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(DashboardGraphResponse::empty("nidus-app"))),
        }
    }

    fn replace(&self, graph: DashboardGraphResponse) {
        let mut snapshot = self
            .snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *snapshot = graph;
    }

    fn get(&self) -> DashboardGraphResponse {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
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
    pub fn collector(&self) -> DashboardCollector<DashboardStorageHandle> {
        self.collector.clone()
    }

    /// Returns the configured dashboard storage.
    pub fn storage(&self) -> DashboardStorageHandle {
        self.storage.clone()
    }

    /// Returns an Axum router for the dashboard.
    pub fn router(&self) -> Router {
        let runtime = self.runtime();
        Router::new()
            .route("/", get(index))
            .route("/assets/styles.css", get(styles))
            .route("/assets/app.js", get(app_js))
            .route("/assets/logo-mark-square-transparent.png", get(logo_mark))
            .route("/assets/favicon-branded-32.png", get(favicon_branded_32))
            .route("/assets/favicon-branded-192.png", get(favicon_branded_192))
            .route("/assets/apple-touch-icon.png", get(apple_touch_icon))
            .route("/api/overview", get(overview))
            .route("/api/graph", get(graph))
            .route("/api/routes", get(routes))
            .route("/api/events", get(events))
            .route("/api/jobs", get(jobs))
            .route("/api/adapters", get(adapters))
            .route("/api/settings", get(settings))
            .route("/api/timeline", get(timeline))
            .route("/stream", get(stream))
            .fallback(index)
            .layer(middleware::from_fn_with_state(
                self.auth.clone(),
                require_dashboard_auth,
            ))
            .with_state(runtime)
    }

    /// Returns an Axum router mounted at the configured dashboard path.
    pub fn mounted_router(&self) -> Router {
        let path = self.path.trim_end_matches('/');
        let runtime = self.runtime();
        Router::new()
            .route(path, get(index))
            .route(&format!("{path}/"), get(index))
            .route(&format!("{path}/assets/styles.css"), get(styles))
            .route(&format!("{path}/assets/app.js"), get(app_js))
            .route(
                &format!("{path}/assets/logo-mark-square-transparent.png"),
                get(logo_mark),
            )
            .route(
                &format!("{path}/assets/favicon-branded-32.png"),
                get(favicon_branded_32),
            )
            .route(
                &format!("{path}/assets/favicon-branded-192.png"),
                get(favicon_branded_192),
            )
            .route(
                &format!("{path}/assets/apple-touch-icon.png"),
                get(apple_touch_icon),
            )
            .route(&format!("{path}/api/overview"), get(overview))
            .route(&format!("{path}/api/graph"), get(graph))
            .route(&format!("{path}/api/routes"), get(routes))
            .route(&format!("{path}/api/events"), get(events))
            .route(&format!("{path}/api/jobs"), get(jobs))
            .route(&format!("{path}/api/adapters"), get(adapters))
            .route(&format!("{path}/api/settings"), get(settings))
            .route(&format!("{path}/api/timeline"), get(timeline))
            .route(&format!("{path}/stream"), get(stream))
            .layer(middleware::from_fn_with_state(
                self.auth.clone(),
                require_dashboard_auth,
            ))
            .with_state(runtime)
    }

    /// Records a route snapshot for dashboard route introspection.
    pub async fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> Result<()> {
        self.storage.record_route_snapshot(route).await
    }

    /// Records the current runtime module graph for dashboard topology.
    pub fn record_graph_snapshot(&self, graph: DashboardGraphResponse) {
        self.graph.replace(graph);
    }

    fn runtime(&self) -> DashboardRuntime {
        DashboardRuntime {
            storage: self.storage.clone(),
            settings: self.settings.clone(),
            graph: self.graph.clone(),
        }
    }
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn styles() -> impl IntoResponse {
    (
        [(http::header::CONTENT_TYPE, "text/css; charset=utf-8")],
        STYLES_CSS,
    )
}

async fn app_js() -> impl IntoResponse {
    (
        [(http::header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        APP_JS,
    )
}

async fn logo_mark() -> impl IntoResponse {
    png(LOGO_MARK_PNG)
}

async fn favicon_branded_32() -> impl IntoResponse {
    png(FAVICON_BRANDED_32_PNG)
}

async fn favicon_branded_192() -> impl IntoResponse {
    png(FAVICON_BRANDED_192_PNG)
}

async fn apple_touch_icon() -> impl IntoResponse {
    png(APPLE_TOUCH_ICON_PNG)
}

fn png(bytes: &'static [u8]) -> impl IntoResponse {
    ([(http::header::CONTENT_TYPE, "image/png")], bytes)
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

async fn overview(State(runtime): State<DashboardRuntime>) -> Json<OverviewResponse> {
    let operations = runtime
        .storage
        .list_operations(1_000)
        .await
        .unwrap_or_default();
    let routes = runtime
        .storage
        .list_route_snapshots()
        .await
        .unwrap_or_default();
    let event_count = operations
        .iter()
        .filter(|operation| operation.kind == DashboardOperationKind::Event)
        .count();
    let job_count = operations
        .iter()
        .filter(|operation| operation.kind == DashboardOperationKind::Job)
        .count();

    Json(OverviewResponse {
        service_name: "nidus-app",
        metrics: vec![
            OverviewMetric {
                label: "Routes",
                value: routes.len().to_string(),
            },
            OverviewMetric {
                label: "Events",
                value: event_count.to_string(),
            },
            OverviewMetric {
                label: "Jobs",
                value: job_count.to_string(),
            },
        ],
    })
}

async fn graph(State(runtime): State<DashboardRuntime>) -> Json<DashboardGraphResponse> {
    let mut response = runtime.graph.get();
    response.refresh_timestamp();

    let mut known_nodes = response
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect::<BTreeSet<_>>();
    let mut known_edges = response
        .edges
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<BTreeSet<_>>();

    if response.nodes.is_empty() {
        let runtime_id = runtime_node_id(&response.service_name);
        known_nodes.insert(runtime_id.clone());
        response.nodes.push(DashboardGraphNode {
            id: runtime_id,
            kind: DashboardGraphNodeKind::Runtime,
            label: response.service_name.clone(),
            summary: Some("dashboard runtime".to_owned()),
            status: None,
            counts: BTreeMap::new(),
            metadata: BTreeMap::new(),
        });
    }

    let runtime_id = response
        .nodes
        .iter()
        .find(|node| node.kind == DashboardGraphNodeKind::Runtime)
        .map(|node| node.id.clone())
        .unwrap_or_else(|| runtime_node_id(&response.service_name));

    let operations = runtime
        .storage
        .list_operations(60)
        .await
        .unwrap_or_default();
    for operation in operations {
        let Some(kind) = graph_node_kind_for_operation(&operation.kind) else {
            continue;
        };
        let node_id = format!("operation:{}", operation.id);
        if known_nodes.insert(node_id.clone()) {
            let mut counts = BTreeMap::new();
            if let Some(duration) = operation.duration_ms {
                counts.insert("duration_ms".to_owned(), duration as usize);
            }
            let mut metadata = BTreeMap::new();
            metadata.insert(
                "operation".to_owned(),
                serde_json::to_value(&operation).unwrap_or(serde_json::Value::Null),
            );
            response.nodes.push(DashboardGraphNode {
                id: node_id.clone(),
                kind,
                label: operation.name.clone(),
                summary: Some(format!(
                    "{} / {}",
                    operation_kind_label(&operation.kind),
                    operation_status_label(&operation.status)
                )),
                status: Some(operation_status_label(&operation.status).to_owned()),
                counts,
                metadata,
            });
        }

        let target =
            activity_target(&operation, &known_nodes).unwrap_or_else(|| runtime_id.clone());
        let edge_id = format!("activity:{}:{node_id}", target);
        if known_edges.insert(edge_id.clone()) {
            response.edges.push(DashboardGraphEdge {
                id: edge_id,
                kind: DashboardGraphEdgeKind::RuntimeActivity,
                source: target,
                target: node_id,
                label: Some(operation_kind_label(&operation.kind).to_owned()),
            });
        }
    }

    Json(response)
}

async fn routes(State(runtime): State<DashboardRuntime>) -> Json<Vec<DashboardRouteSnapshot>> {
    Json(
        runtime
            .storage
            .list_route_snapshots()
            .await
            .unwrap_or_default(),
    )
}

async fn events(State(runtime): State<DashboardRuntime>) -> Json<Vec<DashboardOperation>> {
    Json(operations_by_kind(runtime, DashboardOperationKind::Event).await)
}

async fn jobs(State(runtime): State<DashboardRuntime>) -> Json<Vec<DashboardOperation>> {
    Json(operations_by_kind(runtime, DashboardOperationKind::Job).await)
}

async fn adapters(State(runtime): State<DashboardRuntime>) -> Json<Vec<DashboardOperation>> {
    Json(operations_by_kind(runtime, DashboardOperationKind::Adapter).await)
}

async fn settings(State(runtime): State<DashboardRuntime>) -> Json<DashboardSettings> {
    Json(runtime.settings)
}

async fn timeline(State(runtime): State<DashboardRuntime>) -> Json<Vec<DashboardOperation>> {
    Json(
        runtime
            .storage
            .list_operations(100)
            .await
            .unwrap_or_default(),
    )
}

async fn stream()
-> Sse<impl futures_util::Stream<Item = std::result::Result<Event, std::convert::Infallible>>> {
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
    let heartbeat = IntervalStream::new(tokio::time::interval(Duration::from_secs(15)))
        .map(|_| Ok(Event::default().comment("heartbeat")));
    let stream = stream::once(async move { Ok(Event::default().data(data)) }).chain(heartbeat);
    Sse::new(stream)
}

async fn operations_by_kind(
    runtime: DashboardRuntime,
    kind: DashboardOperationKind,
) -> Vec<DashboardOperation> {
    runtime
        .storage
        .list_operations(100)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|operation| operation.kind == kind)
        .collect()
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
        let storage_config = self.storage;
        let capture = self.capture;
        let retention = self.retention;
        let _ = retention.max_age();
        let storage = storage_from_config(storage_config)?;
        let settings = DashboardSettings {
            auth_mode: auth.mode_name(),
            capture_mode: capture.mode_name(),
            storage_mode: storage.mode_name(),
            retention_max_events: retention.max_event_count(),
        };
        let collector = DashboardCollector::with_retention(storage.clone(), capture, retention);
        Ok(NidusDashboard {
            path: self.path,
            auth,
            storage,
            collector,
            settings,
            graph: DashboardGraphState::new(),
        })
    }
}

fn runtime_node_id(service_name: &str) -> String {
    format!("runtime:{service_name}")
}

fn graph_node_kind_for_operation(kind: &DashboardOperationKind) -> Option<DashboardGraphNodeKind> {
    match kind {
        DashboardOperationKind::Event => Some(DashboardGraphNodeKind::Event),
        DashboardOperationKind::Job => Some(DashboardGraphNodeKind::Job),
        DashboardOperationKind::Adapter => Some(DashboardGraphNodeKind::Adapter),
        DashboardOperationKind::Http | DashboardOperationKind::Lifecycle => None,
    }
}

fn operation_kind_label(kind: &DashboardOperationKind) -> &'static str {
    match kind {
        DashboardOperationKind::Http => "http",
        DashboardOperationKind::Event => "event",
        DashboardOperationKind::Job => "job",
        DashboardOperationKind::Lifecycle => "lifecycle",
        DashboardOperationKind::Adapter => "adapter",
    }
}

fn operation_status_label(status: &DashboardOperationStatus) -> &'static str {
    match status {
        DashboardOperationStatus::Success => "success",
        DashboardOperationStatus::Failure => "failure",
        DashboardOperationStatus::Running => "running",
    }
}

fn activity_target(
    operation: &DashboardOperation,
    known_nodes: &BTreeSet<String>,
) -> Option<String> {
    for key in ["route", "controller", "module"] {
        let Some(value) = operation.attributes.get(key) else {
            continue;
        };
        if key == "route" {
            let direct = format!("route:{value}");
            if known_nodes.contains(&direct) {
                return Some(direct);
            }
        }
        let prefix = format!("{key}:");
        if let Some(found) = known_nodes
            .iter()
            .find(|candidate| candidate.starts_with(&prefix) && candidate.ends_with(value))
        {
            return Some(found.clone());
        }
    }
    None
}

fn storage_from_config(storage: DashboardStorage) -> Result<DashboardStorageHandle> {
    match storage {
        DashboardStorage::Memory => Ok(DashboardStorageHandle::memory()),
        #[cfg(feature = "sqlite")]
        DashboardStorage::Sqlite(path) => Ok(DashboardStorageHandle::Sqlite(
            SqliteDashboardStorage::connect_lazy(&path)?,
        )),
        #[cfg(feature = "sqlite")]
        DashboardStorage::SqliteFromEnv(name) => {
            let path = DashboardStorage::sqlite_from_env(name)
                .resolved_sqlite_path()
                .unwrap_or_else(|| "nidus-dashboard.sqlite".to_owned());
            Ok(DashboardStorageHandle::Sqlite(
                SqliteDashboardStorage::connect_lazy(&path)?,
            ))
        }
        #[cfg(not(feature = "sqlite"))]
        DashboardStorage::Sqlite(_) | DashboardStorage::SqliteFromEnv(_) => Err(
            DashboardError::Storage("sqlite storage requires the `sqlite` feature".to_owned()),
        ),
    }
}
