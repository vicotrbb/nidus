use nidus::prelude::*;

#[test]
fn facade_does_not_reexport_sqlx_adapter_dependencies() {
    let manifest_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = std::fs::read_to_string(manifest_path).unwrap();

    assert!(!manifest.contains("sqlx ="), "{manifest}");
    assert!(!manifest.contains("nidus-sqlx"), "{manifest}");
    assert!(!manifest.contains("nidus-cache"), "{manifest}");
}

#[derive(Clone)]
struct NoopMetrics;

impl HttpMetricsHook for NoopMetrics {
    fn on_request(&self, _method: &axum::http::Method, _route: Option<&str>) {}

    fn on_response(
        &self,
        _method: &axum::http::Method,
        _route: Option<&str>,
        _status: StatusCode,
        _latency: std::time::Duration,
    ) {
    }
}

#[test]
fn prelude_exports_optional_feature_crates() {
    let _config = Config::new();
    let _error = HttpError::bad_request("invalid request");
    let _document = OpenApiDocument::new("Nidus API", "1.0.0");
    let _document_error: Option<OpenApiDocumentError> = None;
    let _validation = ValidationPipe::new();
    let _validated_json = ValidatedJson("ok");
    let _validated_json_rejection: Option<ValidatedJsonRejection> = None;
    let _context = GuardContext::new((), "/health");
    let _and_guard: Option<AndGuard<(), ()>> = None;
    let _or_guard: Option<OrGuard<(), ()>> = None;
    let jobs = JobQueue::new();
    let _async_jobs = AsyncJobQueue::new();
    let events = EventBus::<String>::new();
    let _app = TestApp::from_router(axum::Router::new());
    let _path: Path<u64> = Path(42);
    let _query: Query<Vec<(String, String)>> = Query(Vec::new());
    let _state: State<&'static str> = State("ready");
    let _headers = HeaderMap::new();
    let _body = Json("ok");
    let response: Response = StatusCode::OK.into_response();
    let container = std::sync::Arc::new(Container::new());
    let _scope = RequestScope::from_shared_container(std::sync::Arc::clone(&container));
    let _shared_scope: SharedRequestScope = std::sync::Arc::new(
        RequestScope::from_shared_container(std::sync::Arc::clone(&container)),
    );
    let _scope_layer: RequestScopeLayer = request_scope_layer(container);
    let _scope_service: Option<RequestScopeService<()>> = None;
    let _request_scoped: Option<RequestScoped<String>> = None;
    let _request_scope_rejection: Option<RequestScopeRejection> = None;
    let _request_id_layer: RequestIdLayer = request_id_layer();
    let _request_id_service: Option<RequestIdService<()>> = None;
    let _timeout_layer = timeout_layer(std::time::Duration::from_secs(1));
    let _rate_limit_layer = rate_limit_layer(10, std::time::Duration::from_secs(60));
    let _cors_layer = cors_layer();
    let _compression_layer = compression_layer();
    let _trace_layer = trace_layer();
    let _route_trace_layer = route_trace_layer("/health");
    let _route_make_span = RouteMakeSpan::new("/health");
    let _metrics_layer: MetricsLayer<NoopMetrics> = metrics_layer(NoopMetrics);
    let _route_metrics_layer: MetricsLayer<NoopMetrics> =
        route_metrics_layer("/health", NoopMetrics);
    let _metrics_service: Option<MetricsService<(), NoopMetrics>> = None;

    jobs.run_all();
    events.subscribe();
    assert_eq!(response.status(), StatusCode::OK);
}

#[allow(dead_code)]
fn prelude_exports_guard_extension_trait<G: GuardExt<()>>() {}

#[test]
fn facade_exports_optional_feature_modules() {
    let _config = nidus::config::Config::new();
    let _error = nidus::http::error::HttpError::bad_request("invalid request");
    let _document = nidus::openapi::OpenApiDocument::new("Nidus API", "1.0.0");
    let _validation = nidus::validation::ValidationPipe::new();
    let _validated_json = nidus::validation::ValidatedJson("ok");
    let _context = nidus::auth::GuardContext::new((), "/health");
    let jobs = nidus::jobs::JobQueue::new();
    let _async_jobs = nidus::jobs::AsyncJobQueue::new();
    let events = nidus::events::EventBus::<String>::new();
    let _app = nidus::testing::TestApp::from_router(axum::Router::new());

    jobs.run_all();
    events.subscribe();
}
