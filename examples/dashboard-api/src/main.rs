use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use axum::routing::{get as axum_get, post as axum_post};
use nidus::prelude::*;
use serde::Serialize;

#[injectable]
struct AccountsRepository;

#[injectable]
struct AccountsService;

impl AccountsService {
    fn describe(&self, account_id: &str) -> String {
        format!("account {account_id} is active")
    }
}

#[injectable]
struct BillingLedger;

#[injectable]
struct BillingService;

impl BillingService {
    fn invoice_summary(&self, account_id: &str) -> String {
        format!("billing ledger is current for {account_id}")
    }
}

#[injectable]
struct NotificationTemplates;

#[injectable]
struct NotificationsService;

impl NotificationsService {
    fn channel_status(&self, channel: &str) -> String {
        format!("{channel} channel is accepting deliveries")
    }
}

#[injectable]
struct AuditTrail;

impl AuditTrail {
    fn recent(&self) -> &'static str {
        "audit trail captured recent module activity"
    }
}

#[controller("/accounts")]
struct AccountsController {
    service: Inject<AccountsService>,
}

#[routes]
impl AccountsController {
    #[get("/{id}")]
    async fn show(&self, Path(id): Path<String>) -> Json<MessageDto> {
        Json(MessageDto {
            message: self.service.describe(&id),
        })
    }

    #[post("/")]
    async fn create(&self) -> (StatusCode, Json<MessageDto>) {
        (
            StatusCode::CREATED,
            Json(MessageDto {
                message: "account accepted by AccountsModule".to_owned(),
            }),
        )
    }
}

#[controller("/billing")]
struct BillingController {
    service: Inject<BillingService>,
}

#[routes]
impl BillingController {
    #[get("/{account_id}")]
    async fn account(&self, Path(account_id): Path<String>) -> Json<MessageDto> {
        Json(MessageDto {
            message: self.service.invoice_summary(&account_id),
        })
    }

    #[post("/invoices")]
    async fn create_invoice(&self) -> (StatusCode, Json<MessageDto>) {
        (
            StatusCode::CREATED,
            Json(MessageDto {
                message: "invoice created by BillingModule".to_owned(),
            }),
        )
    }
}

#[controller("/notifications")]
struct NotificationsController {
    service: Inject<NotificationsService>,
}

#[routes]
impl NotificationsController {
    #[post("/email")]
    async fn email(&self) -> Json<MessageDto> {
        Json(MessageDto {
            message: self.service.channel_status("email"),
        })
    }

    #[post("/sms")]
    async fn sms(&self) -> Json<MessageDto> {
        Json(MessageDto {
            message: self.service.channel_status("sms"),
        })
    }
}

#[controller("/audit")]
struct AuditController {
    trail: Inject<AuditTrail>,
}

#[routes]
impl AuditController {
    #[get("/recent")]
    async fn recent(&self) -> Json<MessageDto> {
        Json(MessageDto {
            message: self.trail.recent().to_owned(),
        })
    }
}

#[module]
struct AuditModule {
    providers: [AuditTrail],
    controllers: [AuditController],
    exports: [AuditTrail],
}

#[module]
struct AccountsModule {
    imports: [AuditModule],
    providers: (AccountsRepository, AccountsService),
    controllers: [AccountsController],
    exports: [AccountsService],
}

#[module]
struct BillingModule {
    imports: (AccountsModule, AuditModule),
    providers: (BillingLedger, BillingService),
    controllers: [BillingController],
    exports: [BillingService],
}

#[module]
struct NotificationsModule {
    imports: [AccountsModule],
    providers: (NotificationTemplates, NotificationsService),
    controllers: [NotificationsController],
    exports: [NotificationsService],
}

#[module]
struct AppModule {
    imports: (
        AccountsModule,
        BillingModule,
        NotificationsModule,
        AuditModule,
    ),
}

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
    name: &'static str,
}

#[derive(Serialize)]
struct HealthDto {
    status: &'static str,
}

type CaptureResult = std::result::Result<Json<CaptureDto>, (StatusCode, String)>;
type BoxedCaptureFuture = Pin<Box<dyn Future<Output = CaptureResult> + Send>>;

#[nidus::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let dashboard_auth = if std::env::var("NIDUS_DASHBOARD_DISABLE_AUTH")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes"))
    {
        DashboardAuth::unsafe_disabled_for_local_development()
    } else {
        DashboardAuth::bearer_from_env("NIDUS_DASHBOARD_TOKEN")
    };

    let dashboard = NidusDashboard::builder()
        .auth(dashboard_auth)
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
            "/activity/events/account-created",
            axum_post(event_endpoint(
                state.clone(),
                "account.created",
                "AccountsModule",
                "AccountsController",
            )),
        )
        .route(
            "/activity/events/invoice-paid",
            axum_post(event_endpoint(
                state.clone(),
                "billing.invoice_paid",
                "BillingModule",
                "BillingController",
            )),
        )
        .route(
            "/activity/events/notification-bounced",
            axum_post(event_endpoint(
                state.clone(),
                "notifications.bounced",
                "NotificationsModule",
                "NotificationsController",
            )),
        )
        .route(
            "/activity/jobs/reconcile-billing",
            axum_post(job_endpoint(
                state.clone(),
                "billing.reconcile",
                "BillingModule",
            )),
        )
        .route(
            "/activity/jobs/send-digest",
            axum_post(job_endpoint(
                state.clone(),
                "notifications.send_digest",
                "NotificationsModule",
            )),
        )
        .route(
            "/activity/jobs/audit-retention",
            axum_post(job_endpoint(state, "audit.retention", "AuditModule")),
        )
}

fn event_endpoint(
    state: ExampleState,
    name: &'static str,
    module: &'static str,
    controller: &'static str,
) -> impl Fn() -> BoxedCaptureFuture + Clone {
    move || {
        let state = state.clone();
        Box::pin(async move {
            let sequence = state.sequence.fetch_add(1, Ordering::Relaxed);
            let operation_id = format!("example-event-{sequence}");
            state
                .collector
                .record_event(
                    name,
                    Some(operation_id.as_str()),
                    [
                        ("module", module.to_owned()),
                        ("controller", controller.to_owned()),
                        ("source", "dashboard-api".to_owned()),
                        ("sequence", sequence.to_string()),
                    ],
                )
                .await
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
            Ok::<_, (StatusCode, String)>(Json(CaptureDto {
                captured: "event",
                name,
            }))
        })
    }
}

fn job_endpoint(
    state: ExampleState,
    name: &'static str,
    module: &'static str,
) -> impl Fn() -> BoxedCaptureFuture + Clone {
    move || {
        let state = state.clone();
        Box::pin(async move {
            let sequence = state.sequence.fetch_add(1, Ordering::Relaxed);
            let run_id = format!("example-job-{sequence}");
            let scheduled_id = format!("{run_id}-scheduled");
            state
                .collector
                .record_event(
                    "job.scheduled",
                    Some(scheduled_id.as_str()),
                    [
                        ("module", module.to_owned()),
                        ("source", "dashboard-api".to_owned()),
                        ("job", name.to_owned()),
                    ],
                )
                .await
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
            state
                .collector
                .record_job(name, Some(run_id.as_str()), true, 18 + (sequence % 17))
                .await
                .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
            tokio::time::sleep(Duration::from_millis(1)).await;
            Ok::<_, (StatusCode, String)>(Json(CaptureDto {
                captured: "job",
                name,
            }))
        })
    }
}

async fn health() -> Json<HealthDto> {
    Json(HealthDto { status: "ok" })
}
