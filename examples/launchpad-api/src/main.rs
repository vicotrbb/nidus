use std::{
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use async_trait::async_trait;
use axum::{
    body::Bytes,
    http::HeaderValue,
    routing::{get as axum_get, post as axum_post},
};
use garde::Validate;
use nidus::prelude::{
    ApiDefaults, AsyncJob, AsyncJobQueue, Config, Container, EventBus, EventObserver, Guard,
    GuardContext, GuardError, HealthRegistry, HealthStatus, HttpApplication, HttpError, Inject,
    Job, JobObserver, JobQueue, JobResultStatus, Json, Module, ModuleBuilder, ModuleDefinition,
    Nidus, NidusApplicationExt, NidusError, Observability, ObservedEventBus, ObservedEventContext,
    ObservedJobContext, ObservedJobRunner, Path, RequestContext, RequestIdConfig, Router,
    StatusCode, ValidatedJson, controller, cors_origin_layer, get, guard, injectable, module,
    openapi, patch, post, request_context_layer, request_scope_layer, routes, validate,
    webhook_body_limit_layer,
};
use nidus_cache::{CacheConfig, MokaCacheProvider};
use nidus_sqlx::SqlitePoolProvider;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Deserialize)]
struct AppConfig {
    bind_addr: String,
    api_key: String,
    database_url: String,
}

impl AppConfig {
    fn from_env() -> nidus::prelude::Result<Self> {
        let config = Config::from_pairs([
            ("bind_addr", "127.0.0.1:4100"),
            ("api_key", "launch-secret"),
            ("database_url", "sqlite::memory:"),
        ])
        .merge(Config::from_env_prefix("LAUNCHPAD"));
        config
            .deserialize()
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })
    }

    #[cfg(test)]
    fn test() -> Self {
        Self {
            bind_addr: "127.0.0.1:0".to_owned(),
            api_key: "launch-secret".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        }
    }
}

#[nidus::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env()?;
    let bind_addr = config.bind_addr.clone();
    app(config).await?.listen(&bind_addr).await?;
    Ok(())
}

async fn app(config: AppConfig) -> nidus::prelude::Result<HttpApplication> {
    let observability = Observability::production("nidus-launchpad-api")
        .version(env!("CARGO_PKG_VERSION"))
        .environment("example")
        .prometheus()
        .tracing()
        .otel_from_env();
    let health = HealthRegistry::new()
        .live_check_sync("process", HealthStatus::up)
        .ready_check("sqlite", || async { HealthStatus::up() })
        .ready_check_sync("cache", HealthStatus::up);

    Nidus::create::<AppModule>()
        .with_singleton(config)?
        .with_singleton(observability.clone())?
        .with_openapi("Nidus Launchpad API", "1.0.0")
        .with_schema::<WorkflowDto>()
        .with_observability(observability)
        .build()
        .await
        .map(|app| {
            app.map_router(|router| {
                let router = router
                    .merge(ops_router())
                    .layer(request_context_layer())
                    .layer(request_scope_layer(Arc::new(Container::new())));
                ApiDefaults::production("nidus-launchpad-api")
                    .version(env!("CARGO_PKG_VERSION"))
                    .environment("example")
                    .health(health)
                    .request_ids(RequestIdConfig::development())
                    .body_limit(1024 * 1024)
                    .timeout(Duration::from_millis(500))
                    .apply(router)
                    .layer(cors_origin_layer(HeaderValue::from_static(
                        "http://localhost:4100",
                    )))
            })
        })
}

fn ops_router() -> Router {
    Router::new()
        .route("/ops/context", axum_get(context))
        .route(
            "/ops/webhook",
            axum_post(webhook).layer(webhook_body_limit_layer(32)),
        )
}

async fn context(context: RequestContext) -> Json<RequestContextDto> {
    Json(RequestContextDto {
        request_id: context.request_id().to_owned(),
        method: context.method().to_string(),
        path: context.path().to_owned(),
        client_kind: context.client_kind().as_str().to_owned(),
    })
}

async fn webhook(body: Bytes) -> Json<WebhookDto> {
    Json(WebhookDto {
        received_bytes: body.len(),
    })
}

#[derive(Serialize, ToSchema)]
struct RequestContextDto {
    request_id: String,
    method: String,
    path: String,
    client_kind: String,
}

#[derive(Serialize, ToSchema)]
struct WebhookDto {
    received_bytes: usize,
}

#[module]
struct AppModule {
    imports: (
        InfrastructureModule,
        AuthModule,
        LaunchesModule,
        WorkflowModule,
    ),
    controllers: [HealthController],
}

