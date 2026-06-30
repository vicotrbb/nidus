# Nidus Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the opt-in embedded `nidus-dashboard` crate with protected UI, JSON APIs, SSE stream, SQLite default storage, metadata-first capture, facade integration, docs, and verification.

**Architecture:** Add a standalone `crates/nidus-dashboard` crate that depends on `nidus-http`, `nidus-events`, and `nidus-jobs`, but not on the `nidus-rs` facade. The crate owns dashboard config, auth, storage, capture hooks, embedded assets, JSON APIs, and an Axum router; the facade integrates it behind a `dashboard` feature with `NidusApplicationBuilder::with_dashboard`.

**Tech Stack:** Rust 2024, Axum 0.8, Tower 0.5, Tokio, SQLx SQLite runtime queries, serde/serde_json, SSE via Axum response streaming, embedded HTML/CSS/JS assets using the current Nidus website style tokens.

---

## File Structure

Create:

- `crates/nidus-dashboard/Cargo.toml` - crate metadata, features, dependencies.
- `crates/nidus-dashboard/src/lib.rs` - public exports and crate docs.
- `crates/nidus-dashboard/src/error.rs` - `DashboardError` and `Result`.
- `crates/nidus-dashboard/src/config.rs` - builder, auth, storage, capture, retention config.
- `crates/nidus-dashboard/src/types.rs` - serializable dashboard DTOs and timeline domain types.
- `crates/nidus-dashboard/src/storage/mod.rs` - storage trait and shared helpers.
- `crates/nidus-dashboard/src/storage/memory.rs` - in-memory storage for tests and ephemeral mode.
- `crates/nidus-dashboard/src/storage/sqlite.rs` - SQLite migrations, writes, reads, retention pruning.
- `crates/nidus-dashboard/src/auth.rs` - bearer-token and unsafe-development auth layer.
- `crates/nidus-dashboard/src/collector.rs` - non-blocking capture channel and hook adapters.
- `crates/nidus-dashboard/src/router.rs` - embedded Axum router, API routes, asset routes, SSE.
- `crates/nidus-dashboard/assets/index.html` - dashboard shell.
- `crates/nidus-dashboard/assets/styles.css` - `$impeccable` website-aligned dashboard styles.
- `crates/nidus-dashboard/assets/app.js` - dashboard client, fetch helpers, SSE, filters.
- `crates/nidus-dashboard/tests/auth.rs` - auth and fail-closed integration tests.
- `crates/nidus-dashboard/tests/storage.rs` - memory and SQLite storage tests.
- `crates/nidus-dashboard/tests/router.rs` - route mounting, APIs, assets, SSE tests.
- `crates/nidus-dashboard/tests/capture.rs` - metadata-only, redaction, and hook capture tests.
- `crates/nidus-dashboard/tests/ui_assets.rs` - static asset smoke tests.
- `examples/dashboard-api/Cargo.toml` - copyable example app.
- `examples/dashboard-api/src/main.rs` - embedded dashboard example.
- `examples/dashboard-api/README.md` - run and curl instructions.
- `docs/dashboard.md` - user docs.

Modify:

- `Cargo.toml` - add workspace member and dependencies.
- `crates/nidus/Cargo.toml` - add optional `nidus-dashboard` dependency and `dashboard` feature.
- `crates/nidus/src/lib.rs` - re-export dashboard module behind the feature.
- `crates/nidus/src/prelude.rs` - re-export dashboard setup types behind the feature.
- `crates/nidus/src/app.rs` - add `with_dashboard` builder integration.
- `docs/README.md` - add Dashboard doc link.
- `README.md` - mention optional Nidus Dashboard.
- `docs/api-reference.md` - add docs.rs row after publication.
- `docs/examples.md` - add dashboard example row.

## Implementation Decisions Locked By This Plan

- v1 assets are plain embedded HTML/CSS/JS, not Vite or React. This avoids adding a new frontend build stack while still allowing a polished `$impeccable` UI.
- v1 live updates use SSE, not WebSockets.
- `NidusDashboard::build()` fails closed when auth is missing.
- SQLite is the default configured storage, but tests use memory by default.
- `DashboardStorage::sqlite_from_env` resolves to SQLite only when the env var is present; if absent it uses `nidus-dashboard.sqlite`.
- Payload capture is off by default; no request/response/event payload fields are stored unless `DashboardCapture::payloads()` is used.
- Route and graph snapshots in v1 come from facade builder metadata when using `with_dashboard`. Direct Axum users can call `dashboard.record_route_snapshot` manually later; direct Axum v1 still has timeline/API/storage/UI support.

## Task 1: Workspace And Crate Scaffold

**Files:**
- Modify: `Cargo.toml`
- Create: `crates/nidus-dashboard/Cargo.toml`
- Create: `crates/nidus-dashboard/src/lib.rs`
- Create: `crates/nidus-dashboard/src/error.rs`
- Create: `crates/nidus-dashboard/src/config.rs`
- Create: `crates/nidus-dashboard/src/types.rs`
- Create: `crates/nidus-dashboard/tests/builder.rs`

- [ ] **Step 1: Add a failing builder test**

Create `crates/nidus-dashboard/tests/builder.rs`:

```rust
use nidus_dashboard::{
    DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage, NidusDashboard,
};

#[test]
fn builder_fails_closed_without_auth() {
    let error = NidusDashboard::builder()
        .storage(DashboardStorage::memory())
        .build()
        .expect_err("dashboard must require auth by default");

    assert!(
        error
            .to_string()
            .contains("dashboard authentication is required"),
        "{error}"
    );
}

#[test]
fn builder_accepts_auth_storage_capture_and_retention() {
    let dashboard = NidusDashboard::builder()
        .path("/nidus/dashboard")
        .auth(DashboardAuth::bearer_token("dev-token"))
        .storage(DashboardStorage::memory())
        .capture(DashboardCapture::metadata_only())
        .retention(DashboardRetention::days(7).max_events(100_000))
        .build()
        .expect("authenticated dashboard should build");

    assert_eq!(dashboard.path(), "/nidus/dashboard");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test -p nidus-dashboard --test builder
```

Expected: fail because the crate does not exist.

- [ ] **Step 3: Add workspace member and dependencies**

Modify root `Cargo.toml`:

```toml
[workspace]
members = [
  ".",
  "crates/nidus",
  "crates/nidus-core",
  "crates/nidus-http",
  "crates/nidus-observability",
  "crates/nidus-dashboard",
  "crates/nidus-macros",
  "crates/nidus-config",
  "crates/nidus-openapi",
  "crates/nidus-validation",
  "crates/nidus-auth",
  "crates/nidus-events",
  "crates/nidus-jobs",
  "crates/nidus-sqlx",
  "crates/nidus-cache",
  "crates/nidus-testing",
  "crates/cargo-nidus",
  "examples/hello-world",
  "examples/rest-api",
  "examples/auth-api",
  "examples/openapi",
  "examples/background-jobs",
  "examples/modular-monolith",
  "examples/realworld-api",
  "examples/production-api",
  "examples/sqlx-app",
  "examples/cache-app",
  "examples/integrations-production",
  "examples/launchpad-api",
]
resolver = "2"
```

Add workspace dependencies if absent:

```toml
async-stream = "0.3"
bytes = "1"
tokio-stream = "0.1"
```

Update the existing `sqlx` workspace dependency so SQLite is available to the dashboard crate:

```toml
sqlx = { version = "0.8", default-features = false, features = ["runtime-tokio", "sqlite"] }
```

- [ ] **Step 4: Create crate manifest**

Create `crates/nidus-dashboard/Cargo.toml`:

```toml
[package]
name = "nidus-dashboard"
version.workspace = true
edition.workspace = true
description = "Embedded dashboard for Nidus applications."
documentation = "https://docs.rs/nidus-dashboard"
license.workspace = true
repository.workspace = true
homepage.workspace = true
rust-version.workspace = true
keywords.workspace = true
categories.workspace = true

[features]
default = ["sqlite", "embedded-assets"]
sqlite = ["dep:sqlx"]
embedded-assets = []

[dependencies]
async-stream.workspace = true
axum.workspace = true
bytes.workspace = true
http.workspace = true
nidus-events = { path = "../nidus-events", version = "1.0.4" }
nidus-http = { path = "../nidus-http", version = "1.0.4" }
nidus-jobs = { path = "../nidus-jobs", version = "1.0.4" }
serde.workspace = true
serde_json.workspace = true
sqlx = { workspace = true, optional = true }
thiserror.workspace = true
time.workspace = true
tokio.workspace = true
tokio-stream.workspace = true
tower.workspace = true
tracing.workspace = true
uuid.workspace = true

[dev-dependencies]
nidus-testing = { path = "../nidus-testing", version = "1.0.4" }
tower.workspace = true
```

- [ ] **Step 5: Add initial public modules**

Create `crates/nidus-dashboard/src/lib.rs`:

```rust
#![deny(missing_docs)]

//! Embedded dashboard for Nidus applications.
//!
//! `nidus-dashboard` serves a protected dashboard UI, JSON APIs, and live
//! introspection stream from the same Axum application as the user's service.

mod auth;
mod collector;
mod config;
mod error;
mod router;
mod storage;
mod types;

pub use config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage};
pub use error::{DashboardError, Result};
pub use router::NidusDashboard;
pub use types::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus, DashboardRouteSnapshot,
};
```

Create `crates/nidus-dashboard/src/error.rs`:

```rust
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
```

Create `crates/nidus-dashboard/src/config.rs`:

```rust
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
                .or_else(|| Some(PathBuf::from("nidus-dashboard.sqlite").display().to_string())),
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
}

impl Default for DashboardRetention {
    fn default() -> Self {
        Self::days(7)
    }
}
```

Create `crates/nidus-dashboard/src/types.rs`:

```rust
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
```

Create minimal module stubs that compile:

```rust
// crates/nidus-dashboard/src/auth.rs
//! Dashboard authentication.

// crates/nidus-dashboard/src/collector.rs
//! Dashboard capture hooks.

// crates/nidus-dashboard/src/storage/mod.rs
//! Dashboard storage backends.

// crates/nidus-dashboard/src/router.rs
use axum::Router;

use crate::{
    config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage},
    error::{DashboardError, Result},
};

/// Embedded Nidus Dashboard.
#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
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
        if self.auth.is_none() {
            return Err(DashboardError::MissingAuth);
        }
        if !self.path.starts_with('/') || self.path.ends_with('/') {
            return Err(DashboardError::InvalidPath);
        }
        let _ = self.storage;
        let _ = self.capture;
        let _ = self.retention;
        Ok(NidusDashboard { path: self.path })
    }
}
```

- [ ] **Step 6: Run the builder test**

Run:

```bash
cargo test -p nidus-dashboard --test builder
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/nidus-dashboard
git commit -m "feat(dashboard): scaffold embedded dashboard crate"
```

## Task 2: Auth Gate And Protected Router Skeleton

**Files:**
- Modify: `crates/nidus-dashboard/src/auth.rs`
- Modify: `crates/nidus-dashboard/src/router.rs`
- Create: `crates/nidus-dashboard/tests/auth.rs`

- [ ] **Step 1: Add failing auth integration tests**

Create `crates/nidus-dashboard/tests/auth.rs`:

```rust
use axum::body::Body;
use http::{Request, StatusCode};
use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};
use tower::ServiceExt;

#[tokio::test]
async fn dashboard_rejects_missing_bearer_token() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_rejects_invalid_bearer_token() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("authorization", "Bearer wrong")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn dashboard_accepts_valid_bearer_token() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p nidus-dashboard --test auth
```

Expected: fail because `router()` returns an empty router.

- [ ] **Step 3: Implement auth state and route protection**

Replace `crates/nidus-dashboard/src/auth.rs` with:

```rust
use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{DashboardAuth, error::Result};

/// Runtime dashboard auth state.
#[derive(Clone, Debug)]
pub enum DashboardAuthState {
    /// Bearer token auth.
    Bearer { token: Arc<str> },
    /// Auth disabled explicitly for local development.
    UnsafeDisabled,
}

impl DashboardAuthState {
    /// Builds runtime auth state from config.
    pub fn from_config(auth: DashboardAuth) -> Result<Self> {
        match auth {
            DashboardAuth::BearerToken(token) => Ok(Self::Bearer {
                token: Arc::from(token),
            }),
            DashboardAuth::BearerFromEnv(name) => {
                let token = std::env::var(&name).unwrap_or_default();
                Ok(Self::Bearer {
                    token: Arc::from(token),
                })
            }
            DashboardAuth::UnsafeDisabledForLocalDevelopment => Ok(Self::UnsafeDisabled),
        }
    }

    fn allows(&self, headers: &HeaderMap) -> bool {
        match self {
            Self::UnsafeDisabled => true,
            Self::Bearer { token } => headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|candidate| candidate == token.as_ref()),
        }
    }
}

/// Axum middleware that enforces dashboard authentication.
pub async fn require_dashboard_auth(
    State(auth): State<DashboardAuthState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if auth.allows(&headers) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
```

Update `crates/nidus-dashboard/src/router.rs` to store `DashboardAuthState` and return a protected route:

```rust
use axum::{Router, middleware, routing::get};

use crate::{
    auth::{DashboardAuthState, require_dashboard_auth},
    config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage},
    error::{DashboardError, Result},
};

#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
    auth: DashboardAuthState,
}

impl NidusDashboard {
    pub fn builder() -> NidusDashboardBuilder {
        NidusDashboardBuilder::default()
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(|| async { "Nidus Dashboard" }))
            .layer(middleware::from_fn_with_state(
                self.auth.clone(),
                require_dashboard_auth,
            ))
    }
}

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
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = path.into();
        self
    }

    pub fn auth(mut self, auth: DashboardAuth) -> Self {
        self.auth = Some(auth);
        self
    }

    pub fn storage(mut self, storage: DashboardStorage) -> Self {
        self.storage = storage;
        self
    }

    pub fn capture(mut self, capture: DashboardCapture) -> Self {
        self.capture = capture;
        self
    }

    pub fn retention(mut self, retention: DashboardRetention) -> Self {
        self.retention = retention;
        self
    }

    pub fn build(self) -> Result<NidusDashboard> {
        let Some(auth) = self.auth else {
            return Err(DashboardError::MissingAuth);
        };
        if !self.path.starts_with('/') || self.path.ends_with('/') {
            return Err(DashboardError::InvalidPath);
        }
        let auth = DashboardAuthState::from_config(auth)?;
        let _ = self.storage;
        let _ = self.capture;
        let _ = self.retention;
        Ok(NidusDashboard {
            path: self.path,
            auth,
        })
    }
}
```

- [ ] **Step 4: Run auth and builder tests**

Run:

```bash
cargo test -p nidus-dashboard --test builder --test auth
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/nidus-dashboard/src/auth.rs crates/nidus-dashboard/src/router.rs crates/nidus-dashboard/tests/auth.rs
git commit -m "feat(dashboard): protect embedded dashboard routes"
```

## Task 3: Storage Domain And In-Memory Backend

**Files:**
- Modify: `crates/nidus-dashboard/src/storage/mod.rs`
- Create: `crates/nidus-dashboard/src/storage/memory.rs`
- Modify: `crates/nidus-dashboard/src/types.rs`
- Modify: `crates/nidus-dashboard/src/lib.rs`
- Create: `crates/nidus-dashboard/tests/storage.rs`

- [ ] **Step 1: Add failing storage tests**

Create `crates/nidus-dashboard/tests/storage.rs`:

```rust
use std::collections::BTreeMap;

use nidus_dashboard::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus,
    storage::{DashboardStorageBackend, MemoryDashboardStorage},
};

fn operation(id: &str, name: &str) -> DashboardOperation {
    DashboardOperation {
        id: id.to_owned(),
        kind: DashboardOperationKind::Event,
        name: name.to_owned(),
        status: DashboardOperationStatus::Success,
        timestamp_ms: 1_725_000_000_000,
        duration_ms: Some(12),
        correlation_id: Some("req-1".to_owned()),
        attributes: BTreeMap::from([("source".to_owned(), "test".to_owned())]),
        payload: None,
    }
}

#[tokio::test]
async fn memory_storage_records_and_lists_timeline_operations() {
    let storage = MemoryDashboardStorage::new();

    storage.record_operation(operation("op-1", "user.created")).await.unwrap();
    storage.record_operation(operation("op-2", "project.created")).await.unwrap();

    let timeline = storage.list_operations(10).await.unwrap();

    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0].id, "op-2");
    assert_eq!(timeline[1].id, "op-1");
}

#[tokio::test]
async fn memory_storage_prunes_to_max_events() {
    let storage = MemoryDashboardStorage::new();

    storage.record_operation(operation("op-1", "first")).await.unwrap();
    storage.record_operation(operation("op-2", "second")).await.unwrap();
    storage.prune(1).await.unwrap();

    let timeline = storage.list_operations(10).await.unwrap();

    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0].id, "op-2");
}
```

- [ ] **Step 2: Run storage tests to verify failure**

Run:

```bash
cargo test -p nidus-dashboard --test storage
```

Expected: fail because storage exports do not exist.

- [ ] **Step 3: Implement storage trait and memory backend**

Replace `crates/nidus-dashboard/src/storage/mod.rs`:

```rust
//! Dashboard storage backends.

mod memory;
#[cfg(feature = "sqlite")]
mod sqlite;

use std::{future::Future, pin::Pin};

use crate::{DashboardOperation, error::Result};

pub use memory::MemoryDashboardStorage;
#[cfg(feature = "sqlite")]
pub use sqlite::SqliteDashboardStorage;

/// Boxed storage future.
pub type StorageFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

/// Dashboard storage backend.
pub trait DashboardStorageBackend: Clone + Send + Sync + 'static {
    /// Records a timeline operation.
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()>;

    /// Lists newest operations first.
    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>>;

    /// Prunes to the newest `max_events` operations.
    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()>;
}
```

Create `crates/nidus-dashboard/src/storage/memory.rs`:

```rust
use std::sync::{Arc, Mutex};

use crate::{DashboardOperation, error::Result};

use super::{DashboardStorageBackend, StorageFuture};

/// In-memory dashboard storage.
#[derive(Clone, Debug, Default)]
pub struct MemoryDashboardStorage {
    operations: Arc<Mutex<Vec<DashboardOperation>>>,
}

impl MemoryDashboardStorage {
    /// Creates empty memory storage.
    pub fn new() -> Self {
        Self::default()
    }
}

impl DashboardStorageBackend for MemoryDashboardStorage {
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            self.operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(operation);
            Ok(())
        })
    }

    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>> {
        Box::pin(async move {
            let operations = self
                .operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            Ok(operations.iter().rev().take(limit).cloned().collect())
        })
    }

    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            let mut operations = self
                .operations
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let len = operations.len();
            if len > max_events {
                operations.drain(..len - max_events);
            }
            Ok(())
        })
    }
}
```

Expose storage publicly in `crates/nidus-dashboard/src/lib.rs`:

```rust
pub mod storage;
```

- [ ] **Step 4: Run storage tests**

Run:

```bash
cargo test -p nidus-dashboard --test storage
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/nidus-dashboard/src/storage crates/nidus-dashboard/src/lib.rs crates/nidus-dashboard/tests/storage.rs
git commit -m "feat(dashboard): add dashboard storage abstraction"
```

## Task 4: SQLite Storage And Retention

**Files:**
- Create: `crates/nidus-dashboard/src/storage/sqlite.rs`
- Modify: `crates/nidus-dashboard/src/error.rs`
- Modify: `crates/nidus-dashboard/tests/storage.rs`

- [ ] **Step 1: Add failing SQLite tests**

Append to `crates/nidus-dashboard/tests/storage.rs`:

```rust
#[cfg(feature = "sqlite")]
#[tokio::test]
async fn sqlite_storage_migrates_records_lists_and_prunes() {
    use nidus_dashboard::storage::SqliteDashboardStorage;

    let storage = SqliteDashboardStorage::connect("sqlite::memory:")
        .await
        .unwrap();

    storage.record_operation(operation("op-1", "first")).await.unwrap();
    storage.record_operation(operation("op-2", "second")).await.unwrap();

    let timeline = storage.list_operations(10).await.unwrap();
    assert_eq!(timeline.len(), 2);
    assert_eq!(timeline[0].id, "op-2");

    storage.prune(1).await.unwrap();
    let pruned = storage.list_operations(10).await.unwrap();
    assert_eq!(pruned.len(), 1);
    assert_eq!(pruned[0].id, "op-2");
}
```

- [ ] **Step 2: Run SQLite test to verify failure**

Run:

```bash
cargo test -p nidus-dashboard --features sqlite --test storage sqlite_storage_migrates_records_lists_and_prunes
```

Expected: fail because `SqliteDashboardStorage` is not implemented.

- [ ] **Step 3: Add SQLx error variant**

Modify `crates/nidus-dashboard/src/error.rs`:

```rust
use thiserror::Error;

pub type Result<T> = std::result::Result<T, DashboardError>;

#[derive(Debug, Error)]
pub enum DashboardError {
    #[error("dashboard authentication is required")]
    MissingAuth,

    #[error("dashboard path must start with `/` and must not end with `/`")]
    InvalidPath,

    #[error("dashboard storage error: {0}")]
    Storage(String),

    #[cfg(feature = "sqlite")]
    #[error("dashboard sqlite error: {0}")]
    Sqlite(#[from] sqlx::Error),

    #[error("dashboard serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}
```

- [ ] **Step 4: Implement SQLite backend**

Create `crates/nidus-dashboard/src/storage/sqlite.rs`:

```rust
use sqlx::{Row, SqlitePool, sqlite::SqlitePoolOptions};

use crate::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus,
    error::{DashboardError, Result},
};

use super::{DashboardStorageBackend, StorageFuture};

/// SQLite dashboard storage.
#[derive(Clone, Debug)]
pub struct SqliteDashboardStorage {
    pool: SqlitePool,
}

impl SqliteDashboardStorage {
    /// Connects to SQLite and runs dashboard migrations.
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await?;

        sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS dashboard_operations (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL,
                name TEXT NOT NULL,
                status TEXT NOT NULL,
                timestamp_ms INTEGER NOT NULL,
                duration_ms INTEGER,
                correlation_id TEXT,
                attributes_json TEXT NOT NULL,
                payload_json TEXT
            )",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS dashboard_operations_timestamp_idx
             ON dashboard_operations(timestamp_ms)",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }
}

impl DashboardStorageBackend for SqliteDashboardStorage {
    fn record_operation(&self, operation: DashboardOperation) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            let attributes_json = serde_json::to_string(&operation.attributes)?;
            let payload_json = operation
                .payload
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            sqlx::query(
                "INSERT OR REPLACE INTO dashboard_operations
                 (id, kind, name, status, timestamp_ms, duration_ms, correlation_id, attributes_json, payload_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )
            .bind(&operation.id)
            .bind(kind_to_str(&operation.kind))
            .bind(&operation.name)
            .bind(status_to_str(&operation.status))
            .bind(operation.timestamp_ms)
            .bind(operation.duration_ms.map(|value| value as i64))
            .bind(&operation.correlation_id)
            .bind(attributes_json)
            .bind(payload_json)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }

    fn list_operations(&self, limit: usize) -> StorageFuture<'_, Vec<DashboardOperation>> {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT id, kind, name, status, timestamp_ms, duration_ms, correlation_id, attributes_json, payload_json
                 FROM dashboard_operations
                 ORDER BY timestamp_ms DESC, id DESC
                 LIMIT ?1",
            )
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

            rows.into_iter().map(row_to_operation).collect()
        })
    }

    fn prune(&self, max_events: usize) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            sqlx::query(
                "DELETE FROM dashboard_operations
                 WHERE id NOT IN (
                     SELECT id FROM dashboard_operations
                     ORDER BY timestamp_ms DESC, id DESC
                     LIMIT ?1
                 )",
            )
            .bind(max_events as i64)
            .execute(&self.pool)
            .await?;
            Ok(())
        })
    }
}

fn row_to_operation(row: sqlx::sqlite::SqliteRow) -> Result<DashboardOperation> {
    let kind: String = row.try_get("kind")?;
    let status: String = row.try_get("status")?;
    let duration_ms: Option<i64> = row.try_get("duration_ms")?;
    let attributes_json: String = row.try_get("attributes_json")?;
    let payload_json: Option<String> = row.try_get("payload_json")?;

    Ok(DashboardOperation {
        id: row.try_get("id")?,
        kind: parse_kind(&kind)?,
        name: row.try_get("name")?,
        status: parse_status(&status)?,
        timestamp_ms: row.try_get("timestamp_ms")?,
        duration_ms: duration_ms.map(|value| value as u64),
        correlation_id: row.try_get("correlation_id")?,
        attributes: serde_json::from_str(&attributes_json)?,
        payload: payload_json
            .map(|value| serde_json::from_str(&value))
            .transpose()?,
    })
}

fn kind_to_str(kind: &DashboardOperationKind) -> &'static str {
    match kind {
        DashboardOperationKind::Http => "http",
        DashboardOperationKind::Event => "event",
        DashboardOperationKind::Job => "job",
        DashboardOperationKind::Lifecycle => "lifecycle",
        DashboardOperationKind::Adapter => "adapter",
    }
}

fn status_to_str(status: &DashboardOperationStatus) -> &'static str {
    match status {
        DashboardOperationStatus::Success => "success",
        DashboardOperationStatus::Failure => "failure",
        DashboardOperationStatus::Running => "running",
    }
}

fn parse_kind(value: &str) -> Result<DashboardOperationKind> {
    match value {
        "http" => Ok(DashboardOperationKind::Http),
        "event" => Ok(DashboardOperationKind::Event),
        "job" => Ok(DashboardOperationKind::Job),
        "lifecycle" => Ok(DashboardOperationKind::Lifecycle),
        "adapter" => Ok(DashboardOperationKind::Adapter),
        other => Err(DashboardError::Storage(format!(
            "unknown operation kind `{other}`"
        ))),
    }
}

fn parse_status(value: &str) -> Result<DashboardOperationStatus> {
    match value {
        "success" => Ok(DashboardOperationStatus::Success),
        "failure" => Ok(DashboardOperationStatus::Failure),
        "running" => Ok(DashboardOperationStatus::Running),
        other => Err(DashboardError::Storage(format!(
            "unknown operation status `{other}`"
        ))),
    }
}
```

- [ ] **Step 5: Run storage tests with SQLite**

Run:

```bash
cargo test -p nidus-dashboard --features sqlite --test storage
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/nidus-dashboard/src/error.rs crates/nidus-dashboard/src/storage/sqlite.rs crates/nidus-dashboard/tests/storage.rs
git commit -m "feat(dashboard): persist dashboard timeline in sqlite"
```

## Task 5: Dashboard APIs, Embedded Assets, And SSE

**Files:**
- Modify: `crates/nidus-dashboard/src/router.rs`
- Create: `crates/nidus-dashboard/assets/index.html`
- Create: `crates/nidus-dashboard/assets/styles.css`
- Create: `crates/nidus-dashboard/assets/app.js`
- Create: `crates/nidus-dashboard/tests/router.rs`
- Create: `crates/nidus-dashboard/tests/ui_assets.rs`

- [ ] **Step 1: Add failing router/API/asset tests**

Create `crates/nidus-dashboard/tests/router.rs`:

```rust
use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use nidus_dashboard::{DashboardAuth, DashboardStorage, NidusDashboard};
use tower::ServiceExt;

fn request(path: &str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("authorization", "Bearer secret")
        .body(Body::empty())
        .unwrap()
}

#[tokio::test]
async fn dashboard_serves_shell_and_overview_api() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let shell = app.clone().oneshot(request("/")).await.unwrap();
    assert_eq!(shell.status(), StatusCode::OK);
    let body = to_bytes(shell.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("Nidus Dashboard"));

    let overview = app
        .oneshot(request("/api/overview"))
        .await
        .unwrap();
    assert_eq!(overview.status(), StatusCode::OK);
    let body = to_bytes(overview.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("\"service_name\""));
}

#[tokio::test]
async fn dashboard_serves_assets_under_assets_path() {
    let app = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap()
        .router();

    let styles = app
        .oneshot(request("/assets/styles.css"))
        .await
        .unwrap();

    assert_eq!(styles.status(), StatusCode::OK);
}
```

Create `crates/nidus-dashboard/tests/ui_assets.rs`:

```rust
#[test]
fn embedded_dashboard_assets_include_nidus_palette_and_no_forbidden_patterns() {
    let styles = include_str!("../assets/styles.css");
    assert!(styles.contains("oklch("));
    assert!(styles.contains("--brand:"));
    assert!(!styles.contains("background-clip: text"));
    assert!(!styles.contains("-webkit-background-clip: text"));
    assert!(!styles.contains("border-left: 2px"));
    assert!(!styles.contains("border-left: 3px"));
    assert!(!styles.contains("border-left: 4px"));
    assert!(!styles.contains("border-right: 2px"));
    assert!(!styles.contains("border-right: 3px"));
    assert!(!styles.contains("border-right: 4px"));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test -p nidus-dashboard --test router --test ui_assets
```

Expected: fail because assets and API routes do not exist.

- [ ] **Step 3: Add embedded UI assets using website palette**

Create `crates/nidus-dashboard/assets/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Nidus Dashboard</title>
    <link rel="stylesheet" href="./assets/styles.css" />
  </head>
  <body>
    <div class="dashboard-shell">
      <header class="topbar">
        <div>
          <p class="eyebrow">Live introspection</p>
          <h1>Nidus Dashboard</h1>
        </div>
        <div class="status-pill" id="connection-status">connecting</div>
      </header>
      <aside class="sidebar" aria-label="Dashboard sections">
        <button class="nav-item active" data-view="overview">Overview</button>
        <button class="nav-item" data-view="graph">Graph</button>
        <button class="nav-item" data-view="routes">Routes</button>
        <button class="nav-item" data-view="timeline">Timeline</button>
        <button class="nav-item" data-view="events">Events</button>
        <button class="nav-item" data-view="jobs">Jobs</button>
        <button class="nav-item" data-view="adapters">Adapters</button>
        <button class="nav-item" data-view="settings">Settings</button>
      </aside>
      <main class="main-panel">
        <section class="view active" id="overview">
          <div class="metric-grid" id="overview-grid"></div>
        </section>
        <section class="view" id="timeline">
          <ol class="timeline" id="timeline-list"></ol>
        </section>
        <section class="view" id="graph"><div class="empty-state">Graph snapshot waiting for route metadata.</div></section>
        <section class="view" id="routes"><div class="empty-state">Route snapshot waiting for Nidus facade integration.</div></section>
        <section class="view" id="events"><div class="empty-state">Observed events appear after publication.</div></section>
        <section class="view" id="jobs"><div class="empty-state">Observed jobs appear after execution.</div></section>
        <section class="view" id="adapters"><div class="empty-state">Adapter operations appear when official hooks record them.</div></section>
        <section class="view" id="settings"><div class="empty-state">Dashboard settings load from the local app.</div></section>
      </main>
      <aside class="inspector">
        <h2>Inspector</h2>
        <pre id="inspector-output">{}</pre>
      </aside>
    </div>
    <script src="./assets/app.js" type="module"></script>
  </body>
</html>
```

