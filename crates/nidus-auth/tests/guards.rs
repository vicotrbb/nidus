use async_trait::async_trait;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Router, body::to_bytes, response::IntoResponse, routing::get};
use http::{HeaderMap, HeaderValue, StatusCode};
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

#[derive(Clone)]
struct ApiKeyGuard;

#[async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        match ctx.api_key("x-api-key")? {
            Some("secret") => Ok(()),
            _ => Err(GuardError::unauthorized("missing or invalid api key")),
        }
    }
}

#[derive(Clone)]
struct BearerGuard;

#[async_trait]
impl Guard<()> for BearerGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        match ctx.bearer_token()? {
            Some("secret-token") => Ok(()),
            Some(_) => Err(GuardError::unauthorized("invalid bearer token")),
            None => Err(GuardError::unauthorized("missing bearer token")),
        }
    }
}

#[test]
fn guard_context_header_helpers_return_utf8_values_and_missing_headers() {
    let context = GuardContext::new((), "admin:index").with_headers(headers([
        ("x-api-key", HeaderValue::from_static("secret")),
        (
            "authorization",
            HeaderValue::from_static("Bearer secret-token"),
        ),
    ]));

    assert_eq!(context.header_str("x-api-key").unwrap(), Some("secret"));
    assert_eq!(context.api_key("x-api-key").unwrap(), Some("secret"));
    assert_eq!(context.bearer_token().unwrap(), Some("secret-token"));
    assert_eq!(context.header_str("x-missing").unwrap(), None);
    assert_eq!(context.api_key("x-missing").unwrap(), None);
}

#[test]
fn guard_context_bearer_token_accepts_case_insensitive_scheme() {
    for scheme in ["Bearer", "bearer", "BEARER", "BeArEr"] {
        let value = format!("{scheme} secret-token");
        let context = GuardContext::new((), "admin:index").with_headers(headers([(
            "authorization",
            HeaderValue::from_str(&value).unwrap(),
        )]));

        assert_eq!(context.bearer_token().unwrap(), Some("secret-token"));
    }
}

#[test]
fn guard_context_header_helpers_reject_malformed_values() {
    let mut malformed_headers = HeaderMap::new();
    malformed_headers.insert(
        "x-api-key",
        HeaderValue::from_bytes(&[0xff]).expect("non-UTF-8 header value should be constructible"),
    );
    let context = GuardContext::new((), "admin:index").with_headers(malformed_headers);

    let error = context.header_str("x-api-key").unwrap_err();

    assert_eq!(error.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(error.reason(), "header value is not valid UTF-8");
}

#[test]
fn guard_context_bearer_token_rejects_malformed_authorization_headers() {
    for value in ["Basic secret-token", "Bearer", "Bearer "] {
        let context = GuardContext::new((), "admin:index").with_headers(headers([(
            "authorization",
            HeaderValue::from_static(value),
        )]));

        let error = context.bearer_token().unwrap_err();

        assert_eq!(error.status_code(), StatusCode::UNAUTHORIZED);
        assert_eq!(
            error.reason(),
            "authorization header must use `Bearer <token>`"
        );
    }
}

#[tokio::test]
async fn guard_layer_passes_request_headers_to_guard() {
    let app = Router::new()
        .route("/admin", get(|| async { "ok" }))
        .layer(guard_layer((), "admin:index", ApiKeyGuard));

    let denied = app
        .clone()
        .oneshot(
            http::Request::builder()
                .uri("/admin")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::UNAUTHORIZED);

    let allowed = app
        .oneshot(
            http::Request::builder()
                .uri("/admin")
                .header("x-api-key", "secret")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
    let body = to_bytes(allowed.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"ok");
}

#[tokio::test]
async fn bearer_token_guard_handles_valid_missing_malformed_and_unauthorized_headers() {
    let app = Router::new()
        .route("/me", get(|| async { "ok" }))
        .layer(guard_layer((), "me:read", BearerGuard));

    let allowed = app
        .clone()
        .oneshot(
            http::Request::builder()
                .uri("/me")
                .header("authorization", "Bearer secret-token")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);

    let lowercase_scheme = app
        .clone()
        .oneshot(
            http::Request::builder()
                .uri("/me")
                .header("authorization", "bearer secret-token")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(lowercase_scheme.status(), StatusCode::OK);

    let missing = app
        .clone()
        .oneshot(
            http::Request::builder()
                .uri("/me")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
    let missing_body = to_bytes(missing.into_body(), usize::MAX).await.unwrap();
    let missing_json: serde_json::Value = serde_json::from_slice(&missing_body).unwrap();
    assert_eq!(missing_json["error"]["message"], "missing bearer token");

    let malformed = app
        .clone()
        .oneshot(
            http::Request::builder()
                .uri("/me")
                .header("authorization", "Bearer")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::UNAUTHORIZED);
    let malformed_body = to_bytes(malformed.into_body(), usize::MAX).await.unwrap();
    let malformed_json: serde_json::Value = serde_json::from_slice(&malformed_body).unwrap();
    assert_eq!(
        malformed_json["error"]["message"],
        "authorization header must use `Bearer <token>`"
    );

    let unauthorized = app
        .oneshot(
            http::Request::builder()
                .uri("/me")
                .header("authorization", "Bearer wrong-token")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let unauthorized_body = to_bytes(unauthorized.into_body(), usize::MAX)
        .await
        .unwrap();
    let unauthorized_json: serde_json::Value = serde_json::from_slice(&unauthorized_body).unwrap();
    assert_eq!(
        unauthorized_json["error"]["message"],
        "invalid bearer token"
    );
}

#[tokio::test]
async fn guard_layer_returns_error_response_without_calling_inner_service_when_header_missing() {
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
        .layer(guard_layer((), "admin:index", ApiKeyGuard));

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
    assert_eq!(json["error"]["message"], "missing or invalid api key");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
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

fn headers<const N: usize>(values: [(&'static str, HeaderValue); N]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in values {
        headers.insert(name, value);
    }
    headers
}