struct InfrastructureModule;

impl Module for InfrastructureModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("InfrastructureModule")
            .provider("SqlitePoolProvider")
            .provider("MokaCacheProvider")
            .export_typed::<SqlitePoolProvider>()
            .export_typed::<MokaCacheProvider>()
            .async_initializer(initialize_infrastructure)
            .build()
    }
}

fn initialize_infrastructure(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = nidus::prelude::Result<()>> + Send + '_>> {
    Box::pin(async move {
        let config = container.resolve::<AppConfig>()?;
        let observability = container.resolve::<Observability>()?;
        SqlitePoolProvider::builder()
            .database_url(&config.database_url)
            .max_connections(1)
            .observability(observability.adapter_observer())
            .register(container)
            .await
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })?;
        MokaCacheProvider::builder()
            .config(
                CacheConfig::new()
                    .namespace("launchpad")
                    .max_capacity(1_000)
                    .time_to_live(Duration::from_secs(60)),
            )
            .observability(observability.adapter_observer())
            .register(container)
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })?;

        let database = container.resolve::<SqlitePoolProvider>()?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS launches (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                owner_email TEXT NOT NULL,
                status TEXT NOT NULL
            )
            "#,
        )
        .execute(database.pool())
        .await
        .map_err(db_error)?;
        Ok(())
    })
}

#[module]
struct AuthModule {
    providers: (AuthService, ApiKeyGuard),
    exports: (AuthService, ApiKeyGuard),
}

#[injectable]
#[derive(Clone, Debug)]
struct AuthService {
    config: Inject<AppConfig>,
}

impl AuthService {
    fn accepts(&self, api_key: &str) -> bool {
        self.config.api_key == api_key
    }
}

#[injectable]
#[derive(Clone, Debug)]
struct ApiKeyGuard {
    auth: Inject<AuthService>,
}

#[async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        let Some(api_key) = ctx
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
        else {
            return Err(GuardError::unauthorized("missing or invalid x-api-key"));
        };

        if self.auth.accepts(api_key) {
            Ok(())
        } else {
            Err(GuardError::unauthorized("missing or invalid x-api-key"))
        }
    }
}

#[module]
struct LaunchesModule {
    providers: (LaunchRepository, LaunchService),
    controllers: [LaunchController],
    exports: [LaunchService],
}

#[injectable]
#[derive(Clone)]
struct LaunchRepository {
    database: Inject<SqlitePoolProvider>,
    cache: Inject<MokaCacheProvider>,
}

impl LaunchRepository {
    async fn create(&self, input: CreateLaunchDto) -> Result<LaunchDto, HttpError> {
        let row = sqlx::query_as::<_, (i64, String, String, String)>(
            r#"
            INSERT INTO launches (name, owner_email, status)
            VALUES (?1, ?2, 'queued')
            RETURNING id, name, owner_email, status
            "#,
        )
        .bind(input.name)
        .bind(input.owner_email)
        .fetch_one(self.database.pool())
        .await
        .map_err(map_db_error)?;
        let launch = LaunchDto::from(row);
        self.cache_launch(&launch).await?;
        Ok(launch)
    }

    async fn find(&self, id: i64) -> Result<LaunchDto, HttpError> {
        if let Some(cached) = self.cache.get(cache_key(id)).await {
            return serde_json::from_slice(&cached).map_err(|error| {
                HttpError::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "cache_decode_failed",
                    error.to_string(),
                )
            });
        }

        let row = sqlx::query_as::<_, (i64, String, String, String)>(
            "SELECT id, name, owner_email, status FROM launches WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(self.database.pool())
        .await
        .map_err(map_db_error)?
        .ok_or_else(|| HttpError::not_found("launch not found"))?;
        let launch = LaunchDto::from(row);
        self.cache_launch(&launch).await?;
        Ok(launch)
    }

    async fn update_status(&self, id: i64, status: &str) -> Result<LaunchDto, HttpError> {
        let row = sqlx::query_as::<_, (i64, String, String, String)>(
            r#"
            UPDATE launches
            SET status = ?2
            WHERE id = ?1
            RETURNING id, name, owner_email, status
            "#,
        )
        .bind(id)
        .bind(status)
        .fetch_optional(self.database.pool())
        .await
        .map_err(map_db_error)?
        .ok_or_else(|| HttpError::not_found("launch not found"))?;
        let launch = LaunchDto::from(row);
        self.cache.invalidate(cache_key(id)).await;
        self.cache_launch(&launch).await?;
        Ok(launch)
    }

