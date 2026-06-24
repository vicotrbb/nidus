use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use axum::{Router, body::Body, body::to_bytes, routing::get};
use http::{Method, Request, StatusCode};
use nidus_core::{Container, Inject};
use nidus_http::{RequestScoped, middleware::request_scope_layer};
use serde_json::Value;
use tower::ServiceExt;

#[derive(Debug, PartialEq, Eq)]
struct RequestId(usize);

#[derive(Debug)]
struct RequestContext {
    request_id: Inject<RequestId>,
}

#[tokio::test]
async fn request_scoped_extractor_resolves_provider_from_request_scope() {
    async fn handler(context: RequestScoped<RequestContext>) -> String {
        context.request_id.0.to_string()
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let mut container = Container::new();
    container
        .register_request::<RequestId, _>({
            let calls = Arc::clone(&calls);
            move |_container| Ok(RequestId(calls.fetch_add(1, Ordering::SeqCst)))
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

    let first = app.clone().oneshot(get_request("/scope")).await.unwrap();
    let second = app.oneshot(get_request("/scope")).await.unwrap();

    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(body_json_string(first).await, "0");
    assert_eq!(body_json_string(second).await, "1");
}

#[tokio::test]
async fn request_scoped_extractor_rejects_when_scope_layer_is_missing() {
    async fn handler(_request_id: RequestScoped<RequestId>) -> &'static str {
        "unreachable"
    }

    let app = Router::new().route("/scope", get(handler));

    let response = app.oneshot(get_request("/scope")).await.unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "request_scope_unavailable");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("request_scope_layer")
    );
}

#[tokio::test]
async fn request_scoped_extractor_rejects_when_provider_is_missing() {
    async fn handler(_request_id: RequestScoped<RequestId>) -> &'static str {
        "unreachable"
    }

    let app = Router::new()
        .route("/scope", get(handler))
        .layer(request_scope_layer(Arc::new(Container::new())));

    let response = app.oneshot(get_request("/scope")).await.unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = response_json(response).await;
    assert_eq!(body["error"]["code"], "request_scope_resolution_failed");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap()
            .contains("RequestId")
    );
}

fn get_request(path: &'static str) -> Request<Body> {
    Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .unwrap()
}

async fn body_json_string(response: axum::response::Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}
