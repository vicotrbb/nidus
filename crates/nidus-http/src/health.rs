//! Health and readiness registry helpers.

use std::{
    collections::BTreeMap, future::Future, panic::AssertUnwindSafe, pin::Pin, sync::Arc,
    time::Duration,
};

use axum::{Json, Router, response::IntoResponse, routing::get};
use futures_util::{FutureExt, future::join_all};
use http::StatusCode;
use serde::Serialize;
use tokio::time::timeout;

type HealthFuture = Pin<Box<dyn Future<Output = HealthStatus> + Send>>;
type HealthCheck = Arc<dyn Fn() -> HealthFuture + Send + Sync>;

/// Result of a liveness or readiness check.
///
/// Return [`HealthStatus::up`] for healthy dependencies and
/// [`HealthStatus::down`] with a safe diagnostic message for unhealthy ones.
/// Messages are included in health JSON by default and can be suppressed with
/// [`HealthRegistry::hide_details`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthStatus {
    status: HealthState,
    message: Option<String>,
}

impl HealthStatus {
    /// Creates an up health status.
    pub fn up() -> Self {
        Self {
            status: HealthState::Up,
            message: None,
        }
    }

    /// Creates a down health status with a safe diagnostic message.
    ///
    /// Keep the message operational and non-sensitive because it is exposed in
    /// response bodies unless the registry uses [`HealthRegistry::hide_details`].
    pub fn down(message: impl Into<String>) -> Self {
        Self {
            status: HealthState::Down,
            message: Some(message.into()),
        }
    }

    /// Returns whether the check is up.
    pub const fn is_up(&self) -> bool {
        matches!(self.status, HealthState::Up)
    }
}

/// Health check state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthState {
    /// The dependency is healthy.
    Up,
    /// The dependency is unhealthy.
    Down,
}

/// Registry for liveness and readiness checks.
///
/// The registry produces two routes: `/health/live` and `/health/ready`.
/// With no registered checks, each route returns `200 OK` and
/// `{ "status": "up", "checks": {} }`. When any check returns down or times
/// out, the route returns `503 Service Unavailable`.
///
/// Checks are in-process async closures and are polled concurrently as part of
/// the request future; this helper does not provide service discovery or
/// external health storage. Synchronous checks should be quick. Use an async
/// API or an explicit `spawn_blocking` boundary for blocking work.
///
/// ```
/// use std::time::Duration;
/// use nidus_http::health::{HealthRegistry, HealthStatus};
///
/// let health = HealthRegistry::new()
///     .ready_check_sync("database", || HealthStatus::up())
///     .live_check("worker", || async { HealthStatus::up() })
///     .timeout(Duration::from_secs(1));
///
/// let routes = health.routes();
/// # let _: axum::Router = routes;
/// ```
#[derive(Clone)]
pub struct HealthRegistry {
    live_checks: Vec<NamedHealthCheck>,
    ready_checks: Vec<NamedHealthCheck>,
    timeout: Duration,
    expose_details: bool,
}

impl HealthRegistry {
    /// Creates a registry with always-up live/ready routes and no dependencies.
    ///
    /// The default per-check timeout is two seconds and diagnostic messages are
    /// exposed in responses.
    pub fn new() -> Self {
        Self {
            live_checks: Vec::new(),
            ready_checks: Vec::new(),
            timeout: Duration::from_secs(2),
            expose_details: true,
        }
    }

