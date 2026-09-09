use http::{Request, Response};
use nidus_http::middleware::{PrometheusMetrics, route_metrics_layer};
use std::{
    future::{Future, pending, ready},
    pin::Pin,
    task::{Context, Poll, Waker},
    time::Duration,
};
use tower::{Layer, Service, service_fn};

fn pending_service()
-> impl Service<Request<()>, Response = Response<()>, Error = &'static str, Future: Send> + Clone {
    service_fn(|_: Request<()>| pending::<Result<Response<()>, &'static str>>())
}
fn assert_gauge(metrics: &PrometheusMetrics, expected: u64) {
    assert!(
        metrics.render().contains(&format!(
            "nidus_http_in_flight_requests{{method=\"GET\",route=\"/work\"}} {expected}\n"
        )),
        "{}",
        metrics.render()
    );
}
fn poll_once<F: Future + ?Sized>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn cancellation_before_and_after_poll_releases_exactly_once() {
    let metrics = PrometheusMetrics::new();
    let mut service = route_metrics_layer("/work", metrics.clone()).layer(pending_service());
    let first = service.call(Request::new(()));
    let mut second = service.call(Request::new(()));
    assert_gauge(&metrics, 2);
    assert!(poll_once(second.as_mut()).is_pending());
    drop(first);
    assert_gauge(&metrics, 1);
    drop(second);
    assert_gauge(&metrics, 0);
    let rendered = metrics.render();
    assert!(
        rendered
            .contains("nidus_http_cancelled_requests_total{method=\"GET\",route=\"/work\"} 2\n")
    );
    assert!(!rendered.contains("status=\""));
}

#[test]
fn completed_futures_do_not_cancel_or_release_twice() {
    for result in [Ok(Response::new(())), Err("service failure")] {
        let metrics = PrometheusMetrics::new();
        let mut result = Some(result);
        let mut service = route_metrics_layer("/work", metrics.clone()).layer(service_fn(
            move |_: Request<()>| ready(result.take().unwrap()),
        ));
        let mut future = service.call(Request::new(()));
        assert!(poll_once(future.as_mut()).is_ready());
        assert_gauge(&metrics, 0);
        drop(future);
        assert_gauge(&metrics, 0);
        assert!(
            !metrics
                .render()
                .contains("nidus_http_cancelled_requests_total{method=")
        );
        assert!(metrics.render().contains("} 1\n"));
    }
}

#[tokio::test]
async fn outer_timeout_cancels_inner_metrics_and_inner_timeout_records_service_error() {
    let metrics = PrometheusMetrics::new();
    let mut outer = tower::timeout::Timeout::new(
        route_metrics_layer("/work", metrics.clone()).layer(pending_service()),
        Duration::ZERO,
    );
    assert!(outer.call(Request::new(())).await.is_err());
    assert_gauge(&metrics, 0);
    assert!(
        metrics
            .render()
            .contains("nidus_http_cancelled_requests_total{method=")
    );

    let metrics = PrometheusMetrics::new();
    let inner = tower::timeout::Timeout::new(pending_service(), Duration::ZERO);
    let mut service = route_metrics_layer("/work", metrics.clone()).layer(inner);
    assert!(service.call(Request::new(())).await.is_err());
    assert_gauge(&metrics, 0);
    assert!(metrics.render().contains("nidus_http_errors_total{method="));
    assert!(
        !metrics
            .render()
            .contains("nidus_http_cancelled_requests_total{method=")
    );
}

#[test]
fn excluded_requests_never_create_accounting() {
    let metrics = PrometheusMetrics::new();
    let mut service = route_metrics_layer("/metrics", metrics.clone()).layer(pending_service());
    drop(service.call(Request::new(())));
    assert!(!metrics.render().contains("method="));
}

#[test]
fn unwinding_inner_poll_releases_built_in_accounting() {
    let metrics = PrometheusMetrics::new();
    let mut service =
        route_metrics_layer("/work", metrics.clone()).layer(service_fn(|_: Request<()>| async {
            panic!("inner poll panic");
            #[allow(unreachable_code)]
            Ok::<_, &'static str>(Response::new(()))
        }));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut future = service.call(Request::new(()));
            poll_once(future.as_mut())
        }))
        .is_err()
    );
    assert_gauge(&metrics, 0);
}

