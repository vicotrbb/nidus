//! Test application helpers for Nidus applications.

use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, StatusCode, header::CONTENT_TYPE};
use nidus_config::Config;
use nidus_core::{
    Container, LifecycleHook, LifecycleRunner, Module, ModuleDefinition, Nidus, RequestScope,
    Result,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{str, sync::Arc};
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

    /// Creates a test application builder after validating an explicit module graph.
    pub fn bootstrap_with_modules<M, I>(modules: I) -> Result<TestAppBuilder>
    where
        M: Module,
        I: IntoIterator<Item = ModuleDefinition>,
    {
        Nidus::bootstrap_with_modules::<M, I>(modules)?;
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
        self.request(Method::GET, path)
    }

    /// Starts a POST request.
    pub fn post(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::POST, path)
    }

    /// Starts a PUT request.
    pub fn put(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::PUT, path)
    }

    /// Starts a PATCH request.
    pub fn patch(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::PATCH, path)
    }

    /// Starts a DELETE request.
    pub fn delete(&self, path: impl Into<String>) -> TestRequest {
        self.request(Method::DELETE, path)
    }

    /// Starts a request with an arbitrary HTTP method.
    pub fn request(&self, method: Method, path: impl Into<String>) -> TestRequest {
        TestRequest::new(self.router.clone(), method, path.into())
    }

    /// Resolves a provider from the test container.
    pub fn resolve<T>(&self) -> Result<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.container.resolve::<T>()
    }

    /// Creates a request scope for resolving request-lifetime providers in tests.
    pub fn request_scope(&self) -> RequestScope<'_> {
        self.container.request_scope()
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

    /// Registers a transient provider factory in the test container.
    pub fn transient_provider<T, F>(mut self, factory: F) -> Result<Self>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.container.register_transient::<T, F>(factory)?;
        Ok(self)
    }

    /// Registers a request-lifetime provider factory in the test container.
    pub fn request_provider<T, F>(mut self, factory: F) -> Result<Self>
    where
        T: Send + Sync + 'static,
        F: Fn(&Container) -> Result<T> + Send + Sync + 'static,
    {
        self.container.register_request::<T, F>(factory)?;
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
    headers: HeaderMap,
    content_type: Option<&'static str>,
}

impl TestRequest {
    fn new(router: Router, method: Method, path: String) -> Self {
        Self {
            router,
            method,
            path,
            body: Body::empty(),
            headers: HeaderMap::new(),
            content_type: None,
        }
    }

    /// Sets a request header.
    pub fn header<N, V>(mut self, name: N, value: V) -> Self
    where
        N: TryInto<HeaderName>,
        N::Error: Into<http::Error>,
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        self = self
            .try_header(name, value)
            .expect("test request header was invalid");
        self
    }

    /// Tries to set a request header.
    pub fn try_header<N, V>(mut self, name: N, value: V) -> std::result::Result<Self, http::Error>
    where
        N: TryInto<HeaderName>,
        N::Error: Into<http::Error>,
        V: TryInto<HeaderValue>,
        V::Error: Into<http::Error>,
    {
        let name = name.try_into().map_err(Into::into)?;
        let value = value.try_into().map_err(Into::into)?;
        self.headers.insert(name, value);
        Ok(self)
    }

    /// Sets a UTF-8 text request body.
    pub fn text(mut self, body: impl Into<String>) -> Self {
        self.body = Body::from(body.into());
        self.content_type = Some("text/plain; charset=utf-8");
        self
    }

    /// Sets a raw request body.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Body::from(body.into());
        self
    }

    /// Sets a JSON request body.
    pub fn json<T: Serialize>(mut self, body: &T) -> Self {
        self.body = Body::from(serde_json::to_vec(body).expect("test JSON serialization failed"));
        self.content_type = Some("application/json");
        self
    }

    /// Appends URL-encoded query parameters.
    pub fn query<T: Serialize>(mut self, query: &T) -> Self {
        self = self
            .try_query(query)
            .expect("test query serialization failed");
        self
    }

    /// Tries to append URL-encoded query parameters.
    pub fn try_query<T: Serialize>(
        mut self,
        query: &T,
    ) -> std::result::Result<Self, serde_urlencoded::ser::Error> {
        let query = serde_urlencoded::to_string(query)?;
        if !query.is_empty() {
            self.path = append_query(&self.path, &query);
        }
        Ok(self)
    }

    /// Sends the request against the in-memory app.
    pub async fn send(self) -> TestResponse {
        let mut builder = Request::builder().method(self.method).uri(self.path);
        if let Some(content_type) = self.content_type {
            builder = builder.header(CONTENT_TYPE, content_type);
        }
        for (name, value) in self.headers {
            if let Some(name) = name {
                builder = builder.header(name, value);
            }
        }
        let request = builder.body(self.body).expect("test request build failed");
        let response = self
            .router
            .oneshot(request)
            .await
            .expect("test router response failed");
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("test response body read failed");

        TestResponse {
            status,
            headers,
            body,
        }
    }
}

fn append_query(path: &str, query: &str) -> String {
    let separator = if path.contains('?') && !path.ends_with('?') && !path.ends_with('&') {
        "&"
    } else if path.contains('?') {
        ""
    } else {
        "?"
    };
    format!("{path}{separator}{query}")
}

/// Captured in-memory HTTP response.
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
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

    /// Returns the response headers.
    pub fn headers(&self) -> &HeaderMap {
        &self.headers
    }

    /// Returns a response header by name.
    pub fn header(&self, name: impl AsRef<str>) -> Option<&HeaderValue> {
        self.headers.get(name.as_ref())
    }

    /// Asserts the response status code.
    pub fn assert_status(&self, expected: StatusCode) {
        assert_eq!(self.status, expected);
    }

    /// Asserts a response header as a UTF-8 string.
    pub fn assert_header(&self, name: impl AsRef<str>, expected: &str) {
        let name = name.as_ref();
        let actual = self
            .header(name)
            .unwrap_or_else(|| panic!("missing response header `{name}`"));
        assert_eq!(
            actual
                .to_str()
                .unwrap_or_else(|_| panic!("response header `{name}` was not valid UTF-8")),
            expected
        );
    }

    /// Decodes the response body as JSON.
    pub fn json<T>(&self) -> T
    where
        T: DeserializeOwned,
    {
        self.try_json().expect("test response was not valid JSON")
    }

    /// Tries to decode the response body as JSON.
    pub fn try_json<T>(&self) -> serde_json::Result<T>
    where
        T: DeserializeOwned,
    {
        serde_json::from_slice(&self.body)
    }

    /// Returns the response body as UTF-8 text.
    pub fn text(&self) -> std::result::Result<&str, str::Utf8Error> {
        str::from_utf8(&self.body)
    }

    /// Asserts the response body as UTF-8 text.
    pub async fn assert_text(self, expected: &str) {
        let text = self.text().expect("test response was not UTF-8");
        assert_eq!(text, expected);
    }

    /// Asserts the response body as JSON.
    pub async fn assert_json(self, expected: Value) {
        let actual: Value = self.json();
        assert_eq!(actual, expected);
    }
}
