use async_trait::async_trait;
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

    assert_eq!(error.status_code(), 403);
    assert!(error.to_string().contains("route role"));
}
