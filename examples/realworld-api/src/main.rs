mod auth;
mod config;
mod db;
mod health;
mod modules;
mod observability;
mod projects;
mod tasks;
mod users;

use nidus::prelude::{HttpApplication, Nidus, NidusApplicationExt};

use crate::{config::AppConfig, modules::AppModule};

#[nidus::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init();
    let config = AppConfig::from_env();
    let bind_addr = config.bind_addr.clone();

    app(config).await?.listen(&bind_addr).await?;

    Ok(())
}

async fn app(config: AppConfig) -> nidus::prelude::Result<HttpApplication> {
    Nidus::create::<AppModule>()
        .with_singleton(config)?
        .with_openapi("Nidus Real-World Team Tasks API", "0.1.0")
        .with_tracing()
        .build()
        .await
}

#[cfg(test)]
mod tests {
    use nidus::prelude::StatusCode;
    use nidus_testing::TestApp;
    use serde_json::{Value, json};

    use super::*;
    use crate::auth::guard::{ApiKeyGuard, unauthorized_status};

    async fn test_app() -> TestApp {
        TestApp::from_router(app(AppConfig::test()).await.unwrap().into_router())
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
            }))
            .await;
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
            }))
            .await;
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
            .assert_json(json!({ "status": "ok" }))
            .await;

        create_user(&app).await;

        app.get("/users/1")
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "email": "owner@nidus.dev",
                "display_name": "Owner"
            }))
            .await;
    }

    #[tokio::test]
    async fn project_and_task_routes_require_api_key_and_persist_data() {
        let app = test_app().await;
        create_user(&app).await;

        let rejected = app.get("/projects/1").send().await;
        rejected.assert_status(unauthorized_status());
        rejected
            .assert_json(json!({
                "error": {
                    "code": "unauthorized",
                    "message": "missing or invalid x-api-key"
                }
            }))
            .await;

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
            }))
            .await;

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
            }))
            .await;

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
            ]))
            .await;
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
        assert_eq!(body["error"]["fields"][0]["field"], "display_name");
        assert_eq!(body["error"]["fields"][1]["field"], "email");
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
}