    async fn cache_launch(&self, launch: &LaunchDto) -> Result<(), HttpError> {
        let bytes = serde_json::to_vec(launch).map_err(|error| {
            HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "cache_encode_failed",
                error.to_string(),
            )
        })?;
        self.cache.insert(cache_key(launch.id), bytes).await;
        Ok(())
    }
}

#[injectable]
#[derive(Clone)]
struct LaunchService {
    repository: Inject<LaunchRepository>,
}

impl LaunchService {
    async fn create(&self, input: CreateLaunchDto) -> Result<LaunchDto, HttpError> {
        self.repository.create(input).await
    }

    async fn find(&self, id: i64) -> Result<LaunchDto, HttpError> {
        self.repository.find(id).await
    }

    async fn mark_ready(&self, id: i64) -> Result<LaunchDto, HttpError> {
        self.repository.update_status(id, "ready").await
    }
}

#[controller("/launches")]
struct LaunchController {
    service: Inject<LaunchService>,
}

#[routes]
impl LaunchController {
    #[post("/")]
    #[guard(ApiKeyGuard)]
    #[validate]
    #[openapi(
        summary = "Create launch",
        tags = ["launches"],
        status = 201,
        request = CreateLaunchDto,
        response = LaunchDto
    )]
    async fn create(
        &self,
        ValidatedJson(input): ValidatedJson<CreateLaunchDto>,
    ) -> Result<(StatusCode, Json<LaunchDto>), HttpError> {
        Ok((StatusCode::CREATED, Json(self.service.create(input).await?)))
    }

    #[get("/:id")]
    #[guard(ApiKeyGuard)]
    #[openapi(
        summary = "Find launch",
        tags = ["launches"],
        status = 200,
        response = LaunchDto
    )]
    async fn find(&self, Path(id): Path<i64>) -> Result<Json<LaunchDto>, HttpError> {
        Ok(Json(self.service.find(id).await?))
    }

    #[patch("/:id/ready")]
    #[guard(ApiKeyGuard)]
    #[openapi(
        summary = "Mark launch ready",
        tags = ["launches"],
        status = 200,
        response = LaunchDto
    )]
    async fn ready(&self, Path(id): Path<i64>) -> Result<Json<LaunchDto>, HttpError> {
        Ok(Json(self.service.mark_ready(id).await?))
    }
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
struct CreateLaunchDto {
    #[garde(length(min = 1, max = 80))]
    name: String,
    #[garde(email)]
    owner_email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
struct LaunchDto {
    id: i64,
    name: String,
    owner_email: String,
    status: String,
}

impl From<(i64, String, String, String)> for LaunchDto {
    fn from((id, name, owner_email, status): (i64, String, String, String)) -> Self {
        Self {
            id,
            name,
            owner_email,
            status,
        }
    }
}

#[module]
struct WorkflowModule {
    controllers: [WorkflowController],
}

#[controller("/ops")]
struct WorkflowController;

#[routes]
impl WorkflowController {
    #[post("/workflow")]
    #[guard(ApiKeyGuard)]
    #[openapi(
        summary = "Run observed launch workflow",
        tags = ["ops"],
        status = 200,
        response = WorkflowDto
    )]
    async fn run(&self, context: RequestContext) -> Result<Json<WorkflowDto>, HttpError> {
        run_observed_workflow(context.request_id()).await.map(Json)
    }
}

async fn run_observed_workflow(request_id: &str) -> Result<WorkflowDto, HttpError> {
    let event_observer = CapturingEventObserver::default();
    let bus = EventBus::new();
    let subscriber = bus.subscribe();
    let observed_bus = ObservedEventBus::new(bus, event_observer.clone())
        .context("request_id", request_id)
        .context("service", "launchpad-api")
        .operation_id_generator(|| "event-run-1".to_owned());
    observed_bus.publish_named("launch.ready", LaunchReadyEvent { launch_id: 1 });

    let mut sync_queue = JobQueue::new();
    sync_queue.push(SendLaunchDigest);
    let sync_report = sync_queue.run_all();

    let mut async_queue = AsyncJobQueue::new();
    async_queue.push(WarmLaunchCache);
    let async_report = async_queue.run_all().await;

    let observer = CapturingJobObserver::default();
    let runner = ObservedJobRunner::new(observer.clone())
        .context("request_id", request_id)
        .run_id_generator(|| "job-run-1".to_owned());
    runner.run(&SendLaunchDigest).map_err(map_job_error)?;
    runner
        .run_async(&WarmLaunchCache)
        .await
        .map_err(map_job_error)?;

    Ok(WorkflowDto {
        event: event_observer.first()?,
        event_payloads: subscriber
            .drain()
            .into_iter()
            .map(|event| event.launch_id)
            .collect(),
        job_runs: observer.finished(),
        queue: QueueDto {
            sync_completed: sync_report.completed().to_vec(),
            async_completed: async_report.completed().to_vec(),
        },
    })
}

