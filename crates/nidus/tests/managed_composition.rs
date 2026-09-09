use async_trait::async_trait;
use axum::{Router, body::Body, http::Request, routing::get};
use nidus::http::{RequestScoped, middleware::PrometheusMetrics};
use nidus::lifecycle::managed::{ManagedOptions, State};
use nidus::testing::TestApp;
use nidus::{app::Resource, prelude::*};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;
use tower::ServiceExt;

#[derive(Default)]
struct Probe {
    log: Mutex<Vec<&'static str>>,
    next: AtomicUsize,
    entered: Notify,
    release: Notify,
}
struct External(Arc<Probe>);
#[async_trait]
impl Resource for External {
    async fn initialize(container: &Container) -> Result<Self> {
        let probe = container.resolve::<Probe>()?;
        probe.log.lock().unwrap().push("external:init");
        Ok(Self(probe))
    }
    async fn shutdown(&self) -> Result<()> {
        self.0.log.lock().unwrap().push("external:close");
        Ok(())
    }
}
struct Owned(Arc<Probe>);
#[async_trait]
impl Resource for Owned {
    async fn initialize(container: &Container) -> Result<Self> {
        let external = container.resolve::<External>()?;
        external.0.log.lock().unwrap().push("owned:init");
        Ok(Self(external.0.clone()))
    }
    async fn shutdown(&self) -> Result<()> {
        self.0.log.lock().unwrap().push("owned:close");
        Ok(())
    }
}
struct ScopedValue {
    id: usize,
    resource: Arc<External>,
}
impl nidus::ProviderRegistrant for ScopedValue {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.register_request_scoped(|scope| {
            let resource = scope.resolve::<External>()?;
            Ok(Self {
                id: resource.0.next.fetch_add(1, Ordering::SeqCst),
                resource,
            })
        })
    }
}
#[controller("/module")]
struct ModuleController {
    external: Inject<External>,
}
#[routes]
impl ModuleController {
    #[get("/")]
    async fn scoped(
        &self,
        first: RequestScoped<ScopedValue>,
        second: RequestScoped<ScopedValue>,
    ) -> String {
        assert_eq!(first.id, second.id);
        assert!(Arc::ptr_eq(&first.resource.0, &self.external.0));
        first.id.to_string()
    }
    #[get("/pending")]
    async fn pending(&self) -> &'static str {
        self.external.0.entered.notify_one();
        self.external.0.release.notified().await;
        "finished"
    }
}
struct Resources;
impl Module for Resources {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("Resources")
            .resource::<External>()
            .resource::<Owned>()
            .export_typed::<External>()
            .build()
    }
}
struct Root;
impl Module for Root {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("Root")
            .import_typed::<Resources>()
            .provider_typed::<ScopedValue>()
            .controller_typed::<ModuleController>()
            .build()
    }
}
fn probe() -> (Arc<Probe>, External) {
    let probe = Arc::new(Probe::default());
    (probe.clone(), External(probe))
}

#[tokio::test]
async fn module_test_and_production_share_overrides_scopes_resources_and_manual_routes() {
    for production in [false, true] {
        let (probe, external) = probe();
        let manual = Router::new().route("/manual", get(|| async { "manual" }));
        let app = if production {
            TestApp::from_application(
                Nidus::create::<Root>()
                    .override_provider(external)
                    .unwrap()
                    .with_router(manual)
                    .build()
                    .await
                    .unwrap(),
            )
        } else {
            TestApp::bootstrap_with_router::<Root>(manual)
                .unwrap()
                .override_provider(external)
                .unwrap()
                .build_started()
                .await
                .unwrap()
        };
        let resolved = app.resolve::<External>().unwrap();
        assert!(Arc::ptr_eq(&probe, &resolved.0));
        app.get("/module").send().await.assert_text("0");
        app.get("/module").send().await.assert_text("1");
        app.get("/manual").send().await.assert_text("manual");
        assert_eq!(*probe.log.lock().unwrap(), ["owned:init"]);
        app.shutdown().await.unwrap();
        assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
    }
}

