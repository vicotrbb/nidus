use async_trait::async_trait;
use axum::{Router, body::Body, routing::get};
use futures_util::stream;
use nidus::http::{
    managed::ManagedHttp,
    middleware::PrometheusMetrics,
    server::{ApplicationHttpExt, HttpApplication},
};
use nidus::lifecycle::managed::{Managed, ManagedOptions, ShutdownPolicy};
use nidus_core::{
    Application, Container, LifecycleHook, LifecycleRunner, ModuleBuilder, ModuleGraph,
};
use nidus_testing::TestApp;
use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::Notify,
};

#[derive(Default)]
struct Streaming {
    polled: Notify,
    release: Notify,
    dropped: Notify,
    clock: AtomicUsize,
    body_drop: AtomicUsize,
    cleanup: AtomicUsize,
    closes: AtomicUsize,
}
struct BodyOwner(Arc<Streaming>);
impl Drop for BodyOwner {
    fn drop(&mut self) {
        self.0
            .body_drop
            .store(self.0.clock.fetch_add(1, SeqCst) + 1, SeqCst);
        self.0.dropped.notify_one();
    }
}
struct Cleanup(Arc<Streaming>);
#[async_trait]
impl LifecycleHook for Cleanup {
    async fn on_shutdown(&self) -> nidus::Result<()> {
        self.0
            .cleanup
            .store(self.0.clock.fetch_add(1, SeqCst) + 1, SeqCst);
        self.0.closes.fetch_add(1, SeqCst);
        Ok(())
    }
}
fn application(state: Arc<Streaming>, metrics: &PrometheusMetrics) -> HttpApplication {
    let router = Router::new()
        .route(
            "/stream",
            get({
                let state = state.clone();
                move || {
                    let owner = BodyOwner(state.clone());
                    async move {
                        Body::from_stream(stream::unfold((0, owner), |(step, owner)| async move {
                            match step {
                                0 => {
                                    owner.0.polled.notify_one();
                                    Some((Ok::<_, Infallible>("started"), (1, owner)))
                                }
                                1 => {
                                    owner.0.release.notified().await;
                                    Some((Ok("finished"), (2, owner)))
                                }
                                _ => None,
                            }
                        }))
                    }
                }
            }),
        )
        .layer(metrics.layer());
    Application::with_lifecycle(
        Container::new(),
        ModuleGraph::from_modules([ModuleBuilder::new("Streaming").build()]).unwrap(),
        LifecycleRunner::new().hook(Cleanup(state)),
    )
    .with_router(router)
}
fn assert_response_available(state: &Streaming, metrics: &PrometheusMetrics) {
    assert_eq!(state.body_drop.load(SeqCst), 0);
    assert_eq!(state.cleanup.load(SeqCst), 0);
    let rendered = metrics.render();
    assert!(rendered.contains("nidus_http_in_flight_requests{method=\"GET\",route=\"/stream\"} 0"));
    assert!(
        rendered.contains(
            "nidus_http_requests_total{method=\"GET\",route=\"/stream\",status=\"200\"} 1"
        )
    );
    assert!(
        !rendered
            .lines()
            .any(|line| line.starts_with("nidus_http_cancelled_requests_total{"))
    );
}
fn assert_closed(state: &Streaming) {
    assert_eq!(state.closes.load(SeqCst), 1);
    assert!(state.body_drop.load(SeqCst) > 0);
    assert!(state.body_drop.load(SeqCst) < state.cleanup.load(SeqCst));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tcp_streaming_body_drains_or_is_dropped_at_deadline_before_cleanup() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for expire in [false, true] {
            let state = Arc::new(Streaming::default());
            let metrics = PrometheusMetrics::new();
            let http = application(state.clone(), &metrics);
            let mut options = ManagedOptions::default();
            options.shutdown = ShutdownPolicy {
                drain_timeout: if expire {
                    Duration::from_millis(50)
                } else {
                    Duration::from_secs(5)
                },
                cleanup_timeout: Duration::from_secs(1),
            };
            let managed = Managed::start(
                async { Ok(ManagedHttp::new(http, "127.0.0.1:0".parse().unwrap())) },
                options,
            )
            .await
            .unwrap();
            let address = managed.target().local_addr().unwrap();
            let mut socket = tokio::net::TcpStream::connect(address).await.unwrap();
            socket
                .write_all(b"GET /stream HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            let mut prefix = Vec::new();
            while !prefix.ends_with(b"started") {
                let mut byte = [0];
                socket.read_exact(&mut byte).await.unwrap();
                prefix.push(byte[0]);
            }
            assert!(String::from_utf8_lossy(&prefix).starts_with("HTTP/1.1 200"));
            assert_response_available(&state, &metrics);
            managed.request_shutdown();
            managed.signal().cancelled().await;
            assert_eq!(state.cleanup.load(SeqCst), 0);
            if !expire {
                state.release.notify_one();
            }
            let mut remaining = Vec::new();
            socket.read_to_end(&mut remaining).await.unwrap();
            let report = managed.shutdown().await;
            assert_eq!(report.deadline_expired, expire, "{report:?}");
            if !expire {
                assert!(String::from_utf8_lossy(&remaining).contains("finished"));
                assert!(report.is_success());
            }
            assert_closed(&state);
            tokio::net::TcpListener::bind(address).await.unwrap();
        }
    })
    .await
    .expect("streaming TCP test exceeded watchdog");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_test_body_drain_cancellation_and_deadline_preserve_response_metrics() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for mode in 0..3 {
            let state = Arc::new(Streaming::default());
            let metrics = PrometheusMetrics::new();
            let http = application(state.clone(), &metrics);
            let mut options = ManagedOptions::default();
            options.shutdown.drain_timeout = if mode == 2 {
                Duration::from_millis(50)
            } else {
                Duration::from_secs(5)
            };
            let managed = Managed::start(async { Ok(http) }, options).await.unwrap();
            let app = Arc::new(TestApp::from_managed(managed.clone()));
            let request_app = app.clone();
            let caller = tokio::spawn(async move { request_app.get("/stream").try_send().await });
            state.polled.notified().await;
            assert_response_available(&state, &metrics);
            if mode == 1 {
                caller.abort();
                assert!(matches!(caller.await, Err(ref error) if error.is_cancelled()));
                state.dropped.notified().await;
            } else {
                managed.request_shutdown();
                managed.signal().cancelled().await;
                assert_eq!(state.cleanup.load(SeqCst), 0);
                if mode == 0 {
                    state.release.notify_one();
                    caller
                        .await
                        .unwrap()
                        .unwrap()
                        .assert_text("startedfinished");
                } else {
                    assert!(caller.await.unwrap().is_err());
                }
            }
            let report = managed.shutdown().await;
            assert_eq!(report.deadline_expired, mode == 2, "{report:?}");
            assert_closed(&state);
            assert!(
                !metrics
                    .render()
                    .lines()
                    .any(|line| line.starts_with("nidus_http_cancelled_requests_total{"))
            );
        }
    })
    .await
    .expect("managed test streaming exceeded watchdog");
}
