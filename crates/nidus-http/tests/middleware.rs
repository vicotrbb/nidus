use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::{Extension, Router, body::Body, body::to_bytes, routing::get};
use http::{
    HeaderValue, Method, Request, Response, StatusCode,
    header::{
        ACCEPT_ENCODING, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_REQUEST_METHOD,
        CONTENT_ENCODING, HeaderName, ORIGIN,
    },
};
use nidus_core::{Container, Inject, SharedRequestScope};
use nidus_http::{
    context::RequestIdentity,
    middleware::{
        InMemoryRateLimitStore, RateLimitStore, catch_panic_layer, compression_layer, cors_layer,
        cors_origin_layer, rate_limit_layer, request_id_layer, request_scope_layer, timeout_layer,
        trace_layer,
    },
};
use tokio::time::sleep;
use tower::{Service, ServiceBuilder, ServiceExt, service_fn};
use uuid::{Uuid, Version};

#[derive(Debug, PartialEq, Eq)]
struct RequestId(usize);

#[derive(Debug)]
struct RequestContext {
    request_id: Inject<RequestId>,
}

#[tokio::test]
async fn catch_panic_layer_handles_synchronous_service_panics() {
    let service = ServiceBuilder::new()
        .layer(catch_panic_layer())
        .service(service_fn(
            |_request: Request<Body>| -> std::future::Ready<Result<Response<Body>, Infallible>> {
                panic!("service call panicked synchronously")
            },
        ));

    let response = service.oneshot(Request::new(Body::empty())).await.unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

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
async fn request_id_layer_generates_uuid_v4_response_id() {
    let service = ServiceBuilder::new()
        .layer(request_id_layer())
        .service(service_fn(|_request: Request<()>| async {
            Ok::<_, Infallible>(Response::new(()))
        }));

    let response = service.oneshot(Request::new(())).await.unwrap();
    let request_id = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    let parsed = Uuid::parse_str(request_id).unwrap();

    assert_eq!(parsed.get_version(), Some(Version::Random));
}

#[tokio::test]
async fn request_id_layer_propagates_incoming_request_id() {
    let service = ServiceBuilder::new()
        .layer(request_id_layer())
        .service(service_fn(|_request: Request<()>| async {
            Ok::<_, Infallible>(Response::new(()))
        }));

    let response = service
        .oneshot(
            Request::builder()
                .header("x-request-id", "req-123")
                .body(())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("x-request-id"),
        Some(&HeaderValue::from_static("req-123"))
    );
}

#[tokio::test]
async fn request_id_layer_preserves_existing_response_id() {
    let service = ServiceBuilder::new()
        .layer(request_id_layer())
        .service(service_fn(|_request: Request<()>| async {
            Ok::<_, Infallible>(
                Response::builder()
                    .header("x-request-id", "handler-456")
                    .body(())
                    .unwrap(),
            )
        }));

    let response = service
        .oneshot(
            Request::builder()
                .header("x-request-id", "req-123")
                .body(())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("x-request-id"),
        Some(&HeaderValue::from_static("handler-456"))
    );
}

#[tokio::test]
async fn request_scope_layer_inserts_one_scope_per_http_request() {
    async fn handler(Extension(scope): Extension<SharedRequestScope>) -> String {
        let context = scope.resolve::<RequestContext>().unwrap();
        let request_id = scope.resolve::<RequestId>().unwrap();

        assert!(Arc::ptr_eq(
            &context.request_id.clone().into_inner(),
            &request_id
        ));

        request_id.0.to_string()
    }

    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_request::<RequestId, _>({
            let calls = Arc::clone(&calls);
            move |_container| {
                Ok(RequestId(
                    calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
                ))
            }
        })
        .unwrap();
    container
        .register_request_scoped::<RequestContext, _>(|scope| {
            Ok(RequestContext {
                request_id: scope.inject::<RequestId>()?,
            })
        })
        .unwrap();

    let app = Router::new()
        .route("/scope", get(handler))
        .layer(request_scope_layer(Arc::new(container)));

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/scope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let second = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/scope")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(first.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"0"
    );
    assert_eq!(
        to_bytes(second.into_body(), usize::MAX)
            .await
            .unwrap()
            .as_ref(),
        b"1"
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
async fn rate_limit_layer_backpressures_until_period_resets() {
    let mut service = ServiceBuilder::new()
        .layer(rate_limit_layer(1, Duration::from_millis(50)))
        .service(service_fn(|_request: Request<()>| async {
            Ok::<_, Infallible>(Response::new(()))
        }));

    service
        .ready()
        .await
        .unwrap()
        .call(Request::new(()))
        .await
        .unwrap();

    let limited = tokio::time::timeout(Duration::from_millis(5), service.ready()).await;
    assert!(limited.is_err());

    sleep(Duration::from_millis(60)).await;
    let response = service
        .ready()
        .await
        .unwrap()
        .call(Request::new(()))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn in_memory_rate_limit_store_starts_empty() {
    let store = InMemoryRateLimitStore::new();

    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
}

#[test]
fn in_memory_rate_limit_store_prunes_stale_identity_windows() {
    let store = InMemoryRateLimitStore::new();
    let window = Duration::from_millis(10);

    store
        .check(&RequestIdentity::new("client-a"), 10, window)
        .unwrap();
    store
        .check(&RequestIdentity::new("client-b"), 10, window)
        .unwrap();
    assert_eq!(store.len(), 2);

    std::thread::sleep(Duration::from_millis(25));
    store
        .check(&RequestIdentity::new("client-c"), 10, window)
        .unwrap();

    assert_eq!(store.len(), 1);
}

#[test]
fn in_memory_rate_limit_store_preserves_active_identity_windows() {
    let store = InMemoryRateLimitStore::new();
    let window = Duration::from_secs(60);

    store
        .check(&RequestIdentity::new("client-a"), 10, window)
        .unwrap();
    store
        .check(&RequestIdentity::new("client-b"), 10, window)
        .unwrap();

    assert_eq!(store.len(), 2);
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
async fn cors_origin_layer_allows_one_explicit_origin() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(cors_origin_layer(HeaderValue::from_static(
            "https://api.example.com",
        )));

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/")
                .header(ORIGIN, "https://api.example.com")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(),
        "https://api.example.com"
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

#[tokio::test]
async fn trace_layer_preserves_http_responses() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(trace_layer());

    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}
