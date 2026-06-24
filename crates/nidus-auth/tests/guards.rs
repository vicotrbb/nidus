use async_trait::async_trait;
use axum::{body::to_bytes, response::IntoResponse};
use http::StatusCode;
use nidus_auth::{Guard, GuardContext, GuardError, GuardExt};

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

struct AllowGuard;

#[async_trait]
impl Guard<()> for AllowGuard {
    async fn check(&self, _ctx: GuardContext<()>) -> Result<(), GuardError> {
        Ok(())
    }
}

struct DenyGuard(&'static str);

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
async fn guard_error_maps_to_stable_json_response() {
    let response = GuardError::forbidden("route role does not match").into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["error"]["code"], "forbidden");
    assert_eq!(json["error"]["message"], "route role does not match");
}
