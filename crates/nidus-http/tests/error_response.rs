use axum::{body::to_bytes, response::IntoResponse};
use http::StatusCode;
use nidus_http::error::HttpError;

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

#[allow(dead_code)]
fn assert_send_sync_static<T: Send + Sync + 'static>() {}

#[test]
fn http_error_is_send_sync_static() {
    assert_send_sync_static::<HttpError>();
}
