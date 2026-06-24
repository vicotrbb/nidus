use std::{
    borrow::Cow,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use http::{Method, Request, Response, StatusCode};
use tower::{Layer, Service};

/// Creates a metrics hook layer without a stable route label.
pub fn metrics_layer<H>(hook: H) -> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    MetricsLayer::new(hook)
}

/// Creates a metrics hook layer that records a stable route label.
pub fn route_metrics_layer<H>(route: impl Into<Cow<'static, str>>, hook: H) -> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    MetricsLayer::new(hook).route(route)
}

/// Backend-neutral hook for recording HTTP request metrics.
pub trait HttpMetricsHook: Clone + Send + Sync + 'static {
    /// Records that a request entered the service.
    fn on_request(&self, method: &Method, route: Option<&str>);

    /// Records that a response left the service.
    fn on_response(
        &self,
        method: &Method,
        route: Option<&str>,
        status: StatusCode,
        latency: Duration,
    );

    /// Records that the inner service returned an error before producing a response.
    fn on_error(&self, _method: &Method, _route: Option<&str>, _latency: Duration) {}
}

/// Tower layer that invokes [`HttpMetricsHook`] for request lifecycle metrics.
#[derive(Clone, Debug)]
pub struct MetricsLayer<H> {
    hook: H,
    route: Option<Cow<'static, str>>,
}

impl<H> MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    /// Creates a metrics layer without a route label.
    pub fn new(hook: H) -> Self {
        Self { hook, route: None }
    }

    /// Adds a stable route label to emitted metrics.
    pub fn route(mut self, route: impl Into<Cow<'static, str>>) -> Self {
        self.route = Some(route.into());
        self
    }
}

impl<S, H> Layer<S> for MetricsLayer<H>
where
    H: HttpMetricsHook,
{
    type Service = MetricsService<S, H>;

    fn layer(&self, inner: S) -> Self::Service {
        MetricsService {
            inner,
            hook: self.hook.clone(),
            route: self.route.clone(),
        }
    }
}

/// Service produced by [`MetricsLayer`].
#[derive(Clone, Debug)]
pub struct MetricsService<S, H> {
    inner: S,
    hook: H,
    route: Option<Cow<'static, str>>,
}

impl<S, H, RequestBody, ResponseBody> Service<Request<RequestBody>> for MetricsService<S, H>
where
    S: Service<Request<RequestBody>, Response = Response<ResponseBody>> + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    H: HttpMetricsHook,
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
        let method = request.method().clone();
        let hook = self.hook.clone();
        let route = self.route.clone();
        hook.on_request(&method, route.as_deref());
        let started_at = Instant::now();
        let future = self.inner.call(request);

        Box::pin(async move {
            match future.await {
                Ok(response) => {
                    hook.on_response(
                        &method,
                        route.as_deref(),
                        response.status(),
                        started_at.elapsed(),
                    );
                    Ok(response)
                }
                Err(error) => {
                    hook.on_error(&method, route.as_deref(), started_at.elapsed());
                    Err(error)
                }
            }
        })
    }
}
