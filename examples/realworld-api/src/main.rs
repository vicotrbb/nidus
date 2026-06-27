mod auth;
mod config;
mod db;
mod health;
mod modules;
mod observability;
mod ops;
mod projects;
mod tasks;
mod users;

use std::time::Duration;

use axum::http::HeaderValue;
use nidus::prelude::{
    ApiDefaults, HealthRegistry, HealthStatus, HttpApplication, Nidus, NidusApplicationExt,
    PrometheusMetrics, RequestIdConfig, RequestIdMode, cors_origin_layer, request_scope_layer,
};

use crate::{config::AppConfig, modules::AppModule};

#[nidus::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env();
    observability::init(&config);
    let bind_addr = config.bind_addr.clone();

    app(config).await?.listen(&bind_addr).await?;

    Ok(())
}

async fn app(config: AppConfig) -> nidus::prelude::Result<HttpApplication> {
    let metrics = PrometheusMetrics::new();
    let health = HealthRegistry::new()
        .live_check_sync("process", HealthStatus::up)
        .ready_check("database", || async { HealthStatus::up() })
        .timeout(Duration::from_secs(1));
    let allowed_origin = HeaderValue::from_str(&config.allowed_origin).map_err(|error| {
        nidus::prelude::NidusError::ApplicationBuild {
            message: format!("invalid allowed CORS origin: {error}"),
        }
    })?;
    let request_scope = ops::request_scope_container()?;
    let ops_router = ops::router(&config, &metrics);

    Nidus::create::<AppModule>()
        .with_singleton(config)?
        .with_openapi("Nidus Real-World Team Tasks API", "0.1.0")
        .with_tracing()
        .build()
        .await
        .map(|app| {
            app.map_router(|router| {
                let router = router
                    .merge(ops_router)
                    .layer(request_scope_layer(request_scope));
                ApiDefaults::production("nidus-realworld-api")
                    .version(env!("CARGO_PKG_VERSION"))
                    .environment("example")
                    .metrics(metrics)
                    .health(health)
                    .request_ids(RequestIdConfig::production().mode(RequestIdMode::Strict))
                    .body_limit(1024 * 1024)
                    .timeout(Duration::from_millis(250))
                    .apply(router)
                    .layer(cors_origin_layer(allowed_origin))
            })
        })
}

#[cfg(test)]
mod tests {
    use nidus::prelude::StatusCode;
    use nidus_testing::TestApp;
    use serde_json::{Value, json};

    use super::*;
    use crate::{
        auth::guard::{ApiKeyGuard, unauthorized_status},
        users::CreateUserDto,
    };

    async fn test_app() -> TestApp {
        TestApp::from_router(app(AppConfig::test()).await.unwrap().into_router())
    }

