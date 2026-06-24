use axum::{body::to_bytes, response::IntoResponse};
use http::StatusCode;
use nidus_http::error::HttpError;
use std::sync::{Arc, Mutex};
use tracing::Level;
use tracing_subscriber::{Layer, fmt::MakeWriter, layer::SubscriberExt};

#[derive(Clone, Default)]
struct SharedLogWriter {
    output: Arc<Mutex<Vec<u8>>>,
}

impl SharedLogWriter {
    fn clear(&self) {
        self.output.lock().unwrap().clear();
    }

    fn contents(&self) -> String {
        String::from_utf8(self.output.lock().unwrap().clone()).unwrap()
    }
}

impl<'writer> MakeWriter<'writer> for SharedLogWriter {
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

#[tokio::test]
async fn http_error_maps_to_json_response() {
    let response = HttpError::not_found("user not found").into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["code"], "not_found");
    assert_eq!(json["error"]["message"], "user not found");
}

#[tokio::test]
async fn http_error_supports_custom_status_code_and_message() {
    let error = HttpError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "invalid_user",
        "invalid user",
    );
    let response = error.clone().into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(error.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error.code(), "invalid_user");
    assert_eq!(error.message(), "invalid user");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["error"]["code"], "invalid_user");
    assert_eq!(json["error"]["message"], "invalid user");
}

#[test]
fn http_error_common_helpers_use_stable_codes_and_statuses() {
    let errors = [
        (
            HttpError::bad_request("bad input"),
            StatusCode::BAD_REQUEST,
            "bad_request",
        ),
        (
            HttpError::unauthorized("login required"),
            StatusCode::UNAUTHORIZED,
            "unauthorized",
        ),
        (
            HttpError::forbidden("not allowed"),
            StatusCode::FORBIDDEN,
            "forbidden",
        ),
        (
            HttpError::conflict("already exists"),
            StatusCode::CONFLICT,
            "conflict",
        ),
        (
            HttpError::too_many_requests("slow down"),
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_requests",
        ),
        (
            HttpError::unprocessable_entity("validation failed"),
            StatusCode::UNPROCESSABLE_ENTITY,
            "unprocessable_entity",
        ),
    ];

    for (error, status, code) in errors {
        assert_eq!(error.status(), status);
        assert_eq!(error.code(), code);
    }
}

#[tokio::test]
async fn http_error_can_wrap_internal_failures_without_leaking_details() {
    let response = HttpError::internal_server_error().into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(json["error"]["code"], "internal_server_error");
    assert_eq!(json["error"]["message"], "internal server error");
}

#[test]
fn http_error_emits_structured_tracing_event() {
    let writer = SharedLogWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer.clone())
            .with_ansi(false)
            .with_target(false)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                Level::WARN,
            )),
    );

    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..16 {
            writer.clear();
            tracing_core::callsite::rebuild_interest_cache();
            let _response = HttpError::too_many_requests("slow down").into_response();
            let logs = writer.contents();
            if logs.contains("http error response")
                && logs.contains("http.status=429")
                && logs.contains("error.code=\"too_many_requests\"")
            {
                return;
            }
            std::thread::yield_now();
        }
    });

    let logs = writer.contents();
    assert!(logs.contains("http error response"), "{logs}");
    assert!(logs.contains("http.status=429"), "{logs}");
    assert!(logs.contains("error.code=\"too_many_requests\""), "{logs}");
}

#[allow(dead_code)]
fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn http_error_is_send_sync_static() {
    assert_send_sync_static::<HttpError>();
}
