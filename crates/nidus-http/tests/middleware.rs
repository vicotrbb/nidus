use std::{convert::Infallible, time::Duration};

use axum::{Router, body::Body, routing::get};
use http::{
    Method, Request, Response, StatusCode,
    header::{
        ACCEPT_ENCODING, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_REQUEST_METHOD,
        CONTENT_ENCODING, HeaderName, ORIGIN,
    },
};
use nidus_http::middleware::{compression_layer, cors_layer, request_id_layer, timeout_layer};
use tokio::time::sleep;
use tower::{ServiceBuilder, ServiceExt, service_fn};

#[tokio::test]
async fn request_id_layer_adds_response_header() {
    let service = ServiceBuilder::new()
        .layer(request_id_layer())
        .service(service_fn(|_request: Request<()>| async {
            Ok::<_, Infallible>(Response::new(()))
        }));

    let response = service.oneshot(Request::new(())).await.unwrap();

    assert!(
        response
            .headers()
            .contains_key(HeaderName::from_static("x-request-id"))
    );
}

#[tokio::test]
async fn timeout_layer_errors_when_service_exceeds_deadline() {
    let service = ServiceBuilder::new()
        .layer(timeout_layer(Duration::from_millis(1)))
        .service(service_fn(|_request: Request<()>| async {
            sleep(Duration::from_millis(20)).await;
            Ok::<_, Infallible>(Response::new(()))
        }));

    let error = service.oneshot(Request::new(())).await.unwrap_err();

    assert!(error.is::<tower::timeout::error::Elapsed>());
}

#[tokio::test]
async fn cors_layer_allows_preflight_requests() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(cors_layer());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/")
                .header(ORIGIN, "https://example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "*"
    );
}

#[tokio::test]
async fn compression_layer_encodes_large_accepted_responses() {
    let app = Router::new()
        .route(
            "/",
            get(|| async { "nidus compresses sufficiently large responses" }),
        )
        .layer(compression_layer());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .header(ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get(CONTENT_ENCODING).unwrap(), "gzip");
}
