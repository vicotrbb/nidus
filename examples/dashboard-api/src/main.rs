use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::routing::{get as axum_get, post as axum_post};
use nidus::prelude::*;
use serde::Serialize;

#[controller("/hello")]
struct HelloController;

#[routes]
impl HelloController {
    #[get("/{name}")]
    async fn hello(&self, Path(name): Path<String>) -> Json<MessageDto> {
        Json(MessageDto {
            message: format!("hello {name} from Nidus Dashboard example"),
        })
    }
}

#[module(controllers(HelloController))]
struct AppModule;

#[derive(Clone)]
struct ExampleState {
    collector:
        nidus::dashboard::DashboardCollector<nidus::dashboard::storage::DashboardStorageHandle>,
    sequence: Arc<AtomicU64>,
}

#[derive(Serialize)]
struct MessageDto {
    message: String,
}

#[derive(Serialize)]
struct CaptureDto {
    captured: &'static str,
}

#[derive(Serialize)]
struct HealthDto {
    status: &'static str,
}

#[nidus::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dashboard = NidusDashboard::builder()
        .auth(DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN"))
        .storage(DashboardStorage::sqlite_from_env(
            "NIDUS_DASHBOARD_DATABASE_URL",
        ))
        .capture(DashboardCapture::metadata_only())
        .retention(DashboardRetention::days(7).max_events(100_000))
        .build()?;

    let state = ExampleState {
        collector: dashboard.collector(),
        sequence: Arc::new(AtomicU64::new(1)),
    };

    Nidus::create::<AppModule>()
        .with_dashboard(dashboard)
        .with_router(example_routes(state))
        .build()
        .await?
        .listen("127.0.0.1:4310")
        .await?;
    Ok(())
}

fn example_routes(state: ExampleState) -> Router {
    Router::new()
        .route("/health", axum_get(health))
        .route(
            "/events/user-created",
            axum_post({
                let state = state.clone();
                move || {
                    let state = state.clone();
                    async move {
                        let sequence = state.sequence.fetch_add(1, Ordering::Relaxed);
                        let operation_id = format!("example-event-{sequence}");
                        state
                            .collector
                            .record_event(
                                "user.created",
                                Some(operation_id.as_str()),
                                [
                                    ("source", "dashboard-api".to_owned()),
                                    ("sequence", sequence.to_string()),
                                ],
                            )
                            .await
                            .map_err(|error| {
                                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
                            })?;
                        Ok::<_, (StatusCode, String)>(Json(CaptureDto { captured: "event" }))
                    }
                }
            }),
        )
        .route(
            "/jobs/daily-digest",
            axum_post(move || {
                let state = state.clone();
                async move {
                    let sequence = state.sequence.fetch_add(1, Ordering::Relaxed);
                    let run_id = format!("example-job-{sequence}");
                    state
                        .collector
                        .record_job(
                            "daily_digest",
                            Some(run_id.as_str()),
                            true,
                            14 + (sequence % 9),
                        )
                        .await
                        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    Ok::<_, (StatusCode, String)>(Json(CaptureDto { captured: "job" }))
                }
            }),
        )
}

async fn health() -> Json<HealthDto> {
    Json(HealthDto { status: "ok" })
}