#[derive(Clone)]
struct LaunchReadyEvent {
    launch_id: i64,
}

#[derive(Clone, Copy)]
struct SendLaunchDigest;

impl Job for SendLaunchDigest {
    fn name(&self) -> &'static str {
        "send_launch_digest"
    }

    fn run(&self) -> Result<(), nidus::prelude::JobError> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct WarmLaunchCache;

#[async_trait]
impl AsyncJob for WarmLaunchCache {
    fn name(&self) -> &'static str {
        "warm_launch_cache"
    }

    async fn run(&self) -> Result<(), nidus::prelude::JobError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct CapturingEventObserver {
    contexts: Arc<Mutex<Vec<EventContextDto>>>,
}

impl EventObserver<LaunchReadyEvent> for CapturingEventObserver {
    fn on_event_published(&self, context: &ObservedEventContext) {
        lock(&self.contexts).push(EventContextDto {
            operation_id: context.operation_id().to_owned(),
            event_name: context.event_name().to_owned(),
            attributes: context.attributes().clone(),
        });
    }
}

impl CapturingEventObserver {
    fn first(&self) -> Result<EventContextDto, HttpError> {
        lock(&self.contexts)
            .first()
            .cloned()
            .ok_or_else(HttpError::internal_server_error)
    }
}

#[derive(Clone, Default)]
struct CapturingJobObserver {
    finished: Arc<Mutex<Vec<JobRunDto>>>,
}

impl JobObserver for CapturingJobObserver {
    fn on_job_started(&self, _context: &ObservedJobContext) {}

    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
        lock(&self.finished).push(JobRunDto {
            run_id: context.run_id().to_owned(),
            job_name: context.job_name().to_owned(),
            status: match status {
                JobResultStatus::Success => "success".to_owned(),
                JobResultStatus::Failure => "failure".to_owned(),
            },
            attributes: context.attributes().clone(),
        });
    }
}

impl CapturingJobObserver {
    fn finished(&self) -> Vec<JobRunDto> {
        lock(&self.finished).clone()
    }
}

#[derive(Clone, Serialize, ToSchema)]
struct EventContextDto {
    operation_id: String,
    event_name: String,
    attributes: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, ToSchema)]
struct JobRunDto {
    run_id: String,
    job_name: String,
    status: String,
    attributes: BTreeMap<String, String>,
}

#[derive(Clone, Serialize, ToSchema)]
struct QueueDto {
    sync_completed: Vec<&'static str>,
    async_completed: Vec<&'static str>,
}

#[derive(Clone, Serialize, ToSchema)]
struct WorkflowDto {
    event: EventContextDto,
    event_payloads: Vec<i64>,
    job_runs: Vec<JobRunDto>,
    queue: QueueDto,
}

#[controller("/health")]
struct HealthController;

#[routes]
impl HealthController {
    #[get("/")]
    #[openapi(summary = "Health check", tags = ["health"], status = 200, response = HealthDto)]
    async fn health(&self) -> Json<HealthDto> {
        Json(HealthDto { status: "ok" })
    }
}

#[derive(Serialize, ToSchema)]
struct HealthDto {
    status: &'static str,
}

fn cache_key(id: i64) -> String {
    format!("launch:{id}")
}

fn db_error(error: sqlx::Error) -> NidusError {
    NidusError::ApplicationBuild {
        message: error.to_string(),
    }
}

fn map_db_error(error: sqlx::Error) -> HttpError {
    tracing::error!(error = %error, "database operation failed");
    HttpError::internal_server_error()
}

