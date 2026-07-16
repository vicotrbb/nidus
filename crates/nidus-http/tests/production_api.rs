use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    body::{Body, Bytes, to_bytes},
    extract::{ConnectInfo, MatchedPath},
    routing::{get, post},
};
use http::{HeaderValue, Method, Request, Response, StatusCode, header::HeaderName};
use nidus_http::{
    error::{ErrorEnvelopeLayer, HttpError},
    health::{HealthRegistry, HealthStatus},
    middleware::{
        ApiDefaults, HttpMetricsHook, InMemoryRateLimitStore, PrometheusMetrics, RateLimitConfig,
        RateLimitDecision, RateLimitError, RateLimitStore, RequestContext, RequestIdConfig,
        RequestIdMode, client_ip_identity, request_context_layer, trusted_proxy_client_ip_identity,
        validated_request_id_layer,
    },
};
use serde_json::json;
use tokio::sync::{Barrier, Notify};
use tower::{ServiceBuilder, ServiceExt, service_fn};

struct NotifyOnDrop(Arc<Notify>);

impl Drop for NotifyOnDrop {
    fn drop(&mut self) {
        self.0.notify_one();
    }
}

#[derive(Clone, Copy)]
struct FailingRateLimitStore;

impl RateLimitStore for FailingRateLimitStore {
    fn check(
        &self,
        _identity: &nidus_http::context::RequestIdentity,
        _limit: u64,
        _window: Duration,
    ) -> Result<RateLimitDecision, RateLimitError> {
        Err(RateLimitError::new("rate limit backend unavailable"))
    }
}

