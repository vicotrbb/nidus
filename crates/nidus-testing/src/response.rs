use axum::body::Bytes;
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::str;

/// Captured in-memory HTTP response.
pub struct TestResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl TestResponse {
    pub(crate) fn new(status: StatusCode, headers: HeaderMap, body: Bytes) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

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
