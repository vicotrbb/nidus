use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Dashboard operation kind.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardOperationKind {
    /// HTTP request or response.
    Http,
    /// Observed event publication.
    Event,
    /// Observed job run.
    Job,
    /// Application lifecycle operation.
    Lifecycle,
    /// Official adapter operation.
    Adapter,
}

/// Dashboard operation status.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DashboardOperationStatus {
    /// Operation succeeded.
    Success,
    /// Operation failed.
    Failure,
    /// Operation is in progress.
    Running,
}

/// Unified dashboard timeline operation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DashboardOperation {
    /// Stable operation identifier.
    pub id: String,
    /// Operation kind.
    pub kind: DashboardOperationKind,
    /// Stable operation name.
    pub name: String,
    /// Operation status.
    pub status: DashboardOperationStatus,
    /// Unix timestamp in milliseconds.
    pub timestamp_ms: i64,
    /// Duration in milliseconds, when known.
    pub duration_ms: Option<u64>,
    /// Request id, trace id, or run id when available.
    pub correlation_id: Option<String>,
    /// Stable metadata attributes.
    pub attributes: BTreeMap<String, String>,
    /// Optional redacted payload.
    pub payload: Option<serde_json::Value>,
}

/// Dashboard route snapshot record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DashboardRouteSnapshot {
    /// HTTP method.
    pub method: String,
    /// Full route path.
    pub path: String,
    /// Handler or summary label.
    pub summary: Option<String>,
    /// Guard type names.
    pub guards: Vec<String>,
    /// Pipe type names.
    pub pipes: Vec<String>,
    /// Whether validation is enabled.
    pub validates: bool,
}
