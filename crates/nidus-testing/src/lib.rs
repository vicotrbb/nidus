//! Test application helpers for Nidus applications.

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
};
use http::{Method, Request, StatusCode};
use serde::Serialize;
use serde_json::Value;
use tower::ServiceExt;

/// In-memory test application backed by an Axum router.
#[derive(Clone)]
pub struct TestApp {
    router: Router,
}

impl TestApp {
    /// Creates a test application from an Axum router.
    pub fn from_router(router: Router) -> Self {
        Self { router }
    }

    /// Starts a GET request.
    pub fn get(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::GET, path.into())
    }

    /// Starts a POST request.
    pub fn post(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::POST, path.into())
    }
}

/// In-memory HTTP request builder.
pub struct TestRequest {
    router: Router,
    method: Method,
    path: String,
    body: Body,
    content_type: Option<&'static str>,
}

impl TestRequest {
    fn new(router: Router, method: Method, path: String) -> Self {
        Self {
            router,
            method,
            path,
            body: Body::empty(),
            content_type: None,
        }
    }

    /// Sets a JSON request body.
    pub fn json<T: Serialize>(mut self, body: &T) -> Self {
        self.body = Body::from(serde_json::to_vec(body).expect("test JSON serialization failed"));
        self.content_type = Some("application/json");
        self
    }

    /// Sends the request against the in-memory app.
    pub async fn send(self) -> TestResponse {
        let mut builder = Request::builder().method(self.method).uri(self.path);
        if let Some(content_type) = self.content_type {
            builder = builder.header(http::header::CONTENT_TYPE, content_type);
        }
        let request = builder.body(self.body).expect("test request build failed");
        let response = self
            .router
            .oneshot(request)
            .await
            .expect("test router response failed");
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("test response body read failed");

        TestResponse { status, body }
    }
}

/// Captured in-memory HTTP response.
pub struct TestResponse {
    status: StatusCode,
    body: Bytes,
}

impl TestResponse {
    /// Asserts the response status code.
    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(self.status, expected);
    }

    /// Asserts the response body as UTF-8 text.
    pub async fn assert_text(self, expected: &str) {
        let text = String::from_utf8(self.body.to_vec()).expect("test response was not UTF-8");
        assert_eq!(text, expected);
    }

    /// Asserts the response body as JSON.
    pub async fn assert_json(self, expected: Value) {
        let actual: Value =
            serde_json::from_slice(&self.body).expect("test response was not valid JSON");
        assert_eq!(actual, expected);
    }
}