    /// Adds a liveness check.
    ///
    /// Liveness checks should answer "should this process be restarted?" and
    /// usually avoid dependencies that can recover independently.
    pub fn live_check<F, Fut>(mut self, name: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HealthStatus> + Send + 'static,
    {
        self.live_checks.push(NamedHealthCheck::new(name, check));
        self
    }

    /// Adds a synchronous liveness check.
    pub fn live_check_sync<F>(self, name: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> HealthStatus + Send + Sync + 'static,
    {
        self.live_check(name, move || {
            let status = check();
            async move { status }
        })
    }

    /// Adds a readiness check.
    ///
    /// Readiness checks should answer "can this process serve traffic now?" and
    /// commonly include database, queue, or cache dependencies.
    pub fn ready_check<F, Fut>(mut self, name: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HealthStatus> + Send + 'static,
    {
        self.ready_checks.push(NamedHealthCheck::new(name, check));
        self
    }

    /// Adds a synchronous readiness check.
    pub fn ready_check_sync<F>(self, name: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> HealthStatus + Send + Sync + 'static,
    {
        self.ready_check(name, move || {
            let status = check();
            async move { status }
        })
    }

    /// Sets the timeout for each health check.
    ///
    /// A timed-out check is reported as down with `check timed out`.
    pub fn timeout(mut self, timeout_duration: Duration) -> Self {
        self.timeout = timeout_duration;
        self
    }

    /// Hides diagnostic messages from health response bodies.
    ///
    /// Status values and check names remain visible; only per-check messages are
    /// omitted.
    pub fn hide_details(mut self) -> Self {
        self.expose_details = false;
        self
    }

    /// Returns Axum routes for `/health/live` and `/health/ready`.
    pub fn routes(self) -> Router {
        let Self {
            live_checks,
            ready_checks,
            timeout,
            expose_details,
        } = self;
        let live = HealthRoute::new(live_checks, timeout, expose_details);
        let ready = HealthRoute::new(ready_checks, timeout, expose_details);
        Router::new()
            .route("/health/live", get(move || live.clone().run()))
            .route("/health/ready", get(move || ready.clone().run()))
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct HealthRoute {
    checks: Arc<[NamedHealthCheck]>,
    timeout: Duration,
    expose_details: bool,
}

impl HealthRoute {
    fn new(checks: Vec<NamedHealthCheck>, timeout: Duration, expose_details: bool) -> Self {
        Self {
            checks: checks.into(),
            timeout,
            expose_details,
        }
    }

    async fn run(self) -> axum::response::Response {
        self.run_checks().await.into_response()
    }

    async fn run_checks(self) -> (StatusCode, Json<HealthBody>) {
        let Self {
            checks,
            timeout: timeout_duration,
            expose_details,
        } = self;
        if checks.is_empty() {
            return (
                StatusCode::OK,
                Json(HealthBody {
                    status: HealthState::Up,
                    checks: BTreeMap::new(),
                }),
            );
        }

        let statuses = join_all(checks.iter().map(|check| {
            let check = Arc::clone(&check.check);
            AssertUnwindSafe(async move { timeout(timeout_duration, check()).await }).catch_unwind()
        }))
        .await;

        let mut body_checks = BTreeMap::new();
        let mut all_up = true;
        for (check, result) in checks.iter().zip(statuses) {
            let status = match result {
                Ok(Ok(status)) => status,
                Ok(Err(_)) => HealthStatus::down("check timed out"),
                Err(_) => HealthStatus::down("check panicked"),
            };
            all_up &= status.is_up();
            body_checks.insert(
                check.name.clone(),
                HealthCheckBody {
                    status: status.status,
                    message: if expose_details { status.message } else { None },
                },
            );
        }

        let status = if all_up {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        (
            status,
            Json(HealthBody {
                status: if all_up {
                    HealthState::Up
                } else {
                    HealthState::Down
                },
                checks: body_checks,
            }),
        )
    }
}

#[derive(Clone)]
struct NamedHealthCheck {
    name: String,
    check: HealthCheck,
}

impl NamedHealthCheck {
    fn new<F, Fut>(name: impl Into<String>, check: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HealthStatus> + Send + 'static,
    {
        Self {
            name: name.into(),
            check: Arc::new(move || Box::pin(check())),
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthBody {
    status: HealthState,
    checks: BTreeMap<String, HealthCheckBody>,
}

#[derive(Debug, Serialize)]
struct HealthCheckBody {
    status: HealthState,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}