#[tokio::test]
async fn end_to_end_managed_modules_cancel_metrics_drain_workers_and_close_once() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (probe, external) = probe();
    let metrics = PrometheusMetrics::new();
    let worker_started = Arc::new(Notify::new());
    let worker_done = Arc::new(Notify::new());
    let options = ManagedOptions::default().worker("background", {
        let started = worker_started.clone();
        let done = worker_done.clone();
        move |container, mut signal| async move {
            let resource = container.resolve::<Owned>()?;
            started.notify_one();
            signal.cancelled().await;
            assert_eq!(signal.state(), State::Draining);
            assert_eq!(*resource.0.log.lock().unwrap(), ["owned:init"]);
            resource.0.log.lock().unwrap().push("worker:drained");
            done.notify_one();
            Ok(())
        }
    });
    let http = Nidus::create::<Root>()
        .override_provider(external)
        .unwrap()
        .build()
        .await
        .unwrap()
        .map_router(|router| router.layer(metrics.layer()));
    let managed = nidus::lifecycle::managed::Managed::start(
        async {
            Ok(nidus::http::managed::ManagedHttp::new(
                http,
                "127.0.0.1:0".parse().unwrap(),
            ))
        },
        options,
    )
    .await
    .unwrap();
    worker_started.notified().await;
    let router = managed.target().router().clone();
    let app = TestApp::from_router(router.clone());
    app.get("/module").send().await.assert_text("0");
    let request = tokio::spawn(
        router.oneshot(
            Request::builder()
                .uri("/module/pending")
                .body(Body::empty())
                .unwrap(),
        ),
    );
    probe.entered.notified().await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(
        metrics
            .render()
            .contains("nidus_http_in_flight_requests{method=\"GET\",route=\"/module/pending\"} 0")
    );
    assert!(metrics.render().contains(
        "nidus_http_cancelled_requests_total{method=\"GET\",route=\"/module/pending\"} 1"
    ));
    let address = managed.target().local_addr().unwrap();
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(b"GET /module/pending HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    probe.entered.notified().await;
    managed.request_shutdown();
    let mut signal = managed.signal();
    signal.cancelled().await;
    assert!(!signal.is_ready());
    assert_ne!(signal.state(), State::Stopped);
    probe.release.notify_one();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    assert!(String::from_utf8(response).unwrap().ends_with("finished"));
    let (first, second) = tokio::join!(managed.shutdown(), managed.shutdown());
    assert!(Arc::ptr_eq(&first, &second));
    tokio::net::TcpListener::bind(address).await.unwrap();
    assert!(second.is_success());
    worker_done.notified().await;
    assert_eq!(managed.signal().state(), State::Stopped);
    assert_eq!(
        *probe.log.lock().unwrap(),
        ["owned:init", "worker:drained", "owned:close"]
    );
}

#[tokio::test]
async fn listener_bind_failure_closes_initialized_resources() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (probe, external) = probe();
    let result = Nidus::create::<Root>()
        .override_provider(external)
        .unwrap()
        .start_managed(address, ManagedOptions::default())
        .await;
    let report = match result {
        Err(report) => report,
        Ok(_) => panic!("bind must fail"),
    };
    assert!(
        report.failures[0]
            .source
            .downcast_ref::<std::io::Error>()
            .is_some()
    );
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
}

#[tokio::test]
async fn managed_http_drains_inflight_and_releases_listener() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (probe, external) = probe();
    let health = Router::new().route("/health/ready", get(|| async { "ready" }));
    let managed = Nidus::create::<Root>()
        .override_provider(external)
        .unwrap()
        .with_router(health)
        .start_managed("127.0.0.1:0".parse().unwrap(), ManagedOptions::default())
        .await
        .unwrap();
    let address = managed.target().local_addr().unwrap();
    let readiness = managed.target().router().clone();
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(b"GET /module/pending HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    probe.entered.notified().await;
    managed.request_shutdown();
    let mut signal = managed.signal();
    signal.cancelled().await;
    assert_eq!(signal.state(), State::Draining);
    let response = readiness
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init"]);
    probe.release.notify_one();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.unwrap();
    assert!(String::from_utf8(bytes).unwrap().ends_with("finished"));
    assert!(managed.shutdown().await.is_success());
    let rebound = tokio::net::TcpListener::bind(address).await.unwrap();
    drop(rebound);
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
}

#[tokio::test]
async fn opaque_router_failure_after_initialization_rolls_back() {
    let (probe, external) = probe();
    let duplicate = Router::new().route("/module", get(|| async {}));
    let result = Nidus::create::<Root>()
        .override_provider(external)
        .unwrap()
        .with_router(duplicate)
        .build()
        .await;
    assert!(result.is_err());
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
}

