use async_trait::async_trait;
use axum::Router;
use nidus::prelude::{Controller, Guard, GuardContext, GuardError, RouteDefinition};

struct ApiKeyGuard;

#[async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        if ctx.route_label() == "profile" {
            Ok(())
        } else {
            Err(GuardError::forbidden("invalid route"))
        }
    }
}

fn app() -> Router {
    Controller::new("/")
        .route(RouteDefinition::get("/me", me))
        .into_router()
}

async fn me() -> Result<&'static str, GuardError> {
    ApiKeyGuard.check(GuardContext::new((), "profile")).await?;

    Ok("authorized")
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app()).await.unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;

    #[tokio::test]
    async fn guard_allows_profile_route() {
        ApiKeyGuard
            .check(GuardContext::new((), "profile"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn guard_rejects_unknown_route() {
        let error = ApiKeyGuard
            .check(GuardContext::new((), "admin"))
            .await
            .unwrap_err();

        assert_eq!(error.status_code(), axum::http::StatusCode::FORBIDDEN);
        assert_eq!(error.reason(), "invalid route");
    }

    #[tokio::test]
    async fn auth_route_uses_guard() {
        let response = TestApp::from_router(app()).get("/me").send().await;

        response.assert_status(axum::http::StatusCode::OK);
        response.assert_text("authorized").await;
    }
}
