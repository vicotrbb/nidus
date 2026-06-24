use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{SystemTime, UNIX_EPOCH},
};

use http::{HeaderValue, Request, Response, header::HeaderName};
use tower::{Layer, Service};

/// Tower layer that adds an `x-request-id` response header when absent.
///
/// Incoming request IDs are propagated to the response unless the inner service
/// already set a response ID. Requests without an ID receive a generated one.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestIdLayer;

impl<S> Layer<S> for RequestIdLayer {
    type Service = RequestIdService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestIdService { inner }
    }
}

/// Service produced by [`RequestIdLayer`].
#[derive(Clone, Debug)]
pub struct RequestIdService<S> {
    inner: S,
}

impl<S, RequestBody, ResponseBody> Service<Request<RequestBody>> for RequestIdService<S>
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    RequestBody: Send + 'static,
    ResponseBody: Send + 'static,
{
    type Response = Response<ResponseBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<RequestBody>) -> Self::Future {
        let request_id = request.headers().get(request_id_header()).cloned();
        let future = self.inner.call(request);
        Box::pin(async move {
            let mut response = future.await?;
            response
                .headers_mut()
                .entry(request_id_header())
                .or_insert_with(|| request_id.unwrap_or_else(new_request_id));
            Ok(response)
        })
    }
}

fn request_id_header() -> HeaderName {
    HeaderName::from_static("x-request-id")
}

fn new_request_id() -> HeaderValue {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    HeaderValue::from_str(&format!("nidus-{nanos}"))
        .expect("generated request id contains only valid header characters")
}