#[tokio::test]
async fn managed_deadline_drops_hanging_http_requests_before_resource_cleanup() {
    use tokio::io::AsyncWriteExt;
    let (probe, external) = probe();
    let mut options = ManagedOptions::default();
    options.shutdown.drain_timeout = Duration::ZERO;
    let managed = Nidus::create::<Root>()
        .override_provider(external)
        .unwrap()
        .start_managed("127.0.0.1:0".parse().unwrap(), options)
        .await
        .unwrap();
    let address = managed.target().local_addr().unwrap();
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    stream
        .write_all(b"GET /module/pending HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    probe.entered.notified().await;
    assert!(managed.shutdown().await.deadline_expired);
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
    tokio::net::TcpListener::bind(address).await.unwrap();
}

#[controller("/conflict")]
struct FirstConflict;
#[routes]
impl FirstConflict {
    #[get("/{id}")]
    async fn route(&self) {}
}
#[controller("/conflict")]
struct SecondConflict;
#[routes]
impl SecondConflict {
    #[get("/{name}")]
    async fn route(&self) {}
}
struct ConflictingRoutes;
impl Module for ConflictingRoutes {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("ConflictingRoutes")
            .import_typed::<Resources>()
            .controller_typed::<FirstConflict>()
            .controller_typed::<SecondConflict>()
            .build()
    }
}
#[tokio::test]
async fn structural_route_conflicts_fail_before_resource_initialization() {
    let (probe, external) = probe();
    assert!(
        Nidus::create::<ConflictingRoutes>()
            .override_provider(external)
            .unwrap()
            .build()
            .await
            .is_err()
    );
    assert!(probe.log.lock().unwrap().is_empty());
    assert!(TestApp::bootstrap::<ConflictingRoutes>().is_err());
}

struct Fails;
#[async_trait]
impl Resource for Fails {
    async fn initialize(_: &Container) -> Result<Self> {
        Err(NidusError::ApplicationBuild {
            message: "deliberate initializer failure".into(),
        })
    }
    async fn shutdown(&self) -> Result<()> {
        panic!("failed initializer never transferred ownership")
    }
}
struct PartialFailure;
impl Module for PartialFailure {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("PartialFailure")
            .import_typed::<Resources>()
            .resource::<Fails>()
            .build()
    }
}
#[tokio::test]
async fn initializer_failure_rolls_back_imported_resources_in_production_and_tests() {
    for production in [true, false] {
        let (probe, external) = probe();
        if production {
            assert!(
                Nidus::create::<PartialFailure>()
                    .override_provider(external)
                    .unwrap()
                    .build()
                    .await
                    .is_err()
            );
        } else {
            assert!(
                TestApp::bootstrap::<PartialFailure>()
                    .unwrap()
                    .override_provider(external)
                    .unwrap()
                    .build_started()
                    .await
                    .is_err()
            );
        }
        assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
    }
}

#[tokio::test]
async fn module_managed_test_waits_for_inmemory_http_before_cleanup() {
    let (probe, external) = probe();
    let app = TestApp::bootstrap::<Root>()
        .unwrap()
        .override_provider(external)
        .unwrap()
        .build_managed(ManagedOptions::default())
        .await
        .unwrap();
    let request = tokio::spawn({
        let app = app.clone();
        async move { app.get("/module/pending").send().await }
    });
    probe.entered.notified().await;
    let shutdown = tokio::spawn({
        let app = app.clone();
        async move { app.shutdown_report().await.unwrap() }
    });
    // The pending HTTP request holds the supervisor's tracked work open.
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init"]);
    probe.release.notify_one();
    request.await.unwrap().assert_text("finished");
    assert!(shutdown.await.unwrap().is_success());
    assert_eq!(*probe.log.lock().unwrap(), ["owned:init", "owned:close"]);
    assert!(app.get("/module").try_send().await.is_err());
}

#[tokio::test]
async fn explicit_scope_opt_in_survives_a_singleton_override() {
    let (probe, external) = probe();
    let app = TestApp::bootstrap::<Root>()
        .unwrap()
        .override_provider(external)
        .unwrap()
        .override_provider(ScopedValue {
            id: 42,
            resource: Arc::new(External(probe)),
        })
        .unwrap()
        .with_request_scope()
        .build_started()
        .await
        .unwrap();
    app.get("/module").send().await.assert_text("42");
    app.shutdown().await.unwrap();
}

#[cfg(feature = "config")]
#[tokio::test]
async fn configuration_override_replaces_declared_initializer_before_it_runs() {
    use nidus::config::Config;
    struct ConfigRoot;
    fn original(
        _: &mut Container,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async { panic!("overridden config initializer must not run") })
    }
    impl Module for ConfigRoot {
        fn definition() -> ModuleDefinition {
            ModuleBuilder::new("ConfigRoot")
                .async_initializer_for::<Config>(original)
                .build()
        }
    }
    let app = TestApp::bootstrap::<ConfigRoot>()
        .unwrap()
        .config(Config::from_pairs([("mode", "test")]))
        .build_managed(ManagedOptions::default())
        .await
        .unwrap();
    assert_eq!(
        app.resolve::<Config>().unwrap().get("mode"),
        app.config().get("mode")
    );
    app.shutdown().await.unwrap();
}
