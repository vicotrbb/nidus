//! Test application helpers for Nidus applications.

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
};
use http::{Method, Request, StatusCode};
use nidus_config::Config;
use nidus_core::{Container, LifecycleHook, LifecycleRunner, Module, Nidus, Result};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

/// In-memory test application backed by an Axum router.
#[derive(Clone)]
pub struct TestApp {
    router: Router,
    container: Arc<Container>,
    config: Config,
    lifecycle: Arc<LifecycleRunner>,
}

impl TestApp {
    /// Creates a test application builder after validating a root Nidus module.
    pub fn bootstrap<M>() -> Result<TestAppBuilder>
    where
        M: Module,
    {
        Nidus::bootstrap::<M>()?;
        Ok(Self::builder(Router::new()))
    }

    /// Creates a test application from an Axum router.
    pub fn from_router(router: Router) -> Self {
        Self {
            router,
            container: Arc::new(Container::new()),
            config: Config::new(),
            lifecycle: Arc::new(LifecycleRunner::new()),
        }
    }

    /// Creates a configurable test application builder.
    pub fn builder(router: Router) -> TestAppBuilder {
        TestAppBuilder {
            router,
            container: Container::new(),
            config: Config::new(),
            lifecycle: LifecycleRunner::new(),
        }
    }

    /// Starts a GET request.
    pub fn get(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::GET, path.into())
    }

    /// Starts a POST request.
    pub fn post(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::POST, path.into())
    }

    /// Starts a PUT request.
    pub fn put(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::PUT, path.into())
    }

    /// Starts a PATCH request.
    pub fn patch(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::PATCH, path.into())
    }

    /// Starts a DELETE request.
    pub fn delete(&self, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), Method::DELETE, path.into())
    }

    /// Resolves a provider from the test container.
    pub fn resolve<T>(&self) -> Result<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.container.resolve::<T>()
    }

    /// Returns test configuration overrides.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Runs registered test shutdown lifecycle hooks.
    pub async fn shutdown(&self) -> Result<()> {
        self.lifecycle.shutdown().await
    }
}

/// Builder for in-memory test applications.
pub struct TestAppBuilder {
    router: Router,
    container: Container,
    config: Config,
    lifecycle: LifecycleRunner,
}

impl TestAppBuilder {
    /// Registers a provider in the test container.
    pub fn provider<T>(mut self, value: T) -> Result<Self>
    where
        T: Send + Sync + 'static,
    {
        self.container.register_singleton(value)?;
        Ok(self)
    }

    /// Overrides a provider in the test container.
    pub fn override_provider<T>(mut self, value: T) -> Result<Self>
    where
        T: Send + Sync + 'static,
    {
        self.container.override_singleton(value)?;
        Ok(self)
    }

    /// Sets configuration overrides for the test application.
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Registers a lifecycle hook for the test application.
    pub fn lifecycle_hook<H>(mut self, hook: H) -> Self
    where
        H: LifecycleHook,
    {
        self.lifecycle = self.lifecycle.hook(hook);
        self
    }

    /// Builds the test application.
    pub fn build(self) -> TestApp {
        TestApp {
            router: self.router,
            container: Arc::new(self.container),
            config: self.config,
            lifecycle: Arc::new(self.lifecycle),
        }
    }

    /// Runs startup hooks and builds the test application.
    pub async fn build_started(self) -> Result<TestApp> {
        self.lifecycle.startup().await?;
        Ok(self.build())
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
    /// Returns the response status code.
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns the raw response body bytes.
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Asserts the response status code.
    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(self.status, expected);
    }

    /// Decodes the response body as JSON.
    pub fn json<T>(&self) -> T
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body).expect("test response was not valid JSON")
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
