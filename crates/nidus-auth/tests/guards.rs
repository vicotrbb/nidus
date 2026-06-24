use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Router, body::to_bytes, response::IntoResponse, routing::get};
use http::StatusCode;
use nidus_auth::{Guard, GuardContext, GuardError, GuardExt, guard_layer};
use tower::ServiceExt;

#[derive(Clone)]
struct RoleGuard(&'static str);

#[async_trait]
impl Guard<()> for RoleGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        if ctx.route_label() == self.0 {
            Ok(())
        } else {
            Err(GuardError::forbidden("route role does not match"))
        }
    }
}

#[derive(Clone)]
struct AllowGuard;

#[async_trait]
impl Guard<()> for AllowGuard {
    async fn check(&self, _ctx: GuardContext<()>) -> Result<(), GuardError> {
        Ok(())
    }
}

#[derive(Clone)]
struct DenyGuard(&'static str);

#[derive(Clone)]
struct StateGuard;

#[derive(Clone)]
struct AppState {
    allowed: bool,
}

#[async_trait]
impl Guard<AppState> for StateGuard {
    async fn check(&self, ctx: GuardContext<AppState>) -> Result<(), GuardError> {
        if ctx.state().allowed {
            Ok(())
        } else {
            Err(GuardError::unauthorized("state denied request"))
        }
    }
}

#[async_trait]
impl Guard<()> for DenyGuard {
    async fn check(&self, _ctx: GuardContext<()>) -> Result<(), GuardError> {
        Err(GuardError::forbidden(self.0))
    }
}

#[tokio::test]
async fn guard_context_allows_typed_authorization() {
    let guard = RoleGuard("users:read");
    let context = GuardContext::new((), "users:read");

    guard.check(context).await.unwrap();
}

#[tokio::test]
async fn guard_error_carries_forbidden_reason() {
    let guard = RoleGuard("users:read");
    let context = GuardContext::new((), "users:write");

    let error = guard.check(context).await.unwrap_err();

    assert_eq!(error.status_code(), StatusCode::FORBIDDEN);
    assert_eq!(error.code(), "forbidden");
    assert_eq!(error.reason(), "route role does not match");
    assert_eq!(error.to_string(), "route role does not match");
}

#[tokio::test]
async fn guard_and_requires_both_guards_to_pass() {
    let guard = RoleGuard("users:read").and(AllowGuard);
    guard
        .check(GuardContext::new((), "users:read"))
        .await
        .unwrap();

    let error = guard
        .check(GuardContext::new((), "users:write"))
        .await
        .unwrap_err();

    assert_eq!(error.reason(), "route role does not match");
}

#[tokio::test]
async fn guard_or_allows_second_guard_to_authorize() {
    let guard = DenyGuard("first failed").or(AllowGuard);

    guard
        .check(GuardContext::new((), "users:read"))
        .await
        .unwrap();
}

#[tokio::test]
async fn guard_or_returns_first_error_when_all_guards_fail() {
    let guard = DenyGuard("first failed").or(DenyGuard("second failed"));

    let error = guard
        .check(GuardContext::new((), "users:read"))
        .await
        .unwrap_err();

    assert_eq!(error.reason(), "first failed");
}

#[test]
fn guard_error_carries_typed_unauthorized_status() {
    let error = GuardError::unauthorized("missing token");

    assert_eq!(error.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(error.code(), "unauthorized");
    assert_eq!(error.reason(), "missing token");
}

#[tokio::test]
async fn guard_error_supports_custom_statuses() {
    let response =
        GuardError::new(StatusCode::PRECONDITION_FAILED, "terms must be accepted").into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::PRECONDITION_FAILED);
    assert_eq!(json["error"]["code"], "authorization_failed");
    assert_eq!(json["error"]["message"], "terms must be accepted");
}

#[tokio::test]
async fn guard_error_maps_to_stable_json_response() {
    let response = GuardError::forbidden("route role does not match").into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"]["code"], "forbidden");
    assert_eq!(json["error"]["message"], "route role does not match");
}

#[tokio::test]
async fn guard_layer_allows_authorized_requests() {
    let app = Router::new()
        .route("/admin", get(|| async { "ok" }))
        .layer(guard_layer(
            AppState { allowed: true },
            "admin:index",
            StateGuard,
        ));

    let response = app
        .oneshot(
            http::Request::builder()
                .uri("/admin")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn guard_layer_returns_error_response_without_calling_inner_service() {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Router::new()
        .route("/admin", {
            let calls = Arc::clone(&calls);
            get(move || {
                let calls = Arc::clone(&calls);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    "ok"
                }
            })
        })
        .layer(guard_layer(
            AppState { allowed: false },
            "admin:index",
            StateGuard,
        ));

    let response = app
        .oneshot(
            http::Request::builder()
                .uri("/admin")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["error"]["code"], "unauthorized");
    assert_eq!(json["error"]["message"], "state denied request");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
