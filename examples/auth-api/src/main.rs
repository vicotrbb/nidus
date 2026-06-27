//! Guard-focused Nidus example API.
//!
//! [`ApiKeyGuard`] reads the `x-api-key` header and authorizes only when it
//! matches the expected key. It is wired through the public [`guard_layer`],
//! which (since the header-passing fix) populates the guard context with the
//! request headers — so this is a genuine header-token guard, not a route-label
//! check.

use async_trait::async_trait;
use axum::Router;
use nidus::prelude::{
    ApplicationHttpExt, Controller, Guard, GuardContext, GuardError, Nidus, RouteDefinition,
    guard_layer, module,
};

/// The expected API key for this example. In a real app this would come from
/// configuration or a secret store, not a hardcoded constant.
const EXPECTED_API_KEY: &str = "nidus-dev-secret";

#[derive(Clone)]
struct ApiKeyGuard;

#[async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        match ctx
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
        {
            Some(key) if key == EXPECTED_API_KEY => Ok(()),
            _ => Err(GuardError::unauthorized("missing or invalid x-api-key")),
        }
    }
}

fn app() -> Router {
    Controller::new("/")
        .route(RouteDefinition::get("/me", me))
        .into_router()
        .layer(guard_layer((), "profile", ApiKeyGuard))
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
    use axum::http::{HeaderMap, HeaderValue};
    use nidus_testing::TestApp;

    fn context_with_api_key(key: Option<&str>) -> GuardContext<()> {
        let mut headers = HeaderMap::new();
        if let Some(key) = key {
            headers.insert("x-api-key", HeaderValue::from_str(key).unwrap());
        }
        GuardContext::new((), "profile").with_headers(headers)
    }

    #[tokio::test]
    async fn guard_allows_valid_api_key() {
        ApiKeyGuard
            .check(context_with_api_key(Some(EXPECTED_API_KEY)))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn guard_rejects_missing_api_key() {
        let error = ApiKeyGuard
            .check(context_with_api_key(None))
            .await
            .unwrap_err();

        assert_eq!(error.status_code(), axum::http::StatusCode::UNAUTHORIZED);
        assert_eq!(error.reason(), "missing or invalid x-api-key");
    }

    #[tokio::test]
    async fn guard_rejects_wrong_api_key() {
        let error = ApiKeyGuard
            .check(context_with_api_key(Some("wrong")))
            .await
            .unwrap_err();

        assert_eq!(error.status_code(), axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_route_allows_request_with_valid_key() {
        let response = TestApp::from_router(app())
            .get("/me")
            .header("x-api-key", EXPECTED_API_KEY)
            .send()
            .await;

        response.assert_status(axum::http::StatusCode::OK);
        response.assert_text("authorized");
    }

    #[tokio::test]
    async fn auth_route_rejects_request_without_key() {
        let response = TestApp::from_router(app()).get("/me").send().await;

        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
        response.assert_json(serde_json::json!({
            "error": {
                "code": "unauthorized",
                "message": "missing or invalid x-api-key"
            }
        }));
    }

    #[tokio::test]
    async fn auth_route_rejects_request_with_wrong_key() {
        let response = TestApp::from_router(app())
            .get("/me")
            .header("x-api-key", "wrong")
            .send()
            .await;

        response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
    }
}