Create `crates/nidus-dashboard/assets/styles.css` with the website palette and dense app layout. Preserve OKLCH tokens from `website/src/styles.css`, but tune for dashboards:

```css
@import url("https://fonts.googleapis.com/css2?family=Afacad+Flux:wght@400;520;650;760&family=Bricolage+Grotesque:wght@500;700;800&display=swap");

:root {
  color-scheme: light;
  --bg: oklch(97.5% 0.012 304);
  --panel: oklch(99% 0.006 304);
  --panel-2: oklch(94.5% 0.021 304);
  --panel-3: oklch(90.5% 0.033 304);
  --line: oklch(82% 0.034 304);
  --line-soft: oklch(88% 0.025 304);
  --text: oklch(30% 0.029 286);
  --ink: oklch(17% 0.036 286);
  --muted: oklch(43% 0.03 286);
  --brand: oklch(47% 0.17 296);
  --brand-soft: oklch(39% 0.15 296);
  --brand-tint: oklch(92% 0.052 303);
  --code-bg: oklch(15% 0.026 286);
  --code-panel: oklch(19% 0.03 286);
  --code-text: oklch(93% 0.014 286);
  --radius: 8px;
  --space-xs: 0.25rem;
  --space-sm: 0.5rem;
  --space-md: 0.75rem;
  --space-lg: 1rem;
  --space-xl: 1.5rem;
  --space-2xl: 2rem;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  min-height: 100vh;
  background:
    linear-gradient(90deg, color-mix(in oklch, var(--line), transparent 72%) 1px, transparent 1px),
    linear-gradient(color-mix(in oklch, var(--line), transparent 78%) 1px, transparent 1px),
    var(--bg);
  background-size: 88px 88px, 88px 88px, auto;
  color: var(--text);
  font-family: "Afacad Flux", "Segoe UI", sans-serif;
  line-height: 1.45;
}

button {
  font: inherit;
}

.dashboard-shell {
  display: grid;
  grid-template-columns: 184px minmax(0, 1fr) minmax(280px, 360px);
  grid-template-rows: auto minmax(0, 1fr);
  gap: var(--space-md);
  min-height: 100vh;
  padding: var(--space-md);
}

.topbar,
.sidebar,
.main-panel,
.inspector {
  border: 1px solid color-mix(in oklch, var(--line), transparent 24%);
  border-radius: var(--radius);
  background: color-mix(in oklch, var(--panel), transparent 4%);
}

.topbar {
  grid-column: 1 / -1;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-xl);
  padding: var(--space-lg) var(--space-xl);
}

.eyebrow {
  margin: 0 0 var(--space-xs);
  color: var(--brand-soft);
  font-size: 0.78rem;
  font-weight: 760;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

h1,
h2 {
  margin: 0;
  color: var(--ink);
  font-family: "Bricolage Grotesque", "Afacad Flux", sans-serif;
  letter-spacing: 0;
}

h1 {
  font-size: 1.65rem;
  line-height: 1;
}

h2 {
  font-size: 1.1rem;
  line-height: 1.15;
}

.status-pill {
  border: 1px solid color-mix(in oklch, var(--brand), transparent 55%);
  border-radius: 999px;
  padding: var(--space-xs) var(--space-md);
  background: var(--brand-tint);
  color: var(--brand-soft);
  font-weight: 760;
}

.sidebar {
  display: flex;
  flex-direction: column;
  gap: var(--space-xs);
  padding: var(--space-md);
}

.nav-item {
  min-height: 40px;
  border: 1px solid transparent;
  border-radius: var(--radius);
  background: transparent;
  color: var(--muted);
  text-align: left;
  font-weight: 650;
  cursor: pointer;
}

.nav-item:hover,
.nav-item:focus-visible,
.nav-item.active {
  border-color: var(--line-soft);
  background: var(--panel-2);
  color: var(--ink);
}

.main-panel {
  min-width: 0;
  padding: var(--space-lg);
  overflow: auto;
}

.inspector {
  min-width: 0;
  padding: var(--space-lg);
  overflow: auto;
}

.view {
  display: none;
}

.view.active {
  display: block;
}

.metric-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: var(--space-md);
}

.metric {
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
  background: var(--panel);
  padding: var(--space-lg);
}

.metric strong {
  display: block;
  color: var(--ink);
  font-family: "Bricolage Grotesque", "Afacad Flux", sans-serif;
  font-size: 1.5rem;
  line-height: 1.1;
}

.metric span {
  color: var(--muted);
  font-weight: 650;
}

.timeline {
  display: grid;
  gap: var(--space-sm);
  margin: 0;
  padding: 0;
  list-style: none;
}

.timeline-item {
  border: 1px solid var(--line-soft);
  border-radius: var(--radius);
  background: var(--panel);
  padding: var(--space-md);
  cursor: pointer;
}

.timeline-item:hover,
.timeline-item:focus-visible {
  border-color: color-mix(in oklch, var(--brand), transparent 45%);
}

.empty-state {
  border: 1px dashed color-mix(in oklch, var(--brand), transparent 58%);
  border-radius: var(--radius);
  padding: var(--space-2xl);
  color: var(--muted);
  background: color-mix(in oklch, var(--brand-tint), transparent 34%);
}

pre {
  min-height: 260px;
  margin: var(--space-lg) 0 0;
  overflow: auto;
  border-radius: var(--radius);
  background: var(--code-bg);
  color: var(--code-text);
  padding: var(--space-md);
  font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
  font-size: 0.82rem;
}

@media (max-width: 980px) {
  .dashboard-shell {
    grid-template-columns: 1fr;
  }

  .sidebar {
    flex-direction: row;
    overflow-x: auto;
  }

  .nav-item {
    flex: 0 0 auto;
  }
}
```

Create `crates/nidus-dashboard/assets/app.js`:

```javascript
const views = document.querySelectorAll(".view");
const buttons = document.querySelectorAll(".nav-item");
const overviewGrid = document.querySelector("#overview-grid");
const timelineList = document.querySelector("#timeline-list");
const inspector = document.querySelector("#inspector-output");
const status = document.querySelector("#connection-status");

for (const button of buttons) {
  button.addEventListener("click", () => {
    for (const item of buttons) item.classList.remove("active");
    for (const view of views) view.classList.remove("active");
    button.classList.add("active");
    document.querySelector(`#${button.dataset.view}`).classList.add("active");
  });
}

async function loadOverview() {
  const response = await fetch("./api/overview");
  const overview = await response.json();
  overviewGrid.innerHTML = "";
  for (const metric of overview.metrics) {
    const node = document.createElement("article");
    node.className = "metric";
    node.innerHTML = `<strong>${metric.value}</strong><span>${metric.label}</span>`;
    overviewGrid.appendChild(node);
  }
}

function appendOperation(operation) {
  const item = document.createElement("li");
  item.className = "timeline-item";
  item.tabIndex = 0;
  item.textContent = `${operation.kind} ${operation.name} ${operation.status}`;
  item.addEventListener("click", () => {
    inspector.textContent = JSON.stringify(operation, null, 2);
  });
  timelineList.prepend(item);
}

function connectStream() {
  const stream = new EventSource("./stream");
  stream.addEventListener("open", () => {
    status.textContent = "live";
  });
  stream.addEventListener("message", (event) => {
    appendOperation(JSON.parse(event.data));
  });
  stream.addEventListener("error", () => {
    status.textContent = "reconnecting";
  });
}