fn map_job_error(error: nidus::prelude::JobError) -> HttpError {
    HttpError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "workflow_failed",
        error.to_string(),
    )
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use nidus::prelude::{ModuleGraph, StatusCode};
    use nidus_testing::TestApp;
    use serde_json::{Value, json};

    use super::*;

    async fn test_app() -> TestApp {
        TestApp::from_router(app(AppConfig::test()).await.unwrap().into_router())
    }

    async fn create_launch(app: &TestApp) {
        app.post("/launches")
            .header("x-api-key", "launch-secret")
            .json(&json!({
                "name": "Nidus 1.0",
                "owner_email": "owner@nidus.dev"
            }))
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "name": "Nidus 1.0",
                "owner_email": "owner@nidus.dev",
                "status": "queued"
            }));
    }

    #[test]
    fn module_graph_names_framework_boundaries() {
        let graph = ModuleGraph::from_root::<AppModule>().unwrap();

        assert_eq!(
            graph.get("AppModule").unwrap().imports(),
            [
                "InfrastructureModule",
                "AuthModule",
                "LaunchesModule",
                "WorkflowModule"
            ]
        );
        assert_eq!(
            graph.get("LaunchesModule").unwrap().controllers(),
            ["LaunchController"]
        );
        assert_eq!(
            graph.get("InfrastructureModule").unwrap().exports(),
            ["SqlitePoolProvider", "MokaCacheProvider"]
        );
    }

    #[tokio::test]
    async fn creates_reads_updates_and_caches_launches() {
        let app = test_app().await;
        create_launch(&app).await;

        app.get("/launches/1")
            .header("x-api-key", "launch-secret")
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "name": "Nidus 1.0",
                "owner_email": "owner@nidus.dev",
                "status": "queued"
            }));

        app.patch("/launches/1/ready")
            .header("x-api-key", "launch-secret")
            .send()
            .await
            .assert_json(json!({
                "id": 1,
                "name": "Nidus 1.0",
                "owner_email": "owner@nidus.dev",
                "status": "ready"
            }));
    }

    #[tokio::test]
    async fn guard_and_validation_errors_are_stable() {
        let app = test_app().await;

        let unauthorized = app
            .post("/launches")
            .json(&json!({
                "name": "Nidus 1.0",
                "owner_email": "owner@nidus.dev"
            }))
            .send()
            .await;
        unauthorized.assert_status(StatusCode::UNAUTHORIZED);

        let invalid = app
            .post("/launches")
            .header("x-api-key", "launch-secret")
            .json(&json!({ "name": "", "owner_email": "not-email" }))
            .send()
            .await;
        invalid.assert_status(StatusCode::UNPROCESSABLE_ENTITY);
        let body: Value = invalid.json();
        assert_eq!(body["error"]["code"], "validation_failed");
        assert_eq!(body["error"]["fields"], Value::Null);
    }

    #[tokio::test]
    async fn openapi_health_metrics_context_and_body_limit_are_wired() {
        let app = test_app().await;

        app.get("/health")
            .send()
            .await
            .assert_json(json!({ "status": "ok" }));

        let openapi = app.get("/openapi.json").send().await;
        openapi.assert_status(StatusCode::OK);
        let body: Value = openapi.json();
        assert_eq!(body["info"]["title"], "Nidus Launchpad API");
        assert_eq!(
            body["paths"]["/launches"]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/CreateLaunchDto"
        );

        app.get("/ops/context")
            .header("x-api-key", "launch-secret")
            .send()
            .await
            .assert_status(StatusCode::OK);

        app.get("/metrics")
            .send()
            .await
            .assert_status(StatusCode::OK);

        app.post("/ops/webhook")
            .header("content-length", "33")
            .body("012345678901234567890123456789012")
            .send()
            .await
            .assert_status(StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn observed_events_and_jobs_report_workflow_context() {
        let app = test_app().await;
        let response = app
            .post("/ops/workflow")
            .header("x-api-key", "launch-secret")
            .send()
            .await;
        response.assert_status(StatusCode::OK);
        let body: Value = response.json();

        assert_eq!(body["event"]["event_name"], "launch.ready");
        assert_eq!(body["event_payloads"], json!([1]));
        assert_eq!(
            body["queue"]["sync_completed"],
            json!(["send_launch_digest"])
        );
        assert_eq!(
            body["queue"]["async_completed"],
            json!(["warm_launch_cache"])
        );
        assert_eq!(body["job_runs"][0]["status"], "success");
        assert_eq!(body["job_runs"][1]["job_name"], "warm_launch_cache");
    }

    #[tokio::test]
    async fn container_resolves_adapter_backed_service() {
        let app = app(AppConfig::test()).await.unwrap();
        assert!(
            app.application()
                .container()
                .resolve::<SqlitePoolProvider>()
                .is_ok()
        );
        assert!(
            app.application()
                .container()
                .resolve::<MokaCacheProvider>()
                .is_ok()
        );
        assert!(
            app.application()
                .container()
                .resolve::<LaunchService>()
                .is_ok()
        );
    }
}