#[tokio::test]
async fn validated_request_id_accepts_valid_ids_and_inserts_context() {
    let app = Router::new()
        .route(
            "/context",
            get(|context: RequestContext| async move {
                Json(json!({
                    "requestId": context.request_id(),
                    "correlationId": context.correlation_id(),
                    "method": context.method().as_str(),
                    "route": context.route(),
                    "path": context.path(),
                    "clientKind": context.client_kind().as_str(),
                }))
            }),
        )
        .layer(request_context_layer())
        .layer(validated_request_id_layer(
            RequestIdConfig::production().mode(RequestIdMode::Strict),
        ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/context")
                .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
                .header("x-correlation-id", "corr-123")
                .header("x-api-key", "secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let request_id = response.headers().get("x-request-id").cloned();
    let body = response_json(response).await;

    assert_eq!(request_id.unwrap(), "018f4ad7-56ce-4f6a-a759-29f4438d8d78");
    assert_eq!(body["requestId"], "018f4ad7-56ce-4f6a-a759-29f4438d8d78");
    assert_eq!(body["correlationId"], "corr-123");
    assert_eq!(body["method"], "GET");
    assert_eq!(body["route"], "/context");
    assert_eq!(body["path"], "/context");
    assert_eq!(body["clientKind"], "api_key");
}

#[tokio::test]
async fn validated_request_id_preserves_a_custom_header_name() {
    let header_name = HeaderName::from_static("x-nidus-request-id");
    let app = Router::new()
        .route(
            "/context",
            get(|context: RequestContext| async move { context.request_id().to_owned() }),
        )
        .layer(validated_request_id_layer(
            RequestIdConfig::production().header_name(header_name.clone()),
        ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/context")
                .header(header_name.clone(), "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get(&header_name).unwrap(),
        "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
    );
    assert!(!response.headers().contains_key("x-request-id"));
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"018f4ad7-56ce-4f6a-a759-29f4438d8d78");
}

#[tokio::test]
async fn strict_request_id_policy_rejects_malformed_incoming_ids() {
    let app = Router::new()
        .route("/", get(|| async { "unreached" }))
        .layer(validated_request_id_layer(
            RequestIdConfig::production().mode(RequestIdMode::Strict),
        ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-request-id", "not a uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let request_id = response.headers().get("x-request-id").cloned();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(request_id.is_some());
    assert_eq!(body["error"]["code"], "invalid_request_id");
    assert_eq!(
        body["error"]["requestId"],
        request_id.unwrap().to_str().unwrap()
    );
}

#[tokio::test]
async fn permissive_request_id_policy_replaces_malformed_ids() {
    let service = ServiceBuilder::new()
        .layer(validated_request_id_layer(
            RequestIdConfig::production().mode(RequestIdMode::Permissive),
        ))
        .service(service_fn(|request: Request<Body>| async move {
            let context = request.extensions().get::<RequestContext>().unwrap();
            Ok::<_, Infallible>(Response::new(Body::from(context.request_id().to_owned())))
        }));

    let response = service
        .oneshot(
            Request::builder()
                .header("x-request-id", "bad")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let header = response.headers().get("x-request-id").cloned().unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_ne!(header, HeaderValue::from_static("bad"));
    assert_eq!(body.as_ref(), header.to_str().unwrap().as_bytes());
}

#[tokio::test]
async fn generated_request_id_rejects_invalid_header_values_without_calling_inner_service() {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = ServiceBuilder::new()
        .layer(validated_request_id_layer(
            RequestIdConfig::production().generator(|| "bad\nrequest-id".to_owned()),
        ))
        .service(service_fn({
            let calls = Arc::clone(&calls);
            move |_request: Request<Body>| {
                let calls = Arc::clone(&calls);
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok::<_, Infallible>(Response::new(Body::from("unreached")))
                }
            }
        }));

    let response = service
        .oneshot(
            Request::builder()
                .uri("/missing")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(body["error"]["code"], "invalid_generated_request_id");
    assert_eq!(body["error"]["path"], "/missing");
}

#[tokio::test]
async fn strict_request_id_rejection_handles_invalid_generated_error_ids() {
    let app = Router::new()
        .route("/", get(|| async { "unreached" }))
        .layer(validated_request_id_layer(
            RequestIdConfig::production().generator(|| "bad\nrequest-id".to_owned()),
        ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/strict")
                .header("x-request-id", "not-a-uuid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["error"]["code"], "invalid_generated_request_id");
    assert_eq!(body["error"]["path"], "/strict");
}

#[tokio::test]
async fn error_envelope_includes_request_id_path_and_timestamp() {
    let app = Router::new()
        .route(
            "/users/42",
            get(|| async { HttpError::not_found("missing") }),
        )
        .layer(ErrorEnvelopeLayer::new())
        .layer(request_context_layer())
        .layer(validated_request_id_layer(
            RequestIdConfig::production().mode(RequestIdMode::Strict),
        ));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/users/42")
                .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["statusCode"], 404);
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "missing");
    assert_eq!(body["error"]["path"], "/users/42");
    assert_eq!(
        body["error"]["requestId"],
        "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
    );
    assert!(body["error"]["timestamp"].as_str().unwrap().ends_with('Z'));
}

#[tokio::test]
async fn error_envelope_preserves_legacy_json_error_details() {
    let service = ServiceBuilder::new()
        .layer(ErrorEnvelopeLayer::new())
        .service(service_fn(|_request: Request<Body>| async move {
            Ok::<_, Infallible>(
                Response::builder()
                    .status(StatusCode::UNPROCESSABLE_ENTITY)
                    .body(Body::from(
                        r#"{"error":{"code":"invalid_user","message":"invalid user","field":"email"}}"#,
                    ))
                    .unwrap(),
            )
        }));

    let response = service
        .oneshot(
            Request::builder()
                .uri("/legacy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["error"]["code"], "invalid_user");
    assert_eq!(body["error"]["message"], "invalid user");
    assert_eq!(body["error"]["details"]["field"], "email");
    assert_eq!(body["error"]["path"], "/legacy");
}

#[tokio::test]
async fn error_envelope_wraps_non_json_error_bodies() {
    let service = ServiceBuilder::new()
        .layer(ErrorEnvelopeLayer::new())
        .service(service_fn(|_request: Request<Body>| async move {
            Ok::<_, Infallible>(
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("plain missing"))
                    .unwrap(),
            )
        }));

    let response = service
        .oneshot(
            Request::builder()
                .uri("/plain")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "Not Found");
    assert_eq!(body["error"]["details"], serde_json::Value::Null);
    assert_eq!(body["error"]["path"], "/plain");
}

#[tokio::test]
async fn error_envelope_masks_5xx_legacy_error_code_message_and_details() {
    let service = ServiceBuilder::new()
        .layer(ErrorEnvelopeLayer::new())
        .service(service_fn(|_request: Request<Body>| async move {
            Ok::<_, Infallible>(
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(
                        r#"{"error":{"code":"database_error","message":"database password leaked","secret":"value"}}"#,
                    ))
                    .unwrap(),
            )
        }));

    let response = service
        .oneshot(
            Request::builder()
                .uri("/panic")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    // ERR-1: the internal taxonomy (`database_error`) must NOT leak to the
    // client on a 5xx; only the generic `internal_server_error` code is exposed.
    // The original code is preserved in the server log (envelope_response).
    assert_eq!(body["error"]["code"], "internal_server_error");
    assert_eq!(body["error"]["message"], "internal server error");
    assert_eq!(body["error"]["details"], serde_json::Value::Null);
    assert_eq!(body["error"]["path"], "/panic");
}

#[tokio::test]
async fn error_envelope_skips_oversized_legacy_error_bodies() {
    let oversized_message = "x".repeat(128 * 1024);
    let oversized_body = serde_json::to_string(&json!({
        "error": {
            "code": "oversized_legacy",
            "message": oversized_message,
        }
    }))
    .unwrap();
    let service = ServiceBuilder::new()
        .layer(ErrorEnvelopeLayer::new())
        .service(service_fn(move |_request: Request<Body>| {
            let oversized_body = oversized_body.clone();
            async move {
                Ok::<_, Infallible>(
                    Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .body(Body::from(oversized_body))
                        .unwrap(),
                )
            }
        }));

    let response = service
        .oneshot(Request::builder().uri("/huge").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "bad_request");
    assert_eq!(body["error"]["message"], "Bad Request");
    assert_eq!(body["error"]["path"], "/huge");
}

#[tokio::test]
async fn health_registry_runs_ready_checks_in_parallel_and_controls_details() {
    let ready_barrier = Arc::new(Barrier::new(2));
    let database_barrier = Arc::clone(&ready_barrier);
    let cache_barrier = Arc::clone(&ready_barrier);
    let registry = HealthRegistry::new()
        .live_check_sync("process", HealthStatus::up)
        .ready_check("database", move || {
            let ready_barrier = Arc::clone(&database_barrier);
            async move {
                ready_barrier.wait().await;
                HealthStatus::up()
            }
        })
        .ready_check("cache", move || {
            let ready_barrier = Arc::clone(&cache_barrier);
            async move {
                ready_barrier.wait().await;
                HealthStatus::down("cache unavailable")
            }
        })
        .timeout(Duration::from_secs(1))
        .hide_details();
    let app = Router::new().merge(registry.routes());

    let live = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/health/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);

    let ready = app
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = ready.status();
    let body = response_json(ready).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "down");
    assert_eq!(body["checks"]["database"]["status"], "up");
    assert_eq!(body["checks"]["cache"]["status"], "down");
    assert!(body["checks"]["cache"].get("message").is_none());
}

#[tokio::test]
async fn health_registry_reports_timed_out_checks_as_down() {
    let registry = HealthRegistry::new()
        .ready_check("database", std::future::pending::<HealthStatus>)
        .timeout(Duration::from_millis(20));
    let app = Router::new().merge(registry.routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "down");
    assert_eq!(body["checks"]["database"]["status"], "down");
    assert_eq!(body["checks"]["database"]["message"], "check timed out");
}

#[tokio::test]
async fn health_registry_cancels_checks_when_request_is_cancelled() {
    let started = Arc::new(Notify::new());
    let cancelled = Arc::new(Notify::new());
    let registry = HealthRegistry::new()
        .ready_check("database", {
            let started = Arc::clone(&started);
            let cancelled = Arc::clone(&cancelled);
            move || {
                let started = Arc::clone(&started);
                let cancelled = Arc::clone(&cancelled);
                async move {
                    let _notify_on_drop = NotifyOnDrop(cancelled);
                    started.notify_one();
                    std::future::pending::<HealthStatus>().await
                }
            }
        })
        .timeout(Duration::from_secs(60));
    let app = Router::new().merge(registry.routes());

    let request = tokio::spawn(
        app.oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        ),
    );
    tokio::time::timeout(Duration::from_secs(1), started.notified())
        .await
        .expect("health check should start before the request is cancelled");
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());

    tokio::time::timeout(Duration::from_secs(1), cancelled.notified())
        .await
        .expect("cancelled health requests should abort unfinished checks");
}

#[tokio::test]
async fn health_registry_reports_panicking_checks_as_down() {
    let registry = HealthRegistry::new()
        .ready_check_sync("cache", || panic!("cache client panicked"))
        .ready_check("queue", || async { panic!("queue client panicked") })
        .timeout(Duration::from_secs(1));
    let app = Router::new().merge(registry.routes());

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response_json(response).await;

    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "down");
    assert_eq!(body["checks"]["cache"]["status"], "down");
    assert_eq!(body["checks"]["cache"]["message"], "check panicked");
    assert_eq!(body["checks"]["queue"]["status"], "down");
    assert_eq!(body["checks"]["queue"]["message"], "check panicked");
}

#[tokio::test]
async fn prometheus_metrics_uses_matched_routes_and_excludes_internal_paths() {
    let metrics = PrometheusMetrics::new().exclude_route("/metrics");
    let app = Router::new()
        .route("/users/{id}", get(|| async { "ok" }))
        .route(
            "/metrics",
            get({
                let metrics = metrics.clone();
                move || async move { metrics.render() }
            }),
        )
        .layer(metrics.layer());

    let _ = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/users/42")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();

    assert!(text.contains(r#"route="/users/{id}""#), "{text}");
    assert!(!text.contains(r#"route="/users/42""#), "{text}");
    assert!(!text.contains(r#"route="/metrics""#), "{text}");
}

#[test]
fn prometheus_metrics_renders_bounded_duration_histogram_buckets() {
    let metrics = PrometheusMetrics::new();

    for milliseconds in 1..=1000 {
        metrics.on_request(&Method::GET, Some("/bulk"));
        metrics.on_response(
            &Method::GET,
            Some("/bulk"),
            StatusCode::OK,
            Duration::from_millis(milliseconds % 20),
        );
    }

    let text = metrics.render();

    assert!(
        text.contains(
            r#"nidus_http_request_duration_seconds_bucket{method="GET",route="/bulk",status="200",le="0.005"}"#
        ),
        "{text}"
    );
    assert!(
        text.contains(
            r#"nidus_http_request_duration_seconds_bucket{method="GET",route="/bulk",status="200",le="+Inf"} 1000"#
        ),
        "{text}"
    );
    assert!(
        text.contains(
            r#"nidus_http_request_duration_seconds_count{method="GET",route="/bulk",status="200"} 1000"#
        ),
        "{text}"
    );
}

#[test]
fn prometheus_metrics_counts_inner_service_errors_as_requests() {
    let metrics = PrometheusMetrics::new();

    metrics.on_request(&Method::GET, Some("/fallible"));
    metrics.on_error(&Method::GET, Some("/fallible"), Duration::from_millis(25));

    let text = metrics.render();

    assert!(
        text.contains(
            r#"nidus_http_requests_total{method="GET",route="/fallible",status="500"} 1"#
        ),
        "{text}"
    );
    assert!(
        text.contains(
            r#"nidus_http_request_duration_seconds_count{method="GET",route="/fallible",status="500"} 1"#
        ),
        "{text}"
    );
    assert!(
        text.contains(r#"nidus_http_errors_total{method="GET",route="/fallible",status="500"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"nidus_http_in_flight_requests{method="GET",route="/fallible"} 0"#),
        "{text}"
    );
}

#[test]
fn prometheus_metrics_escapes_label_values() {
    let metrics = PrometheusMetrics::new();

    metrics.on_request(&Method::GET, Some("/quoted\"route\\line\nbreak"));
    metrics.on_response(
        &Method::GET,
        Some("/quoted\"route\\line\nbreak"),
        StatusCode::OK,
        Duration::from_millis(1),
    );

    let text = metrics.render();

    assert!(
        text.contains(r#"route="/quoted\"route\\line\nbreak""#),
        "{text}"
    );
    assert!(
        !text.contains("line\nbreak"),
        "raw newlines must be escaped in label values: {text}"
    );
}

#[test]
fn prometheus_metrics_records_high_cardinality_routes_explicitly() {
    let metrics = PrometheusMetrics::new();

    for route in (0..25).map(|index| format!("/users/{index}")) {
        metrics.on_request(&Method::GET, Some(&route));
        metrics.on_response(
            &Method::GET,
            Some(&route),
            StatusCode::OK,
            Duration::from_millis(1),
        );
    }

    let text = metrics.render();

    assert_eq!(text.matches("nidus_http_requests_total").count(), 26);
    assert!(text.contains(r#"route="/users/0""#), "{text}");
    assert!(text.contains(r#"route="/users/24""#), "{text}");
}

#[test]
fn prometheus_metrics_caps_distinct_routes_when_max_series_configured() {
    let metrics = PrometheusMetrics::new().with_max_series(2);

    for route in ["/a", "/b", "/c"] {
        metrics.on_request(&Method::GET, Some(route));
        metrics.on_response(
            &Method::GET,
            Some(route),
            StatusCode::OK,
            Duration::from_millis(1),
        );
    }

    let text = metrics.render();

    // The first two distinct routes fit within the configured cap; the third
    // overflows into a single shared bucket so series count can never grow
    // unbounded regardless of how many distinct labels are observed.
    assert!(text.contains(r#"route="/a""#), "{text}");
    assert!(text.contains(r#"route="/b""#), "{text}");
    assert!(
        !text.contains(r#"route="/c""#),
        "route beyond the cap must overflow, not create a new series: {text}"
    );
    assert!(
        text.contains(r#"route="<overflow>""#),
        "overflow bucket must be present: {text}"
    );
}

#[test]
fn prometheus_metrics_unbounded_by_default_admits_all_routes() {
    let metrics = PrometheusMetrics::new();

    for route in ["/a", "/b", "/c", "/d"] {
        metrics.on_request(&Method::GET, Some(route));
        metrics.on_response(
            &Method::GET,
            Some(route),
            StatusCode::OK,
            Duration::from_millis(1),
        );
    }

    let text = metrics.render();
    assert!(text.contains(r#"route="/a""#), "{text}");
    assert!(text.contains(r#"route="/d""#), "{text}");
    assert!(
        !text.contains(r#"route="<overflow>""#),
        "no overflow bucket when cap is not configured: {text}"
    );
}

#[test]
fn prometheus_metrics_can_render_while_recording_concurrently() {
    let metrics = PrometheusMetrics::new();
    let writers = (0..4)
        .map(|worker| {
            let metrics = metrics.clone();
            std::thread::spawn(move || {
                for index in 0..200 {
                    let route = format!("/workers/{worker}/requests/{index}");
                    metrics.on_request(&Method::GET, Some(&route));
                    metrics.on_response(
                        &Method::GET,
                        Some(&route),
                        StatusCode::OK,
                        Duration::from_micros(250),
                    );
                }
            })
        })
        .collect::<Vec<_>>();

    for _ in 0..50 {
        let text = metrics.render();
        assert!(text.contains("# TYPE nidus_http_requests_total counter"));
    }

    for writer in writers {
        writer.join().unwrap();
    }

    let text = metrics.render();
    assert!(text.contains(r#"route="/workers/0/requests/0""#), "{text}");
    assert!(
        text.contains(r#"route="/workers/3/requests/199""#),
        "{text}"
    );
}

#[tokio::test]
async fn production_api_defaults_composes_routes_layers_and_overrides() {
    async fn matched_path(path: MatchedPath) -> String {
        path.as_str().to_owned()
    }

    let metrics = PrometheusMetrics::new();
    let defaults = ApiDefaults::production("users-api")
        .metrics(metrics.clone())
        .request_ids(RequestIdConfig::production().mode(RequestIdMode::Strict))
        .rate_limit(
            RateLimitConfig::new(1, Duration::from_secs(60), InMemoryRateLimitStore::new())
                .identity(client_ip_identity())
                .fail_closed(),
        );
    let app = defaults.apply(Router::new().route("/users/{id}", get(matched_path)).route(
        "/metrics",
        get({
            let metrics = metrics.clone();
            move || async move { metrics.render() }
        }),
    ));

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/users/42")
                .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(
        first.headers().get("x-request-id").unwrap(),
        "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
    );

    let second = app
        .oneshot(
            Request::builder()
                .uri("/users/43")
                .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d79")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(second.headers().contains_key("retry-after"));
}

#[tokio::test]
async fn rate_limit_store_errors_preserve_fail_open_and_fail_closed_policies() {
    let router = || Router::new().route("/", get(|| async { "ok" }));
    let config = || RateLimitConfig::new(10, Duration::from_secs(60), FailingRateLimitStore);

    let fail_open = router()
        .layer(config().fail_open().layer())
        .oneshot(Request::new(Body::empty()))
        .await
        .unwrap();
    let fail_closed = router()
        .layer(config().fail_closed().layer())
        .oneshot(Request::new(Body::empty()))
        .await
        .unwrap();

    assert_eq!(fail_open.status(), StatusCode::OK);
    assert_eq!(fail_open.headers()["ratelimit-limit"], "10");
    assert_eq!(fail_closed.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(fail_closed.headers().contains_key("retry-after"));
}

#[tokio::test]
async fn trusted_proxy_rate_limit_cannot_be_bypassed_with_a_spoofed_prefix() {
    let trusted_proxy = "127.0.0.1".parse::<IpAddr>().unwrap();
    let app = Router::new().route("/", get(|| async { "ok" })).layer(
        RateLimitConfig::new(1, Duration::from_secs(60), InMemoryRateLimitStore::new())
            .identity(trusted_proxy_client_ip_identity([trusted_proxy]))
            .layer(),
    );
    let request = |forwarded_for: &'static str| {
        Request::builder()
            .uri("/")
            .header("x-forwarded-for", forwarded_for)
            .extension(ConnectInfo(SocketAddr::new(trusted_proxy, 5000)))
            .body(Body::empty())
            .unwrap()
    };

    let first = app
        .clone()
        .oneshot(request("198.51.100.200, 203.0.113.10"))
        .await
        .unwrap();
    let second = app
        .oneshot(request("192.0.2.200, 203.0.113.10"))
        .await
        .unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn production_api_defaults_apply_security_headers_body_limit_and_timeout() {
    let metrics = PrometheusMetrics::new();
    let app = ApiDefaults::production("users-api")
        .metrics(metrics.clone())
        .body_limit(4)
        .timeout(Duration::from_millis(1))
        .apply(
            Router::new()
                .route(
                    "/slow",
                    get(|| async {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        "late"
                    }),
                )
                .merge(metrics.routes()),
        );

    let oversized = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/slow")
                .header("content-length", "5")
                .body(Body::from("12345"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(oversized.headers()["x-content-type-options"], "nosniff");

    let timeout = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/slow")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(timeout.status(), StatusCode::REQUEST_TIMEOUT);
    assert!(timeout.headers().contains_key("x-request-id"));
    assert_eq!(timeout.headers()["x-content-type-options"], "nosniff");

    let metrics = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(metrics.into_body(), usize::MAX).await.unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains(r#"nidus_http_requests_total{method="GET",route="/slow",status="408"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"nidus_http_in_flight_requests{method="GET",route="/slow"} 0"#),
        "{text}"
    );
}

#[tokio::test]
async fn production_defaults_envelope_meter_and_identify_handler_errors() {
    // Pins the documented production middleware order by asserting that a single
    // handler-produced 500 is simultaneously:
    //   - wrapped by the production error envelope (ErrorEnvelope is outside the handler)
    //   - recorded by metrics (metrics is outside ErrorEnvelope)
    //   - given an x-request-id (request-id layer is outside metrics)
    //   - given security headers (security headers are outermost)
    // Reordering any of these layers relative to the handler would fail an assertion.
    let metrics = PrometheusMetrics::new();
    let app = ApiDefaults::production("users-api")
        .metrics(metrics.clone())
        .apply(
            Router::new()
                .route(
                    "/boom",
                    get(|| async {
                        Err::<&'static str, HttpError>(HttpError::internal_server_error())
                    }),
                )
                .merge(metrics.routes()),
        );

    let response = app
        .clone()
        .oneshot(Request::builder().uri("/boom").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(response.headers().contains_key("x-request-id"));
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let body = response_json(response).await;
    assert_eq!(body["error"]["statusCode"], 500);
    assert_eq!(body["error"]["message"], "internal server error");
    assert!(body["error"]["requestId"].is_string());

    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        to_bytes(metrics_response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        text.contains(r#"nidus_http_requests_total{method="GET",route="/boom",status="500"} 1"#),
        "{text}"
    );
}

#[tokio::test]
async fn body_limit_without_streaming_cap_is_bypassed_without_content_length() {
    // F-HTTP-2 (documents the gap): the default body_limit checks the
    // Content-Length header only, so a body without that header (chunked-transfer
    // shape) is not rejected even when it exceeds the configured limit. The body
    // reaches the handler, which reads it in full here.
    let app = ApiDefaults::production("users-api")
        .body_limit(4)
        .apply(Router::new().route(
            "/echo",
            post(|bytes: Bytes| async move { bytes.len().to_string() }),
        ));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/echo")
                // Intentionally no `content-length` header.
                .body(Body::from(vec![b'a'; 1024]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "a headerless body bypasses the Content-Length-only limit"
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&*body, b"1024");
}

#[tokio::test]
async fn streaming_body_limit_caps_bodies_without_content_length() {
    // F-HTTP-2 (the opt-in fix): streaming_body_limit wraps the request body and
    // caps bytes as they are read, so a headerless/chunked body that bypasses
    // body_limit is still rejected when the handler reads past the cap.
    let app = ApiDefaults::production("users-api")
        .body_limit(4)
        .streaming_body_limit(4)
        .apply(Router::new().route("/echo", post(|_bytes: Bytes| async move { "ok" })));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/echo")
                // Intentionally no `content-length` header.
                .body(Body::from(vec![b'a'; 1024]))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "streaming_body_limit must cap a headerless body as it is read"
    );
}

#[tokio::test]
async fn production_defaults_envelope_and_meter_body_limit_rejections() {
    // F-HTTP-3: an oversized-body 413 must be enveloped, metered, and carry a
    // request id, like a timeout (408), instead of being silently rejected
    // before the observability layers.
    let metrics = PrometheusMetrics::new();
    let app = ApiDefaults::production("users-api")
        .metrics(metrics.clone())
        .body_limit(4)
        .apply(
            Router::new()
                .route("/echo", get(|| async { "ok" }))
                .merge(metrics.routes()),
        );

    let oversized = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/echo")
                .header("content-length", "5")
                .body(Body::from("12345"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert!(
        oversized.headers().contains_key("x-request-id"),
        "413 must carry a request id"
    );
    assert_eq!(oversized.headers()["x-content-type-options"], "nosniff");
    let body = response_json(oversized).await;
    assert_eq!(body["error"]["statusCode"], 413, "413 must be enveloped");

    let metrics_response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        to_bytes(metrics_response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        text.contains(r#"status="413""#),
        "413 must be metered: {text}"
    );
}

#[tokio::test]
async fn production_defaults_envelope_unmatched_routes_as_not_found() {
    let metrics = PrometheusMetrics::new();
    let app = ApiDefaults::production("users-api")
        .metrics(metrics.clone())
        .apply(
            Router::new()
                .route("/users", get(|| async { "ok" }))
                .merge(metrics.routes()),
        );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/missing")
                .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(response.headers()["content-type"], "application/json");
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        response.headers()["x-request-id"],
        "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
    );
    let body = response_json(response).await;
    assert_eq!(body["error"]["statusCode"], 404);
    assert_eq!(body["error"]["code"], "not_found");
    assert_eq!(body["error"]["message"], "route not found");
    assert_eq!(body["error"]["path"], "/missing");
    assert_eq!(
        body["error"]["requestId"],
        "018f4ad7-56ce-4f6a-a759-29f4438d8d78"
    );
    assert!(body["error"]["timestamp"].as_str().unwrap().ends_with('Z'));

    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        to_bytes(metrics_response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        text.contains(
            r#"nidus_http_requests_total{method="GET",route="<unknown>",status="404"} 1"#
        ),
        "404 fallback must be metered by the production stack: {text}"
    );
}

#[tokio::test]
async fn production_defaults_envelope_panic_as_500() {
    // F-HTTP-7: a panicking handler under the production stack must yield a
    // structured 500 envelope (with request-id) instead of an aborted
    // connection. Requires CatchPanicLayer inside the production stack.
    async fn panicking_handler() -> &'static str {
        panic!("handler panicked");
    }
    let metrics = PrometheusMetrics::new();
    let app = ApiDefaults::production("users-api")
        .metrics(metrics.clone())
        .apply(
            Router::new()
                .route("/panic", get(panicking_handler))
                .merge(metrics.routes()),
        );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/panic")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert!(
        response.headers().contains_key("x-request-id"),
        "panic response must carry a request id"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    let body = response_json(response).await;
    assert_eq!(body["error"]["statusCode"], 500);
    assert_eq!(body["error"]["message"], "internal server error");
    assert!(body["error"]["requestId"].is_string());

    // The panic must also be metered like any other request.
    let metrics_response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let text = String::from_utf8(
        to_bytes(metrics_response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap();
    assert!(
        text.contains(r#"nidus_http_requests_total{method="GET",route="/panic",status="500"} 1"#),
        "{text}"
    );
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