await loadOverview();
connectStream();
```

- [ ] **Step 4: Implement API routes and asset serving**

Update `crates/nidus-dashboard/src/router.rs` so `router()` contains:

```rust
use axum::{
    Json, Router, middleware,
    response::{Html, IntoResponse, Sse},
    routing::get,
};
use serde::Serialize;
use tokio_stream::StreamExt;

use crate::{
    auth::{DashboardAuthState, require_dashboard_auth},
    config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage},
    error::{DashboardError, Result},
    types::{DashboardOperation, DashboardOperationKind, DashboardOperationStatus},
};

const INDEX_HTML: &str = include_str!("../assets/index.html");
const STYLES_CSS: &str = include_str!("../assets/styles.css");
const APP_JS: &str = include_str!("../assets/app.js");

#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
    auth: DashboardAuthState,
}

impl NidusDashboard {
    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(index))
            .route("/assets/styles.css", get(styles))
            .route("/assets/app.js", get(app_js))
            .route("/api/overview", get(overview))
            .route("/api/timeline", get(timeline))
            .route("/stream", get(stream))
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

async fn stream() -> Sse<impl futures_util::Stream<Item = std::result::Result<axum::response::sse::Event, std::convert::Infallible>>> {
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
    let stream = tokio_stream::once(Ok(axum::response::sse::Event::default().data(data)));
    Sse::new(stream)
}
```

Preserve the existing builder code from Task 2 in the same file; only replace route construction and add handlers.

- [ ] **Step 5: Run router and asset tests**

Run:

```bash
cargo test -p nidus-dashboard --test router --test ui_assets
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add crates/nidus-dashboard/src/router.rs crates/nidus-dashboard/assets crates/nidus-dashboard/tests/router.rs crates/nidus-dashboard/tests/ui_assets.rs
git commit -m "feat(dashboard): serve embedded dashboard UI and APIs"
```

## Task 6: Collector Hooks And Metadata-Only Capture

**Files:**
- Modify: `crates/nidus-dashboard/src/collector.rs`
- Modify: `crates/nidus-dashboard/src/router.rs`
- Modify: `crates/nidus-dashboard/src/lib.rs`
- Create: `crates/nidus-dashboard/tests/capture.rs`

- [ ] **Step 1: Add failing capture tests**

Create `crates/nidus-dashboard/tests/capture.rs`:

```rust
use nidus_dashboard::{
    DashboardAuth, DashboardOperationKind, DashboardStorage, NidusDashboard,
    storage::DashboardStorageBackend,
};

#[tokio::test]
async fn dashboard_collector_records_metadata_without_payloads() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_event("user.created", Some("op-1"), [("tenant", "acme")])
        .await
        .unwrap();

    let operations = dashboard.storage().list_operations(10).await.unwrap();
    assert_eq!(operations.len(), 1);
    assert_eq!(operations[0].kind, DashboardOperationKind::Event);
    assert_eq!(operations[0].name, "user.created");
    assert_eq!(operations[0].payload, None);
    assert_eq!(
        operations[0].attributes.get("tenant").map(String::as_str),
        Some("acme")
    );
}
```

- [ ] **Step 2: Run capture test to verify failure**

Run:

```bash
cargo test -p nidus-dashboard --test capture
```

Expected: fail because collector APIs do not exist.

- [ ] **Step 3: Implement collector**

Replace `crates/nidus-dashboard/src/collector.rs`:

```rust
//! Dashboard capture hooks.

use std::collections::BTreeMap;

use crate::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus,
    error::Result,
    storage::{DashboardStorageBackend, MemoryDashboardStorage},
};

/// Dashboard collector.
#[derive(Clone, Debug)]
pub struct DashboardCollector<S = MemoryDashboardStorage>
where
    S: DashboardStorageBackend,
{
    storage: S,
}

impl<S> DashboardCollector<S>
where
    S: DashboardStorageBackend,
{
    /// Creates a collector.
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    /// Records an observed event publication.
    pub async fn record_event<I, K, V>(
        &self,
        name: impl Into<String>,
        operation_id: Option<&str>,
        attributes: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let operation = DashboardOperation {
            id: operation_id
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            kind: DashboardOperationKind::Event,
            name: name.into(),
            status: DashboardOperationStatus::Success,
            timestamp_ms: now_ms(),
            duration_ms: None,
            correlation_id: operation_id.map(str::to_owned),
            attributes: attributes
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect::<BTreeMap<_, _>>(),
            payload: None,
        };
        self.storage.record_operation(operation).await
    }
}

fn now_ms() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp_nanos() as i64 / 1_000_000
}
```

Update `crates/nidus-dashboard/src/lib.rs`:

```rust
pub use collector::DashboardCollector;
```

Update `NidusDashboard` in `router.rs` to store memory storage and collector for now:

```rust
use crate::{
    collector::DashboardCollector,
    storage::MemoryDashboardStorage,
};

#[derive(Clone, Debug)]
pub struct NidusDashboard {
    path: String,
    auth: DashboardAuthState,
    storage: MemoryDashboardStorage,
    collector: DashboardCollector<MemoryDashboardStorage>,
}

impl NidusDashboard {
    pub fn collector(&self) -> DashboardCollector<MemoryDashboardStorage> {
        self.collector.clone()
    }

    pub fn storage(&self) -> MemoryDashboardStorage {
        self.storage.clone()
    }
}
```

In `build`, initialize:

```rust
let storage = MemoryDashboardStorage::new();
let collector = DashboardCollector::new(storage.clone());
Ok(NidusDashboard {
    path: self.path,
    auth,
    storage,
    collector,
})
```

Keep SQLite selection as a later refinement after the memory path is fully wired.

- [ ] **Step 4: Run capture tests**

Run:

```bash
cargo test -p nidus-dashboard --test capture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/nidus-dashboard/src/collector.rs crates/nidus-dashboard/src/lib.rs crates/nidus-dashboard/src/router.rs crates/nidus-dashboard/tests/capture.rs
git commit -m "feat(dashboard): capture metadata-only dashboard events"
```

## Task 7: Facade Integration With `with_dashboard`

**Files:**
- Modify: `crates/nidus/Cargo.toml`
- Modify: `crates/nidus/src/lib.rs`
- Modify: `crates/nidus/src/prelude.rs`
- Modify: `crates/nidus/src/app.rs`
- Modify: `Cargo.toml`
- Create: `crates/nidus/tests/dashboard_features.rs`

- [ ] **Step 1: Add failing facade test**

Create `crates/nidus/tests/dashboard_features.rs`:

```rust
#![cfg(feature = "dashboard")]

use axum::body::{Body, to_bytes};
use http::{Request, StatusCode};
use nidus::prelude::*;
use tower::ServiceExt;

#[module]
struct AppModule;

#[tokio::test]
async fn module_builder_mounts_dashboard_router() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    let app = Nidus::create::<AppModule>()
        .with_dashboard(dashboard)
        .build()
        .await
        .unwrap()
        .into_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nidus/dashboard/")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("Nidus Dashboard"));
}
```

- [ ] **Step 2: Run facade test to verify failure**

Run:

```bash
cargo test -p nidus-rs --features dashboard --test dashboard_features
```

Expected: fail because the `dashboard` feature and `with_dashboard` do not exist.

- [ ] **Step 3: Add facade feature and dependency**

Modify `crates/nidus/Cargo.toml`:

```toml
[features]
default = ["http", "config", "tracing"]
http = ["dep:nidus-http"]
otel = ["http", "nidus-http/otel"]
observability = ["dep:nidus-observability", "http"]
dashboard = ["dep:nidus-dashboard", "http"]
config = ["dep:nidus-config"]
openapi = ["dep:nidus-openapi"]
validation = ["dep:nidus-validation"]
auth = ["dep:nidus-auth"]
events = ["dep:nidus-events"]
jobs = ["dep:nidus-jobs"]
testing = ["dep:nidus-testing"]
tracing = []

