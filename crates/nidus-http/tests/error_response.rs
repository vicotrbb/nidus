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
