use std::convert::Infallible;

use axum::{Router, body::to_bytes, response::IntoResponse, routing::post};
use garde::Validate;
use http::StatusCode;
use nidus_validation::{Pipe, ValidatedJson, ValidationPipe};
use serde::Deserialize;
use tower::ServiceExt;

#[derive(Debug, Deserialize, Validate)]
struct CreateUser {
    #[garde(email)]
    email: String,
}

#[derive(Debug, Deserialize, Validate)]
struct UserProfile {
    #[garde(length(min = 2))]
    display_name: String,
}

#[derive(Debug, Deserialize, Validate)]
struct CreateTeam {
    #[garde(dive)]
    owner: UserProfile,
    #[garde(dive)]
    members: Vec<UserProfile>,
}

struct TrimEmailPipe;

impl Pipe<CreateUser> for TrimEmailPipe {
    type Output = CreateUser;
    type Error = Infallible;

    fn transform(&self, mut input: CreateUser) -> Result<Self::Output, Self::Error> {
        input.email = input.email.trim().to_owned();
        Ok(input)
    }
}

#[test]
fn validation_pipe_accepts_valid_values() {
    let input = CreateUser {
        email: "user@nidus.dev".to_owned(),
    };

    let output = ValidationPipe::new().transform(input).unwrap();

    assert_eq!(output.email, "user@nidus.dev");
}

#[test]
fn validation_pipe_rejects_invalid_values() {
    let input = CreateUser {
        email: "not-an-email".to_owned(),
    };

    let error = ValidationPipe::new().transform(input).unwrap_err();

    assert!(error.to_string().contains("email"));
}

#[test]
fn validation_errors_expose_field_level_details() {
    let input = CreateUser {
        email: "not-an-email".to_owned(),
    };

    let error = ValidationPipe::new().transform(input).unwrap_err();
    let fields = error.field_errors();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field(), "email");
    assert_eq!(fields[0].code(), "email");
    assert_eq!(
        fields[0].message(),
        Some("not a valid email: value is missing `@`")
    );
}

#[test]
fn validation_errors_flatten_nested_field_paths() {
    let input = CreateTeam {
        owner: UserProfile {
            display_name: "A".to_owned(),
        },
        members: vec![UserProfile {
            display_name: "B".to_owned(),
        }],
    };

    let error = ValidationPipe::new().transform(input).unwrap_err();
    let fields = error.field_errors();

    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].field(), "members[0].display_name");
    assert_eq!(fields[0].code(), "length");
    assert_eq!(fields[0].message(), Some("length is lower than 2"));
    assert_eq!(fields[1].field(), "owner.display_name");
    assert_eq!(fields[1].code(), "length");
    assert_eq!(fields[1].message(), Some("length is lower than 2"));
}

#[tokio::test]
async fn validation_errors_map_to_stable_json_response() {
    let input = CreateUser {
        email: "not-an-email".to_owned(),
    };

    let error = ValidationPipe::new().transform(input).unwrap_err();
    assert_eq!(error.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error.code(), "validation_failed");

    let response = error.into_response();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["error"]["code"], "validation_failed");
    assert_eq!(json["error"]["message"], "request validation failed");
    assert_eq!(json["error"]["fields"][0]["field"], "email");
    assert_eq!(json["error"]["fields"][0]["code"], "email");
    assert_eq!(
        json["error"]["fields"][0]["message"],
        "not a valid email: value is missing `@`"
    );
}

#[test]
fn custom_pipe_transforms_request_values() {
    let input = CreateUser {
        email: " user@nidus.dev ".to_owned(),
    };

    let output = TrimEmailPipe.transform(input).unwrap();

    assert_eq!(output.email, "user@nidus.dev");
}

#[test]
fn validation_pipe_implements_typed_pipe_trait() {
    let input = CreateUser {
        email: "user@nidus.dev".to_owned(),
    };

    let output =
        <ValidationPipe as Pipe<CreateUser>>::transform(&ValidationPipe::new(), input).unwrap();

    assert_eq!(output.email, "user@nidus.dev");
}

#[tokio::test]
async fn validated_json_extractor_accepts_valid_bodies() {
    async fn create_user(ValidatedJson(input): ValidatedJson<CreateUser>) -> String {
        input.email
    }

    let app = Router::new().route("/users", post(create_user));
    let response = app
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/users")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email":"user@nidus.dev"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..], b"user@nidus.dev");
}

#[tokio::test]
async fn validated_json_extractor_rejects_invalid_bodies_with_validation_response() {
    async fn create_user(ValidatedJson(input): ValidatedJson<CreateUser>) -> String {
        input.email
    }

    let app = Router::new().route("/users", post(create_user));
    let response = app
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/users")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email":"not-an-email"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(json["error"]["code"], "validation_failed");
    assert_eq!(json["error"]["fields"][0]["field"], "email");
}

/// Audit V-1: a body that is syntactically invalid JSON must be rejected with a
/// 400 (Axum's `JsonRejection`) and never reach the 422 validation path, which
/// is reserved for structurally-valid input that fails business rules.
#[tokio::test]
async fn validated_json_extractor_rejects_malformed_json_with_bad_request() {
    async fn create_user(ValidatedJson(input): ValidatedJson<CreateUser>) -> String {
        input.email
    }

    let app = Router::new().route("/users", post(create_user));
    let response = app
        .oneshot(
            http::Request::builder()
                .method(http::Method::POST)
                .uri("/users")
                .header(http::header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(r#"{"email": broken json}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "malformed JSON must yield 400, not 422"
    );
}