[dependencies]
nidus-dashboard = { path = "../nidus-dashboard", version = "1.0.4", optional = true }
```

Add root dev dependency in `Cargo.toml` if needed for workspace tests:

```toml
nidus-dashboard = { path = "crates/nidus-dashboard", version = "1.0.4" }
```

- [ ] **Step 4: Add facade exports**

Modify `crates/nidus/src/lib.rs`:

```rust
#[cfg(feature = "dashboard")]
pub use nidus_dashboard as dashboard;
```

Modify `crates/nidus/src/prelude.rs`:

```rust
#[cfg(feature = "dashboard")]
pub use nidus_dashboard::{
    DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage, NidusDashboard,
};
```

- [ ] **Step 5: Add builder integration**

Modify `crates/nidus/src/app.rs`:

```rust
#[cfg(feature = "dashboard")]
use nidus_dashboard::NidusDashboard;
```

Add a field to `NidusApplicationBuilder`:

```rust
#[cfg(feature = "dashboard")]
dashboard: Option<NidusDashboard>,
```

Initialize it in `new()`:

```rust
#[cfg(feature = "dashboard")]
dashboard: None,
```

Add builder method:

```rust
/// Mounts Nidus Dashboard into the composed HTTP application.
#[cfg(feature = "dashboard")]
pub fn with_dashboard(mut self, dashboard: NidusDashboard) -> Self {
    self.dashboard = Some(dashboard);
    self
}
```

Merge the dashboard router in `build_router` after user routes and before observability layers:

```rust
#[cfg(feature = "dashboard")]
if let Some(dashboard) = &self.dashboard {
    let path = dashboard.path().to_owned();
    router = router.nest(&path, dashboard.router());
}
```

- [ ] **Step 6: Run facade dashboard test**

Run:

```bash
cargo test -p nidus-rs --features dashboard --test dashboard_features
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml crates/nidus/Cargo.toml crates/nidus/src/lib.rs crates/nidus/src/prelude.rs crates/nidus/src/app.rs crates/nidus/tests/dashboard_features.rs
git commit -m "feat(dashboard): add facade dashboard integration"
```

## Task 8: Route Snapshot And Settings APIs

**Files:**
- Modify: `crates/nidus-dashboard/src/types.rs`
- Modify: `crates/nidus-dashboard/src/storage/mod.rs`
- Modify: `crates/nidus-dashboard/src/storage/memory.rs`
- Modify: `crates/nidus-dashboard/src/router.rs`
- Modify: `crates/nidus/src/app.rs`
- Modify: `crates/nidus/tests/dashboard_features.rs`

- [ ] **Step 1: Add failing route snapshot test**

Append to `crates/nidus/tests/dashboard_features.rs` with a controller that has a route and assert `/api/routes` contains it.

```rust
#[controller("/users")]
struct UsersController;

impl UsersController {
    #[get("/{id}")]
    async fn show(&self) -> &'static str {
        "user"
    }
}

#[module(controllers = [UsersController])]
struct RoutesModule;

#[tokio::test]
async fn dashboard_routes_api_includes_nidus_controller_routes() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .build()
        .unwrap();

    let app = Nidus::create::<RoutesModule>()
        .with_dashboard(dashboard)
        .build()
        .await
        .unwrap()
        .into_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/nidus/dashboard/api/routes")
                .header("authorization", "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8_lossy(&body);
    assert!(text.contains("\"method\":\"GET\""), "{text}");
    assert!(text.contains("\"path\":\"/users/{id}\""), "{text}");
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test -p nidus-rs --features dashboard --test dashboard_features dashboard_routes_api_includes_nidus_controller_routes
```

Expected: fail because route snapshots are not recorded.

- [ ] **Step 3: Add route snapshot storage methods**

Extend `DashboardStorageBackend`:

```rust
fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> StorageFuture<'_, ()>;
fn list_route_snapshots(&self) -> StorageFuture<'_, Vec<DashboardRouteSnapshot>>;
```

Implement in memory storage with a second `Arc<Mutex<Vec<DashboardRouteSnapshot>>>`.

- [ ] **Step 4: Add route snapshot API**

Update `NidusDashboard`:

```rust
pub async fn record_route_snapshot(&self, route: DashboardRouteSnapshot) -> Result<()> {
    self.storage.record_route_snapshot(route).await
}
```

Update router routes:

```rust
.route("/api/routes", get(routes))
.route("/api/settings", get(settings))
```

Handlers should return JSON with route snapshots and settings. Use `State<DashboardRuntime>` if needed to access storage from handlers.

- [ ] **Step 5: Register snapshots from facade builder**

In `crates/nidus/src/app.rs`, while iterating route metadata in `build_router`, collect snapshots:

```rust
#[cfg(feature = "dashboard")]
let mut dashboard_routes = Vec::new();
```

Inside the route loop after `full_path` is computed:

```rust
#[cfg(feature = "dashboard")]
dashboard_routes.push(nidus_dashboard::DashboardRouteSnapshot {
    method: route.method().to_owned(),
    path: full_path.clone(),
    summary: route.summary().map(str::to_owned),
    guards: route.guards().iter().map(|value| (*value).to_owned()).collect(),
    pipes: route.pipes().iter().map(|value| (*value).to_owned()).collect(),
    validates: route.validates(),
});
```

After module/controller scanning and before dashboard router merge:

```rust
#[cfg(feature = "dashboard")]
if let Some(dashboard) = &self.dashboard {
    let dashboard = dashboard.clone();
    let routes = dashboard_routes.clone();
    tokio::spawn(async move {
        for route in routes {
            let _ = dashboard.record_route_snapshot(route).await;
        }
    });
}
```

If spawning from `build_router` causes lifetime or runtime issues, move route snapshot registration into `build` after `build_router` by returning route snapshots from a helper. Keep tests as the source of truth.

- [ ] **Step 6: Run route snapshot test**

Run:

```bash
cargo test -p nidus-rs --features dashboard --test dashboard_features dashboard_routes_api_includes_nidus_controller_routes
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/nidus-dashboard/src crates/nidus/src/app.rs crates/nidus/tests/dashboard_features.rs
git commit -m "feat(dashboard): expose route snapshots in dashboard"
```

## Task 9: Payload Redaction Contract

**Files:**
- Modify: `crates/nidus-dashboard/src/config.rs`
- Modify: `crates/nidus-dashboard/src/collector.rs`
- Modify: `crates/nidus-dashboard/tests/capture.rs`

- [ ] **Step 1: Add failing redaction tests**

Append to `crates/nidus-dashboard/tests/capture.rs`:

```rust
#[tokio::test]
async fn payload_capture_redacts_configured_fields() {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_token("secret"))
        .storage(DashboardStorage::memory())
        .capture(
            nidus_dashboard::DashboardCapture::payloads()
                .redact_fields(["password", "token"])
                .max_payload_bytes(1024),
        )
        .build()
        .unwrap();

    dashboard
        .collector()
        .record_payload_event(
            "user.login",
            Some("op-2"),
            serde_json::json!({
                "email": "user@example.com",
                "password": "secret",
                "nested": { "token": "abc" }
            }),
        )
        .await
        .unwrap();

    let operations = dashboard.storage().list_operations(10).await.unwrap();
    let payload = operations[0].payload.as_ref().unwrap();

    assert_eq!(payload["email"], "user@example.com");
    assert_eq!(payload["password"], "[redacted]");
    assert_eq!(payload["nested"]["token"], "[redacted]");
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test -p nidus-dashboard --test capture payload_capture_redacts_configured_fields
```

Expected: fail because payload recording is not implemented.

- [ ] **Step 3: Expose capture config to collector and implement redaction**

Make `DashboardCollector` hold `DashboardCapture`.

Add:

```rust
pub async fn record_payload_event(
    &self,
    name: impl Into<String>,
    operation_id: Option<&str>,
    payload: serde_json::Value,
) -> Result<()> {
    let payload = if self.capture.captures_payloads() {
        Some(redact_value(payload, self.capture.redacted_fields()))
    } else {
        None
    };
    let operation = DashboardOperation {
        id: operation_id
            .map(str::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        kind: DashboardOperationKind::Event,
        name: name.into(),
        status: DashboardOperationStatus::Success,
        timestamp_ms: now_ms(),
        duration_ms: None,
        correlation_id: operation_id.map(str::to_owned),
        attributes: BTreeMap::new(),
        payload,
    };
    self.storage.record_operation(operation).await
}
```

Add `redacted_fields(&self) -> &[String]` to `DashboardCapture`.

Implement recursive `redact_value`:

```rust
fn redact_value(mut value: serde_json::Value, redacted_fields: &[String]) -> serde_json::Value {
    match &mut value {
        serde_json::Value::Object(map) => {
            for (key, value) in map.iter_mut() {
                if redacted_fields.iter().any(|field| field.eq_ignore_ascii_case(key)) {
                    *value = serde_json::Value::String("[redacted]".to_owned());
                } else {
                    *value = redact_value(std::mem::take(value), redacted_fields);
                }
            }
            value
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                *item = redact_value(std::mem::take(item), redacted_fields);
            }
            value
        }
        _ => value,
    }
}
```

- [ ] **Step 4: Run capture tests**

Run:

```bash
cargo test -p nidus-dashboard --test capture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/nidus-dashboard/src/config.rs crates/nidus-dashboard/src/collector.rs crates/nidus-dashboard/tests/capture.rs
git commit -m "feat(dashboard): redact opt-in captured payloads"
```

## Task 10: Example App And Documentation

**Files:**
- Create: `examples/dashboard-api/Cargo.toml`
- Create: `examples/dashboard-api/src/main.rs`
- Create: `examples/dashboard-api/README.md`
- Modify: `Cargo.toml`
- Create: `docs/dashboard.md`
- Modify: `docs/README.md`
- Modify: `README.md`
- Modify: `docs/examples.md`

- [ ] **Step 1: Add example workspace member**

Modify root `Cargo.toml` members to include:

```toml
"examples/dashboard-api",
```

- [ ] **Step 2: Create dashboard example manifest**

Create `examples/dashboard-api/Cargo.toml`:

```toml
[package]
name = "nidus-example-dashboard-api"
version = "0.1.0"
edition.workspace = true
publish = false

[dependencies]
axum.workspace = true
nidus = { package = "nidus-rs", path = "../../crates/nidus", features = ["dashboard", "http"] }
tokio.workspace = true
```

- [ ] **Step 3: Create example app**

Create `examples/dashboard-api/src/main.rs`:

```rust
use nidus::prelude::*;

#[controller("/hello")]
struct HelloController;

impl HelloController {
    #[get("/")]
    async fn hello(&self) -> &'static str {
        "hello from Nidus Dashboard example"
    }
}

