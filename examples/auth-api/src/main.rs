//! Guard-focused Nidus example API.

use async_trait::async_trait;
use axum::Router;
use nidus::prelude::{
    ApplicationHttpExt, Controller, Guard, GuardContext, GuardError, Nidus, RouteDefinition,
    guard_layer, module,
};

#[derive(Clone)]
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
    app_with_route_label("profile")
}

fn app_with_route_label(route_label: &'static str) -> Router {
    Controller::new("/")
        .route(RouteDefinition::get("/me", me))
        .into_router()
        .layer(guard_layer((), route_label, ApiKeyGuard))
}

async fn me() -> &'static str {
    "authorized"
}

#[nidus::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Nidus::bootstrap::<AppModule>()?
        .with_router(app())
        .listen("127.0.0.1:3000")
        .await?;
    Ok(())
}

#[module]
struct AppModule;

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

    #[tokio::test]
    async fn auth_route_rejects_guard_failures() {
        let response = TestApp::from_router(app_with_route_label("admin"))
            .get("/me")
            .send()
            .await;

        response.assert_status(axum::http::StatusCode::FORBIDDEN);
        response
            .assert_json(serde_json::json!({
                "error": {
                    "code": "forbidden",
                    "message": "invalid route"
                }
            }))
            .await;
    }
}
