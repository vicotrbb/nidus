//! Production API defaults example.

use std::time::Duration;

use axum::routing::get as axum_get;
use nidus::prelude::*;
use serde::Serialize;

#[derive(Serialize)]
struct UserDto {
    id: i64,
    email: &'static str,
    request_id: String,
}

fn app() -> Router {
    let observability = Observability::production("nidus-production-api")
        .version(env!("CARGO_PKG_VERSION"))
        .environment("example")
        .prometheus()
        .tracing();
    let health = HealthRegistry::new()
        .live_check_sync("process", HealthStatus::up)
        .ready_check("database", || async { HealthStatus::up() })
        .ready_check("cache", || async { HealthStatus::up() })
        .hide_details();
    let limited = Router::new()
        .route("/limited", axum_get(|| async { "limited ok" }))
        .layer(
            RateLimitConfig::new(1, Duration::from_secs(60), InMemoryRateLimitStore::new())
                .identity(client_ip_identity())
                .fail_closed()
                .layer(),
        );
    let slow = Router::new().route(
        "/slow",
        axum_get(|| async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            "slow ok"
        }),
    );

    ApiDefaults::production("nidus-production-api")
        .health(health)
        .observability(&observability)
        .request_ids(RequestIdConfig::production().mode(RequestIdMode::Strict))
        .timeout(Duration::from_millis(50))
        .apply(
            UsersController
                .into_router()
                .merge(limited)
                .merge(slow)
                .merge(observability.routes()),
        )
}

#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn find_user(&self, Path(id): Path<i64>, context: RequestContext) -> Json<UserDto> {
        Json(UserDto {
            id,
            email: "user@nidus.dev",
            request_id: context.request_id().to_owned(),
        })
    }

    #[get("/domain-error")]
    async fn domain_error(&self) -> HttpError {
        HttpError::not_found("user not found")
    }

    #[get("/internal-error")]
    async fn internal_error(&self) -> HttpError {
        HttpError::internal_server_error()
    }
}

#[nidus::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let _ = LoggingConfig::development("nidus-production-api")
        .environment("local")
        .init();
    let address = std::env::var("NIDUS_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    Nidus::create::<AppModule>()
        .build_with_router(app())
        .await?
        .listen(address)
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
    async fn production_api_includes_request_id_in_responses() {
        let response = TestApp::from_router(app())
            .get("/users/42")
            .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
            .send()
            .await;

        response.assert_status(StatusCode::OK);
        response.assert_header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78");
    }

    #[tokio::test]
    async fn production_api_exposes_health_and_metrics() {
        let app = TestApp::from_router(app());
        app.get("/health/live")
            .send()
            .await
            .assert_status(StatusCode::OK);
        app.get("/health/ready")
            .send()
            .await
            .assert_status(StatusCode::OK);
        app.get("/metrics")
            .send()
            .await
            .assert_status(StatusCode::OK);
    }

    #[tokio::test]
    async fn production_api_maps_slow_routes_to_timeout() {
        TestApp::from_router(app())
            .get("/slow")
            .send()
            .await
            .assert_status(StatusCode::REQUEST_TIMEOUT);
    }
}
