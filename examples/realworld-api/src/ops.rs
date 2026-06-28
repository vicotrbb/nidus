use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::{
    body::Bytes,
    routing::{get as axum_get, post as axum_post},
};
use nidus::prelude::{
    AsyncJob, AsyncJobQueue, EventBus, EventObserver, HttpError, Job, JobObserver, JobQueue,
    JobResultStatus, Json, ObservedEventBus, ObservedEventContext, ObservedJobContext,
    ObservedJobRunner, RateLimitConfig, RequestContext, RequestScoped, Router, api_key_identity,
    timeout_response_layer, webhook_body_limit_layer,
};
use serde::Serialize;

use crate::config::AppConfig;

#[derive(Debug)]
pub struct ScopedRequestNumber(pub u64);

pub fn router(_config: &AppConfig) -> Router {
    Router::new()
        .route("/context", axum_get(context))
        .route(
            "/ops/limited",
            axum_get(|| async { Json(serde_json::json!({ "status": "limited ok" })) }).layer(
                RateLimitConfig::new(1, Duration::from_secs(60), DefaultRateLimitStore::new())
                    .identity(api_key_identity())
                    .fail_closed()
                    .layer(),
            ),
        )
        .route(
            "/ops/webhook",
            axum_post(webhook).layer(webhook_body_limit_layer(16)),
        )
        .route(
            "/ops/timeout",
            axum_get(timeout).layer(timeout_response_layer(Duration::from_millis(5))),
        )
        .route("/ops/fail", axum_get(fail))
        .route("/ops/workflows/observed", axum_post(observed_workflow))
}

pub fn request_scope_container() -> nidus::prelude::Result<Arc<nidus::prelude::Container>> {
    let mut container = nidus::prelude::Container::new();
    let next_request = Arc::new(AtomicU64::new(0));
    container.register_request::<ScopedRequestNumber, _>({
        let next_request = Arc::clone(&next_request);
        move |_container| {
            Ok(ScopedRequestNumber(
                next_request.fetch_add(1, Ordering::SeqCst),
            ))
        }
    })?;
    Ok(Arc::new(container))
}

async fn context(
    context: RequestContext,
    scoped: RequestScoped<ScopedRequestNumber>,
) -> Json<RequestContextDto> {
    Json(RequestContextDto {
        request_id: context.request_id().to_owned(),
        correlation_id: context.correlation_id().map(str::to_owned),
        method: context.method().as_str().to_owned(),
        path: context.path().to_owned(),
        route: context.route().map(str::to_owned),
        client_kind: context.client_kind().as_str().to_owned(),
        scoped_request_number: scoped.0,
    })
}

async fn webhook(body: Bytes) -> Json<WebhookDto> {
    Json(WebhookDto {
        received_bytes: body.len(),
    })
}

async fn timeout() -> &'static str {
    tokio::time::sleep(Duration::from_millis(25)).await;
    "late"
}

async fn fail() -> HttpError {
    HttpError::new(
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "database_unavailable",
        "sqlite connection string leaked",
    )
}

async fn observed_workflow(
    context: RequestContext,
) -> Result<Json<ObservedWorkflowDto>, HttpError> {
    let request_id = context.request_id().to_owned();

    let event_observer = CapturingEventObserver::default();
    let bus = EventBus::new();
    let subscriber = bus.subscribe();
    let observed_bus = ObservedEventBus::new(bus, event_observer.clone())
        .context("request_id", request_id.clone())
        .context("feature", "realworld-api")
        .operation_id_generator(|| "event-run-1".to_owned());
    observed_bus.publish_named("task.completed", TaskCompletedEvent { task_id: 1 });

    let mut sync_queue = JobQueue::new();
    sync_queue.push(SendTaskDigestJob);
    let sync_report = sync_queue.run_all();

    let mut async_queue = AsyncJobQueue::new();
    async_queue.push(RefreshProjectSummaryJob);
    let async_report = async_queue.run_all().await;

    let sync_job_observer = CapturingJobObserver::default();
    let sync_runner = ObservedJobRunner::new(sync_job_observer.clone())
        .context("request_id", request_id.clone())
        .context("queue", "sync")
        .run_id_generator(|| "job-run-1".to_owned());
    sync_runner
        .run(&SendTaskDigestJob)
        .map_err(|error| workflow_job_error("sync", &error))?;

    let async_job_observer = CapturingJobObserver::default();
    let async_runner = ObservedJobRunner::new(async_job_observer.clone())
        .context("request_id", request_id)
        .context("queue", "async")
        .run_id_generator(|| "job-run-1".to_owned());
    async_runner
        .run_async(&RefreshProjectSummaryJob)
        .await
        .map_err(|error| workflow_job_error("async", &error))?;

    Ok(Json(ObservedWorkflowDto {
        event: first_observed_event(event_observer.published())?,
        event_payloads: subscriber
            .drain()
            .into_iter()
            .map(|event| event.task_id)
            .collect(),
        sync_job: sync_job_observer.observation()?,
        async_job: async_job_observer.observation()?,
        queue: QueueDto {
            sync_completed: sync_report.completed().to_vec(),
            async_completed: async_report.completed().to_vec(),
        },
    }))
}

