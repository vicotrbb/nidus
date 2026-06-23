use async_trait::async_trait;
use http::StatusCode;
use nidus_auth::{Guard, GuardContext, GuardError};

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
    assert_eq!(error.reason(), "route role does not match");
    assert_eq!(error.to_string(), "route role does not match");
}

#[test]
fn guard_error_carries_typed_unauthorized_status() {
    let error = GuardError::unauthorized("missing token");

    assert_eq!(error.status_code(), StatusCode::UNAUTHORIZED);
    assert_eq!(error.reason(), "missing token");
}