#[module(controllers = [HelloController])]
struct AppModule;

#[tokio::main]
async fn main() -> nidus::Result<()> {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN"))
        .storage(DashboardStorage::sqlite_from_env("NIDUS_DASHBOARD_DATABASE_URL"))
        .capture(DashboardCapture::metadata_only())
        .retention(DashboardRetention::days(7).max_events(100_000))
        .build()?;

    Nidus::create::<AppModule>()
        .with_dashboard(dashboard)
        .build()
        .await?
        .listen("127.0.0.1:4310")
        .await
}
```

- [ ] **Step 4: Create docs**

Create `docs/dashboard.md` with:

```markdown
# Nidus Dashboard

`nidus-dashboard` is an optional embedded dashboard for inspecting a running Nidus application.

```toml
nidus = { package = "nidus-rs", version = "1.0.4", features = ["dashboard"] }
```

```rust
let dashboard = NidusDashboard::builder()
    .path("/nidus/dashboard")
    .auth(DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN"))
    .storage(DashboardStorage::sqlite_from_env("NIDUS_DASHBOARD_DATABASE_URL"))
    .capture(DashboardCapture::metadata_only())
    .retention(DashboardRetention::days(7).max_events(100_000))
    .build()?;

let app = Nidus::create::<AppModule>()
    .with_dashboard(dashboard)
    .build()
    .await?;
```

Dashboard routes are protected by default. Mounting without dashboard auth fails. Use `DashboardAuth::unsafe_disabled_for_local_development()` only for local development.

Default capture is metadata-only. Payload capture is opt-in, byte-capped, and redacted.
```

Update `docs/README.md`, `README.md`, and `docs/examples.md` with one concise dashboard entry.

- [ ] **Step 5: Verify docs and example compile**

Run:

```bash
cargo check -p nidus-example-dashboard-api
cargo test -p nidus-dashboard
cargo test -p nidus-rs --features dashboard --test dashboard_features
```

Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml examples/dashboard-api docs/dashboard.md docs/README.md README.md docs/examples.md
git commit -m "docs(dashboard): document embedded dashboard setup"
```

## Task 11: Final Verification And Release Hygiene

**Files:**
- Modify only files required by failed verification from previous tasks.

- [ ] **Step 1: Run formatting**

Run:

```bash
cargo fmt --all -- --check
```

Expected: pass. If it fails, run `cargo fmt --all`, inspect the diff, and commit formatting with the relevant task fix.

- [ ] **Step 2: Run focused tests**

Run:

```bash
cargo test -p nidus-dashboard
cargo test -p nidus-rs --features dashboard --test dashboard_features
cargo check -p nidus-example-dashboard-api
```

Expected: all pass.

- [ ] **Step 3: Run workspace tests**

Run:

```bash
cargo test --workspace
```

Expected: pass.

- [ ] **Step 4: Run docs build**

Run:

```bash
cargo doc --workspace --no-deps
```

Expected: pass.

- [ ] **Step 5: Run website/docs verification if website files changed**

Run:

```bash
cd website
npm run verify
```

Expected: pass.

- [ ] **Step 6: Final hygiene**

Run:

```bash
git status --short
git diff --check
```

Expected: only intentional changes are present; `git diff --check` has no output.

- [ ] **Step 7: Final commit if needed**

If verification required final fixes:

```bash
git add Cargo.toml README.md docs crates/nidus-dashboard crates/nidus examples/dashboard-api
git commit -m "chore(dashboard): finalize dashboard verification"
```

## Self-Review Checklist

- Spec coverage:
  - Optional embedded crate: Tasks 1 and 7.
  - `/nidus/dashboard` embedded UI/API/stream: Tasks 5 and 7.
  - Built-in zero-trust auth: Task 2.
  - User extension point through Axum router/facade composition: Tasks 2 and 7.
  - SQLite default and configurable storage: Tasks 3, 4, and 10.
  - Metadata-first capture and redaction: Tasks 6 and 9.
  - Live introspection only: Tasks 5, 6, and 8 avoid mutation APIs.
  - Nidus website palette and `$impeccable` UI constraints: Task 5.
  - Docs and example: Task 10.
  - Verification gates: Task 11.
- Placeholder scan:
  - No task depends on unspecified files.
  - Open implementation decisions from the spec have been resolved in this plan.
  - The only future extension point is `DashboardStorage::custom`, which is intentionally not implemented in v1.
- Type consistency:
  - Public setup types are `NidusDashboard`, `DashboardAuth`, `DashboardStorage`, `DashboardCapture`, `DashboardRetention`.
  - Runtime event records use `DashboardOperation`.
  - Storage trait is `DashboardStorageBackend`.
  - Facade method is `with_dashboard`.