    fn valid_request_id() -> &'static str {
        "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
    }

    async fn create_user(app: &TestApp) {
        app.post("/users")
            .json(&json!({
                "email": "owner@nidus.dev",
                "display_name": "Owner"
            }))
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "email": "owner@nidus.dev",
                "display_name": "Owner"
            }));
    }

    async fn create_project(app: &TestApp) {
        app.post("/projects")
            .header("x-api-key", "dev-secret")
            .json(&json!({
                "owner_id": 1,
                "name": "Launch API"
            }))
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "owner_id": 1,
                "name": "Launch API"
            }));
    }

    #[test]
    fn root_module_recursively_drives_application_boundaries() {
        let graph = nidus::prelude::ModuleGraph::from_root::<AppModule>().unwrap();

        assert_eq!(
            graph.get("AppModule").unwrap().imports(),
            [
                "DatabaseModule",
                "AuthModule",
                "UsersModule",
                "ProjectsModule"
            ]
        );
        assert_eq!(
            graph.get("AppModule").unwrap().controllers(),
            ["HealthController"]
        );
        assert_eq!(
            graph.get("ProjectsModule").unwrap().providers(),
            [
                "ProjectsRepository",
                "ProjectsService",
                "TasksRepository",
                "TasksService"
            ]
        );
        assert_eq!(
            graph.get("UsersModule").unwrap().controllers(),
            ["UsersController"]
        );
        assert_eq!(
            graph.get("ProjectsModule").unwrap().controllers(),
            ["ProjectsController", "TasksController"]
        );
    }

    #[tokio::test]
    async fn composed_container_resolves_real_dependency_flow() {
        let app = app(AppConfig::test()).await.unwrap();
        let users = app
            .application()
            .container()
            .resolve::<crate::users::UsersService>()
            .unwrap();
        let user = users
            .create_user(CreateUserDto {
                email: "di@nidus.dev".to_owned(),
                display_name: "DI User".to_owned(),
            })
            .await
            .unwrap();

        assert_eq!(user.email, "di@nidus.dev");
    }

    #[tokio::test]
    async fn health_and_user_routes_are_public() {
        let app = test_app().await;

        app.get("/health")
            .send()
            .await
            .assert_json(json!({ "status": "ok" }));

        create_user(&app).await;

        app.get("/users/1").send().await.assert_json(json!({
            "id": 1,
            "email": "owner@nidus.dev",
            "display_name": "Owner"
        }));
    }

    #[tokio::test]
    async fn project_and_task_routes_require_api_key_and_persist_data() {
        let app = test_app().await;
        create_user(&app).await;

        let rejected = app.get("/projects/1").send().await;
        rejected.assert_status(unauthorized_status());
        let body: Value = rejected.json();
        assert_eq!(body["error"]["statusCode"], 401);
        assert_eq!(body["error"]["code"], "unauthorized");
        assert_eq!(body["error"]["message"], "missing or invalid x-api-key");
        assert_eq!(body["error"]["path"], "/projects/1");

        create_project(&app).await;

        app.post("/projects/1/tasks")
            .header("x-api-key", "dev-secret")
            .json(&json!({
                "title": "Write docs",
                "description": "Document the real-world example"
            }))
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "project_id": 1,
                "title": "Write docs",
                "description": "Document the real-world example",
                "completed": false
            }));

        app.patch("/tasks/1/complete")
            .header("x-api-key", "dev-secret")
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "project_id": 1,
                "title": "Write docs",
                "description": "Document the real-world example",
                "completed": true
            }));

        app.get("/projects/1/tasks")
            .header("x-api-key", "dev-secret")
            .query(&json!({ "completed": true }))
            .send()
            .await
            .assert_json(json!([
                {
                    "id": 1,
                    "project_id": 1,
                    "title": "Write docs",
                    "description": "Document the real-world example",
                    "completed": true
                }
            ]));
    }

    #[tokio::test]
    async fn validation_rejections_are_stable() {
        let app = test_app().await;

        let response = app
            .post("/users")
            .json(&json!({
                "email": "not-an-email",
                "display_name": ""
            }))
            .send()
            .await;

        response.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value = response.json();
        assert_eq!(body["error"]["code"], "validation_failed");
        assert_eq!(body["error"]["message"], "request validation failed");
        assert_eq!(
            body["error"]["details"]["fields"][0]["field"],
            "display_name"
        );
        assert_eq!(body["error"]["details"]["fields"][1]["field"], "email");
    }

    #[tokio::test]
    async fn guard_metadata_is_executable_through_composed_router() {
        let app = test_app().await;
        create_user(&app).await;

        assert_eq!(
            crate::projects::ProjectsController::routes()[0].guards(),
            ["ApiKeyGuard"]
        );
        assert!(
            crate::tasks::TasksController::routes()
                .iter()
                .all(|route| route.guards() == ["ApiKeyGuard"])
        );

        let rejected = app
            .post("/projects")
            .header("x-api-key", "wrong")
            .json(&json!({
                "owner_id": 1,
                "name": "Launch API"
            }))
            .send()
            .await;
        rejected.assert_status(unauthorized_status());

        let guard = app
            .get("/projects/1")
            .header("x-api-key", "dev-secret")
            .send()
            .await;
        guard.assert_status(StatusCode::NOT_FOUND);

        let auth_guard = app
            .post("/projects")
            .header("x-api-key", "dev-secret")
            .json(&json!({
                "owner_id": 1,
                "name": "Launch API"
            }))
            .send()
            .await;
        auth_guard.assert_status(StatusCode::CREATED);

        let _ = std::any::TypeId::of::<ApiKeyGuard>();
    }

    #[tokio::test]
    async fn openapi_document_is_served_with_core_metadata() {
        let app = test_app().await;
        let response = app.get("/openapi.json").send().await;
        response.assert_status(StatusCode::OK);

        let body: Value = response.json();
        assert_eq!(body["info"]["title"], "Nidus Real-World Team Tasks API");
        assert_eq!(body["paths"]["/health"]["get"]["summary"], "Health check");
        assert_eq!(
            body["paths"]["/projects/{project_id}/tasks"]["post"]["summary"],
            "Create task"
        );
        assert_eq!(
            body["paths"]["/tasks/{id}/complete"]["patch"]["summary"],
            "Complete task"
        );
        assert_eq!(
            body["paths"]["/projects"]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/CreateProjectDto"
        );
        assert!(body["components"]["schemas"]["TaskDto"].is_object());
        assert!(body["components"]["schemas"]["CreateTaskDto"].is_object());
        assert!(body["components"]["schemas"]["UserDto"].is_object());
        assert!(body["components"]["schemas"]["CreateUserDto"].is_object());
        assert!(body["components"]["schemas"]["ProjectDto"].is_object());
        assert!(body["components"]["schemas"]["HealthDto"].is_object());

        let docs = app.get("/docs").send().await;
        docs.assert_status(StatusCode::OK);
    }

    #[tokio::test]
    async fn production_request_ids_are_strict_and_visible_to_handlers() {
        let app = test_app().await;

        let accepted = app
            .get("/context")
            .header("x-request-id", valid_request_id())
            .header("x-correlation-id", "corr-realworld")
            .header("x-api-key", "dev-secret")
            .send()
            .await;
        accepted.assert_status(StatusCode::OK);
        accepted.assert_header("x-request-id", valid_request_id());
        let body: Value = accepted.json();
        assert_eq!(body["request_id"], valid_request_id());
        assert_eq!(body["correlation_id"], "corr-realworld");
        assert_eq!(body["client_kind"], "api_key");
        assert_eq!(body["scoped_request_number"], 0);

        let rejected = app
            .get("/context")
            .header("x-request-id", "not-a-uuid")
            .send()
            .await;
        rejected.assert_status(StatusCode::BAD_REQUEST);
        let body: Value = rejected.json();
        assert_eq!(body["error"]["code"], "invalid_request_id");
        assert_eq!(body["error"]["path"], "/context");
        assert!(body["error"]["requestId"].as_str().is_some());
    }

    #[tokio::test]
    async fn production_health_metrics_errors_and_security_headers_are_wired() {
        let app = test_app().await;

        app.get("/health/live").send().await.assert_json(json!({
            "status": "up",
            "checks": {
                "process": { "status": "up" }
            }
        }));

        app.get("/health/ready").send().await.assert_json(json!({
            "status": "up",
            "checks": {
                "database": { "status": "up" }
            }
        }));

        let domain_404 = app
            .get("/projects/404")
            .header("x-api-key", "dev-secret")
            .header("x-request-id", valid_request_id())
            .send()
            .await;
        domain_404.assert_status(StatusCode::NOT_FOUND);
        domain_404.assert_header("x-content-type-options", "nosniff");
        let body: Value = domain_404.json();
        assert_eq!(body["error"]["statusCode"], 404);
        assert_eq!(body["error"]["code"], "not_found");
        assert_eq!(body["error"]["message"], "project not found");
        assert_eq!(body["error"]["path"], "/projects/404");
        assert_eq!(body["error"]["requestId"], valid_request_id());

        let masked_500 = app
            .get("/ops/fail")
            .header("x-request-id", valid_request_id())
            .send()
            .await;
        masked_500.assert_status(StatusCode::INTERNAL_SERVER_ERROR);
        let body: Value = masked_500.json();
        assert_eq!(body["error"]["message"], "internal server error");
        assert_eq!(body["error"]["path"], "/ops/fail");

        app.get("/metrics")
            .send()
            .await
            .assert_status(StatusCode::OK);
        let metrics = app.get("/metrics").send().await;
        let text = metrics.text().unwrap();
        assert!(text.contains(r#"route="/projects/{id}""#), "{text}");
        assert!(text.contains(r#"status="404""#), "{text}");
        assert!(!text.contains(r#"route="/metrics""#), "{text}");
    }

    #[tokio::test]
    async fn production_rate_limit_body_limit_timeout_and_cors_are_deterministic() {
        let app = test_app().await;

        let first = app
            .get("/ops/limited")
            .header("x-api-key", "rate-key")
            .send()
            .await;
        first.assert_status(StatusCode::OK);
        first.assert_header("ratelimit-limit", "1");
        first.assert_header("ratelimit-remaining", "0");

        let second = app
            .get("/ops/limited")
            .header("x-api-key", "rate-key")
            .send()
            .await;
        second.assert_status(StatusCode::TOO_MANY_REQUESTS);
        second.assert_header("ratelimit-limit", "1");
        assert!(second.header("retry-after").is_some());

        let oversized = app
            .post("/ops/webhook")
            .header("content-length", "33")
            .body("012345678901234567890123456789012")
            .send()
            .await;
        oversized.assert_status(StatusCode::PAYLOAD_TOO_LARGE);
        oversized.assert_header("x-nidus-body-limit", "webhook-raw-body");

        app.get("/ops/timeout")
            .send()
            .await
            .assert_status(StatusCode::REQUEST_TIMEOUT);

        let cors = app
            .request(axum::http::Method::OPTIONS, "/context")
            .header("origin", "https://console.nidus.dev")
            .header("access-control-request-method", "GET")
            .send()
            .await;
        cors.assert_status(StatusCode::OK);
        cors.assert_header("access-control-allow-origin", "https://console.nidus.dev");
    }

    #[tokio::test]
    async fn observed_events_and_jobs_receive_context() {
        let app = test_app().await;

        let response = app
            .post("/ops/workflows/observed")
            .header("x-request-id", valid_request_id())
            .send()
            .await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();

        assert_eq!(body["event"]["operation_id"], "event-run-1");
        assert_eq!(body["event"]["event_name"], "task.completed");
        assert_eq!(
            body["event"]["attributes"]["request_id"],
            valid_request_id()
        );
        assert_eq!(body["sync_job"]["started"]["run_id"], "job-run-1");
        assert_eq!(body["sync_job"]["finished"]["status"], "success");
        assert_eq!(body["async_job"]["started"]["run_id"], "job-run-1");
        assert_eq!(body["async_job"]["finished"]["status"], "success");
        assert_eq!(body["queue"]["sync_completed"], json!(["send_task_digest"]));
        assert_eq!(
            body["queue"]["async_completed"],
            json!(["refresh_project_summary"])
        );
    }

    #[test]
    fn config_logging_and_otel_helpers_are_part_of_the_example_surface() {
        let config = AppConfig::from_nidus_config(
            nidus::prelude::Config::from_pairs([
                ("api_key", "configured-secret"),
                ("bind_addr", "127.0.0.1:4000"),
                ("database_url", "sqlite::memory:"),
                ("allowed_origin", "https://console.nidus.dev"),
            ])
            .deserialize()
            .unwrap(),
        );
        assert_eq!(config.api_key, "configured-secret");
        assert_eq!(config.allowed_origin, "https://console.nidus.dev");

        let logging = observability::logging_config(&config);
        assert_eq!(logging.format(), nidus::prelude::LoggingFormat::Pretty);
        assert!(logging.redacts_header("x-api-key"));

        let mut headers = nidus::prelude::HeaderMap::new();
        let trace = nidus::prelude::TraceContext::new(
            "4bf92f3577b34da6a3ce929d0e0e4736",
            "00f067aa0ba902b7",
            true,
        );
        nidus::prelude::inject_trace_context(&mut headers, &trace);
        let extracted = nidus::prelude::extract_trace_context(&headers).unwrap();
        assert_eq!(extracted.trace_id(), trace.trace_id());
    }
}
