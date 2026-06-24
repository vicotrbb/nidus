use axum::{
    Router,
    body::{Body, Bytes, to_bytes},
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, header::CONTENT_TYPE};
use serde::Serialize;
use std::{error::Error, fmt};
use tower::ServiceExt;

use crate::response::TestResponse;

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
    pub(crate) fn new(router: Router, method: Method, path: String) -> Self {
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
    pub fn json<T: Serialize>(self, body: &T) -> Self {
        self.try_json(body).expect("test JSON serialization failed")
    }

    /// Tries to set a JSON request body.
    pub fn try_json<T: Serialize>(
        mut self,
        body: &T,
    ) -> std::result::Result<Self, serde_json::Error> {
        self.body = Body::from(serde_json::to_vec(body)?);
        self.content_type = Some("application/json");
        Ok(self)
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

    /// Sends the request against the in-memory app, panicking if request construction fails.
    pub async fn send(self) -> TestResponse {
        self.try_send().await.expect("test request send failed")
    }

    /// Tries to send the request against the in-memory app.
    pub async fn try_send(self) -> Result<TestResponse, TestRequestError> {
        let mut builder = Request::builder().method(self.method).uri(self.path);
        if let Some(content_type) = self.content_type {
            builder = builder.header(CONTENT_TYPE, content_type);
        }
        for (name, value) in self.headers {
            if let Some(name) = name {
                builder = builder.header(name, value);
            }
        }
        let request = builder.body(self.body).map_err(TestRequestError::Request)?;
        let response = match self.router.oneshot(request).await {
            Ok(response) => response,
            Err(error) => match error {},
        };
        let status = response.status();
        let headers = response.headers().clone();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(TestRequestError::Body)?;

        Ok(TestResponse::new(status, headers, body))
    }
}

/// Error returned by fallible in-memory request execution.
#[derive(Debug)]
pub enum TestRequestError {
    /// The HTTP request could not be constructed.
    Request(http::Error),
    /// The response body could not be collected.
    Body(axum::Error),
}

impl fmt::Display for TestRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Request(error) => write!(formatter, "test request build failed: {error}"),
            Self::Body(error) => write!(formatter, "test response body read failed: {error}"),
        }
    }
}

impl Error for TestRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            Self::Body(error) => Some(error),
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
