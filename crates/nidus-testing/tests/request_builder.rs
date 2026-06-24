use axum::{
    Router,
    body::Bytes,
    extract::Query,
    routing::{get, post},
};
use http::HeaderMap;
use nidus_testing::{TestApp, TestRequestError};
use serde::{Serialize, ser};

#[derive(Debug, serde::Deserialize, Serialize)]
struct SearchQuery {
    name: String,
    page: u32,
}

struct BrokenJsonBody;

impl Serialize for BrokenJsonBody {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        Err(ser::Error::custom("broken json body"))
    }
}

#[tokio::test]
async fn test_request_sends_custom_headers() {
    let router = Router::new().route(
        "/echo-header",
        post(|headers: HeaderMap| async move {
            headers
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing")
                .to_owned()
        }),
    );

    let response = TestApp::from_router(router)
        .post("/echo-header")
        .header("x-api-key", "secret")
        .send()
        .await;

    response.assert_text("secret").await;
}

#[test]
fn test_request_try_header_reports_invalid_headers() {
    let router = Router::new();

    let invalid_name = TestApp::from_router(router.clone())
        .get("/health")
        .try_header("bad header", "secret");
    assert!(invalid_name.is_err());

    let invalid_value = TestApp::from_router(router)
        .get("/health")
        .try_header("x-api-key", "bad\nvalue");
    assert!(invalid_value.is_err());
}

#[test]
fn test_request_try_json_reports_serialization_errors() {
    let router = Router::new();

    let error = match TestApp::from_router(router)
        .post("/users")
        .try_json(&BrokenJsonBody)
    {
        Ok(_) => panic!("broken JSON body should fail to serialize"),
        Err(error) => error,
    };

    assert_eq!(error.to_string(), "broken json body");
}

#[tokio::test]
async fn test_request_try_send_reports_request_build_errors() {
    let router = Router::new();

    let error = match TestApp::from_router(router).get("bad uri").try_send().await {
        Ok(_) => panic!("invalid request URI should fail"),
        Err(error) => error,
    };

    assert!(matches!(error, TestRequestError::Request(_)));
}

#[tokio::test]
async fn test_request_sends_text_body_with_content_type() {
    let router = Router::new().route(
        "/echo-text",
        post(|headers: HeaderMap, body: String| async move {
            let content_type = headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .unwrap_or("missing");
            format!("{content_type}:{body}")
        }),
    );

    let response = TestApp::from_router(router)
        .post("/echo-text")
        .text("hello")
        .send()
        .await;

    response
        .assert_text("text/plain; charset=utf-8:hello")
        .await;
}

#[tokio::test]
async fn test_request_sends_raw_body() {
    let router = Router::new().route(
        "/bytes",
        post(|body: Bytes| async move { body.len().to_string() }),
    );

    let response = TestApp::from_router(router)
        .post("/bytes")
        .body(Bytes::from_static(b"nidus"))
        .send()
        .await;

    response.assert_text("5").await;
}

#[tokio::test]
async fn test_request_appends_typed_query_params() {
    let router =
        Router::new().route(
            "/users",
            get(|Query(query): Query<SearchQuery>| async move {
                format!("{}:{}", query.name, query.page)
            }),
        );

    let response = TestApp::from_router(router)
        .get("/users")
        .query(&SearchQuery {
            name: "Ada Lovelace".to_owned(),
            page: 2,
        })
        .send()
        .await;

    response.assert_text("Ada Lovelace:2").await;
}