#[test]
fn custom_hooks_receive_cancellation_and_readiness_is_delegated() {
    use nidus_http::middleware::HttpMetricsHook;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Clone)]
    struct Hook(Arc<AtomicUsize>);
    impl HttpMetricsHook for Hook {
        fn on_request(&self, _: &http::Method, _: Option<&str>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn on_response(&self, _: &http::Method, _: Option<&str>, _: http::StatusCode, _: Duration) {
            panic!("no response")
        }
        fn on_cancel(&self, _: &http::Method, _: Option<&str>, _: Duration) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    struct ReadyService(bool);
    impl Service<Request<()>> for ReadyService {
        type Response = Response<()>;
        type Error = &'static str;
        type Future = std::future::Pending<Result<Response<()>, Self::Error>>;
        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            self.0 = true;
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, _: Request<()>) -> Self::Future {
            assert!(self.0);
            self.0 = false;
            pending()
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let mut service = route_metrics_layer("/work", Hook(count.clone())).layer(ReadyService(false));
    assert!(
        service
            .poll_ready(&mut Context::from_waker(Waker::noop()))
            .is_ready()
    );
    assert_eq!(count.load(Ordering::SeqCst), 0);
    let future = service.call(Request::new(()));
    assert_eq!(count.load(Ordering::SeqCst), 1);
    drop(future);
    assert_eq!(count.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_request_cancellation_releases_all_accounting() {
    use std::sync::Arc;
    let metrics = PrometheusMetrics::new();
    let entered = Arc::new(tokio::sync::Barrier::new(33));
    let release = Arc::new(tokio::sync::Barrier::new(33));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..32 {
        let mut service = route_metrics_layer("/work", metrics.clone()).layer(pending_service());
        let entered = entered.clone();
        let release = release.clone();
        tasks.spawn(async move {
            let future = service.call(Request::new(()));
            entered.wait().await;
            release.wait().await;
            drop(future);
        });
    }
    entered.wait().await;
    assert_gauge(&metrics, 32);
    release.wait().await;
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    assert_gauge(&metrics, 0);
    assert!(
        metrics
            .render()
            .contains("nidus_http_cancelled_requests_total{method=\"GET\",route=\"/work\"} 32")
    );
}

#[test]
fn unwinding_inner_call_releases_accounting() {
    struct Panics;
    impl Service<Request<()>> for Panics {
        type Response = Response<()>;
        type Error = &'static str;
        type Future = std::future::Pending<Result<Response<()>, Self::Error>>;
        fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
        fn call(&mut self, _: Request<()>) -> Self::Future {
            panic!("inner call panic")
        }
    }
    let metrics = PrometheusMetrics::new();
    let mut service = route_metrics_layer("/work", metrics.clone()).layer(Panics);
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(
            service.call(Request::new(()))
        )))
        .is_err()
    );
    assert_gauge(&metrics, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mixed_terminal_outcomes_conserve_accounting_with_exclusion_and_overflow() {
    let metrics = PrometheusMetrics::new().with_max_series(2);
    let mut tasks = tokio::task::JoinSet::new();
    for worker in 0..8 {
        let metrics = metrics.clone();
        tasks.spawn(async move {
            let mut expected = [0_u64; 3];
            for index in 0..96 {
                let kind = (index + worker) % 6;
                let excluded = index % 7 == 0;
                let route = if excluded {
                    "/metrics".to_owned()
                } else {
                    format!("/route-{}", index % 8)
                };
                let inner = service_fn(move |_: Request<()>| async move {
                    match kind {
                        0 | 5 => Ok(Response::new(())),
                        1 => Err("service failure"),
                        4 => panic!("controlled metrics unwind"),
                        _ => pending().await,
                    }
                });
                let mut service = route_metrics_layer(route, metrics.clone()).layer(inner);
                let mut future = service.call(Request::new(()));
                match kind {
                    0 | 5 => {
                        assert!(future.await.is_ok());
                    }
                    1 => {
                        assert!(future.await.is_err());
                    }
                    2 => drop(future),
                    3 => {
                        assert!(poll_once(future.as_mut()).is_pending());
                        drop(future);
                    }
                    4 => {
                        assert!(
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| poll_once(
                                future.as_mut()
                            )))
                            .is_err()
                        );
                        drop(future);
                    }
                    _ => unreachable!(),
                }
                if !excluded {
                    expected[match kind {
                        0 | 5 => 0,
                        1 => 1,
                        _ => 2,
                    }] += 1;
                }
                tokio::task::yield_now().await;
            }
            expected
        });
    }
    let mut expected = [0_u64; 3];
    while let Some(result) = tasks.join_next().await {
        for (total, count) in expected.iter_mut().zip(result.unwrap()) {
            *total += count;
        }
    }
    let rendered = metrics.render();
    let sum = |name: &str| -> u64 {
        rendered
            .lines()
            .filter(|line| line.starts_with(name))
            .map(|line| line.rsplit_once(' ').unwrap().1.parse::<u64>().unwrap())
            .sum()
    };
    assert_eq!(sum("nidus_http_in_flight_requests{"), 0);
    assert!(
        rendered
            .lines()
            .filter(|line| line.starts_with("nidus_http_in_flight_requests{"))
            .all(|line| line.ends_with(" 0"))
    );
    assert_eq!(sum("nidus_http_requests_total{"), expected[0] + expected[1]);
    assert_eq!(sum("nidus_http_errors_total{"), expected[1]);
    assert_eq!(sum("nidus_http_cancelled_requests_total{"), expected[2]);
    assert_eq!(
        sum("nidus_http_request_duration_seconds_count{"),
        expected[0] + expected[1]
    );
    assert!(rendered.contains("<overflow>"));
    assert!(!rendered.contains("route=\"/metrics\""));
    assert!(
        rendered
            .lines()
            .filter(|line| line.starts_with("nidus_http_cancelled_requests_total{"))
            .all(|line| !line.contains("status="))
    );
}