type DefaultRateLimitStore = nidus::prelude::InMemoryRateLimitStore;

#[derive(Serialize)]
struct RequestContextDto {
    request_id: String,
    correlation_id: Option<String>,
    method: String,
    path: String,
    route: Option<String>,
    client_kind: String,
    scoped_request_number: u64,
}

#[derive(Serialize)]
struct WebhookDto {
    received_bytes: usize,
}

#[derive(Clone)]
struct TaskCompletedEvent {
    task_id: i64,
}

#[derive(Clone, Default)]
struct CapturingEventObserver {
    contexts: Arc<Mutex<Vec<EventContextDto>>>,
}

impl EventObserver<TaskCompletedEvent> for CapturingEventObserver {
    fn on_event_published(&self, context: &ObservedEventContext) {
        lock(&self.contexts).push(EventContextDto {
            operation_id: context.operation_id().to_owned(),
            event_name: context.event_name().to_owned(),
            attributes: context.attributes().clone(),
        });
    }
}

impl CapturingEventObserver {
    fn published(&self) -> Vec<EventContextDto> {
        lock(&self.contexts).clone()
    }
}

#[derive(Clone, Serialize)]
struct EventContextDto {
    operation_id: String,
    event_name: String,
    attributes: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct SendTaskDigestJob;

impl Job for SendTaskDigestJob {
    fn name(&self) -> &'static str {
        "send_task_digest"
    }

    fn run(&self) -> std::result::Result<(), nidus::prelude::JobError> {
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct RefreshProjectSummaryJob;

#[async_trait::async_trait]
impl AsyncJob for RefreshProjectSummaryJob {
    fn name(&self) -> &'static str {
        "refresh_project_summary"
    }

    async fn run(&self) -> std::result::Result<(), nidus::prelude::JobError> {
        Ok(())
    }
}

#[derive(Clone, Default)]
struct CapturingJobObserver {
    started: Arc<Mutex<Vec<JobContextDto>>>,
    finished: Arc<Mutex<Vec<JobFinishedDto>>>,
}

impl JobObserver for CapturingJobObserver {
    fn on_job_started(&self, context: &ObservedJobContext) {
        lock(&self.started).push(JobContextDto::from(context));
    }

    fn on_job_finished(&self, context: &ObservedJobContext, status: JobResultStatus) {
        lock(&self.finished).push(JobFinishedDto {
            run_id: context.run_id().to_owned(),
            job_name: context.job_name(),
            attributes: context.attributes().clone(),
            status: match status {
                JobResultStatus::Success => "success",
                JobResultStatus::Failure => "failure",
            },
        });
    }
}

impl CapturingJobObserver {
    fn observation(&self) -> Result<JobObservationDto, HttpError> {
        let started = lock(&self.started)
            .first()
            .cloned()
            .ok_or_else(|| workflow_invariant_error("job start should be observed"))?;
        let finished = lock(&self.finished)
            .first()
            .cloned()
            .ok_or_else(|| workflow_invariant_error("job finish should be observed"))?;

        Ok(JobObservationDto { started, finished })
    }
}

fn first_observed_event(events: Vec<EventContextDto>) -> Result<EventContextDto, HttpError> {
    events
        .into_iter()
        .next()
        .ok_or_else(|| workflow_invariant_error("observed event should be captured"))
}

fn workflow_job_error(queue: &'static str, error: &nidus::prelude::JobError) -> HttpError {
    tracing::error!(
        queue,
        error = %error,
        "realworld observed workflow job failed"
    );
    HttpError::internal_server_error()
}

fn workflow_invariant_error(message: &'static str) -> HttpError {
    tracing::error!(
        invariant = message,
        "realworld observed workflow invariant failed"
    );
    HttpError::internal_server_error()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_observer_reports_missing_observations_as_http_error() {
        let observer = CapturingJobObserver::default();

        assert!(observer.observation().is_err());
    }

    #[test]
    fn event_observer_reports_missing_event_as_http_error() {
        assert!(first_observed_event(Vec::new()).is_err());
    }
}

#[derive(Clone, Serialize)]
struct JobContextDto {
    run_id: String,
    job_name: &'static str,
    attributes: BTreeMap<String, String>,
}

impl From<&ObservedJobContext> for JobContextDto {
    fn from(context: &ObservedJobContext) -> Self {
        Self {
            run_id: context.run_id().to_owned(),
            job_name: context.job_name(),
            attributes: context.attributes().clone(),
        }
    }
}

#[derive(Clone, Serialize)]
struct JobFinishedDto {
    run_id: String,
    job_name: &'static str,
    attributes: BTreeMap<String, String>,
    status: &'static str,
}

#[derive(Serialize)]
struct JobObservationDto {
    started: JobContextDto,
    finished: JobFinishedDto,
}

#[derive(Serialize)]
struct QueueDto {
    sync_completed: Vec<&'static str>,
    async_completed: Vec<&'static str>,
}

#[derive(Serialize)]
struct ObservedWorkflowDto {
    event: EventContextDto,
    event_payloads: Vec<i64>,
    sync_job: JobObservationDto,
    async_job: JobObservationDto,
    queue: QueueDto,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
