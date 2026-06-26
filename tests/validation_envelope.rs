//! Cross-crate composition tests (audit V-1).
//!
//! These pin the contract between `nidus_validation` (the `ValidatedJson`
//! extractor and its 422 `fields` payload) and `nidus_http`'s production
//! `ErrorEnvelopeLayer`: a validation failure must surface through the envelope
//! with its `code`, `message`, and field-level `details` intact, plus the
//! envelope's own `statusCode` / `timestamp` / `path` / `requestId` metadata.

use axum::{
    Router,
    body::to_bytes,
    http::{self, header},
    routing::post,
};
use nidus_http::error::ErrorEnvelopeLayer;
use nidus_validation::ValidatedJson;
use serde::Deserialize;
use tower::ServiceExt;
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
}

async fn create_user(ValidatedJson(input): ValidatedJson<CreateUser>) -> String {
    input.email
}

fn app() -> Router {
    Router::new()
        .route("/users", post(create_user))
        .layer(ErrorEnvelopeLayer::new())
}

#[tokio::test]
async fn validation_422_is_enveloped_with_field_details_intact() {
    let response = app()
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/users")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email":"not-an-email"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), http::StatusCode::UNPROCESSABLE_ENTITY);

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let error = &json["error"];

    // Envelope metadata is added.
    assert_eq!(error["statusCode"], 422);
    assert!(error["timestamp"].as_str().is_some(), "timestamp missing");
    assert_eq!(error["path"], "/users");

    // The validation taxonomy is preserved (not masked, since 4xx is client-visible).
    assert_eq!(error["code"], "validation_failed");
    assert_eq!(error["message"], "request validation failed");

    // Field-level details survive the legacy-body flatten into `details`.
    assert_eq!(error["details"]["fields"][0]["field"], "email");
    assert_eq!(error["details"]["fields"][0]["code"], "email");
}

#[tokio::test]
async fn validation_passes_through_envelope_unchanged_on_success() {
    let response = app()
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/users")
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email":"user@nidus.dev"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), http::StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"user@nidus.dev");
}
