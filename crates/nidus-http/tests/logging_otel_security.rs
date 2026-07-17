use std::{
    convert::Infallible,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{Router, body::Body, body::to_bytes, routing::get, routing::post};
use http::{Method, Request, Response, StatusCode, header};
use nidus_http::{
    logging::{LoggingConfig, LoggingFormat, StructuredMakeSpan},
    middleware::{
        body_limit_layer, security_headers_layer, streaming_body_limit_layer,
        timeout_response_layer, webhook_body_limit_layer,
    },
};
use tower::{ServiceBuilder, ServiceExt, service_fn};
use tower_http::trace::MakeSpan;

#[test]
fn logging_config_builds_production_json_subscriber_with_redaction_metadata() {
    let writer = SharedLogWriter::default();
    let config = LoggingConfig::production("users-api")
        .version("1.2.3")
        .environment("test")
        .redact_header("x-api-key");
    let subscriber = config.subscriber_with_writer(writer.clone());

    tracing::subscriber::with_default(subscriber, || {
        let span = config.service_span();
        let _entered = span.enter();
        tracing::info!(
            request.id = "req-1",
            http.route = "/users/{id}",
            http.method = "GET",
            http.status_code = 200,
            "request completed"
        );
    });

    let logs = writer.contents();
    assert_eq!(config.format(), LoggingFormat::Json);
    assert!(config.redacts_header("x-api-key"));
    assert!(config.redacts_header("X-API-Key"));
    assert!(!config.redacts_header("x-session-id"));
    assert!(logs.contains(r#""message":"request completed""#), "{logs}");
    assert!(logs.contains(r#""service.name":"users-api""#), "{logs}");
    assert!(logs.contains(r#""service.version":"1.2.3""#), "{logs}");
    assert!(
        logs.contains(r#""deployment.environment":"test""#),
        "{logs}"
    );
    assert!(logs.contains(r#""request.id":"req-1""#), "{logs}");
}

#[test]
fn logging_redaction_lookup_is_ascii_case_insensitive_and_order_independent() {
    let config = LoggingConfig::production("users-api")
        .redact_header("x-zeta")
        .redact_header("Authorization")
        .redact_header("x-alpha")
        .redact_header("authorization")
        .redact_header("x-Ä");

    for header in [
        "x-alpha",
        "X-ALPHA",
        "authorization",
        "AUTHORIZATION",
        "x-zeta",
    ] {
        assert!(
            config.redacts_header(header),
            "expected {header} to be redacted"
        );
    }
    assert!(config.redacts_header("X-Ä"));
    assert!(!config.redacts_header("x-ä"));
    assert!(!config.redacts_header("x-session-id"));
}

#[test]
fn structured_make_span_records_service_request_and_route_fields() {
    let writer = SharedLogWriter::default();
    let config = LoggingConfig::production("users-api").environment("test");
    let subscriber = config.subscriber_with_writer(writer.clone());
    let mut make_span = StructuredMakeSpan::new(config).route("/users/{id}");
    let request = Request::builder()
        .method(Method::GET)
        .uri("/users/42")
        .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
        .header(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
        )
        .body(())
        .unwrap();

    tracing::subscriber::with_default(subscriber, || {
        let span = make_span.make_span(&request);
        let fields = span.metadata().unwrap().fields();

        assert!(fields.field("service.name").is_some());
        assert!(fields.field("deployment.environment").is_some());
        assert!(fields.field("request.id").is_some());
        assert!(fields.field("trace.id").is_some());
        assert!(fields.field("http.method").is_some());
        assert!(fields.field("http.route").is_some());

        let _entered = span.enter();
        tracing::info!("request observed");
    });

    let logs = writer.contents();
    assert!(
        logs.contains(r#""request.id":"018f4ad7-56ce-4f6a-a759-29f4438d8d78""#),
        "{logs}"
    );
    assert!(
        logs.contains(r#""trace.id":"4bf92f3577b34da6a3ce929d0e0e4736""#),
        "{logs}"
    );
    assert!(logs.contains(r#""http.route":"/users/{id}""#), "{logs}");
}

#[tokio::test]
async fn security_headers_layer_adds_safe_default_response_headers() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(security_headers_layer());

    let response = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()["x-frame-options"], "DENY");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
}

#[tokio::test]
async fn body_limit_layer_rejects_oversized_requests() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(body_limit_layer(4));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header(header::CONTENT_LENGTH, "5")
                .body(Body::from("12345"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn body_limit_layer_allows_undeclared_body_sizes() {
    let app = Router::new()
        .route("/", post(|body: String| async move { body }))
        .layer(body_limit_layer(4));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .body(Body::from("12345"))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), b"12345");
}

#[tokio::test]
async fn streaming_body_limit_layer_rejects_oversized_body_without_content_length() {
    let app = Router::new()
        .route("/", post(|body: String| async move { body }))
        .layer(streaming_body_limit_layer(4));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/")
                .body(Body::from("12345"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn webhook_body_limit_helper_marks_raw_body_boundary() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(webhook_body_limit_layer(4));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header(header::CONTENT_LENGTH, "5")
                .body(Body::from("12345"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(response.headers()["x-nidus-body-limit"], "webhook-raw-body");
}

#[tokio::test]
async fn timeout_response_layer_maps_elapsed_work_to_http_response() {
    let service = ServiceBuilder::new()
        .layer(timeout_response_layer(Duration::from_millis(1)))
        .service(service_fn(|_request: Request<Body>| async {
            tokio::time::sleep(Duration::from_millis(20)).await;
            Ok::<_, Infallible>(Response::new(Body::from("late")))
        }));

    let response = service
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
}

#[derive(Clone, Default)]
struct SharedLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl SharedLogWriter {
    fn contents(&self) -> String {
        String::from_utf8(self.output.lock().unwrap().clone()).unwrap()
    }
}

impl<'writer> tracing_subscriber::fmt::MakeWriter<'writer> for SharedLogWriter {
    type Writer = SharedLogGuard;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogGuard {
            output: Arc::clone(&self.output),
        }
    }
}

struct SharedLogGuard {
    output: Arc<Mutex<Vec<u8>>>,
}

impl std::io::Write for SharedLogGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
