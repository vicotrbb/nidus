use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use axum::extract::Request;
use tower::{Layer, Service};

use crate::context::{RequestContext, header_to_string};

/// Creates a Tower layer that enriches [`RequestContext`] request extensions.
pub fn request_context_layer() -> RequestContextLayer {
    RequestContextLayer
}

/// Tower layer that inserts request/correlation context into request extensions.
#[derive(Clone, Copy, Debug, Default)]
pub struct RequestContextLayer;

impl<S> Layer<S> for RequestContextLayer {
    type Service = RequestContextService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        RequestContextService { inner }
    }
}

/// Service produced by [`RequestContextLayer`].
#[derive(Clone, Debug)]
pub struct RequestContextService<S> {
    inner: S,
}

impl<S> Service<Request> for RequestContextService<S>
where
    S: Service<Request> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut request: Request) -> Self::Future {
        let (mut parts, body) = request.into_parts();
        let request_id = parts
            .extensions
            .get::<RequestContext>()
            .map(|context| context.request_id().to_owned())
            .or_else(|| header_to_string(&parts.headers, "x-request-id"))
            .unwrap_or_else(|| "unknown".to_owned());
        let context = RequestContext::from_parts(&parts, request_id);
        parts.extensions.insert(context);
        request = Request::from_parts(parts, body);
        let future = self.inner.call(request);
        Box::pin(future)
    }
}
