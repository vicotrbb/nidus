use nidus::prelude::*;

#[test]
fn prelude_exports_optional_feature_crates() {
    let _config = Config::new();
    let _error = HttpError::bad_request("invalid request");
    let _document = OpenApiDocument::new("Nidus API", "0.1.0");
    let _validation = ValidationPipe::new();
    let _context = GuardContext::new((), "/health");
    let _and_guard: Option<AndGuard<(), ()>> = None;
    let _or_guard: Option<OrGuard<(), ()>> = None;
    let _pool: Option<PgPool> = None;
    let _pool_options = PgPoolOptions::new();
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
    let _document = nidus::openapi::OpenApiDocument::new("Nidus API", "0.1.0");
    let _validation = nidus::validation::ValidationPipe::new();
    let _context = nidus::auth::GuardContext::new((), "/health");
    let _pool_options = nidus::sqlx::postgres::PgPoolOptions::new();
    let jobs = nidus::jobs::JobQueue::new();
    let _async_jobs = nidus::jobs::AsyncJobQueue::new();
    let events = nidus::events::EventBus::<String>::new();
    let _app = nidus::testing::TestApp::from_router(axum::Router::new());

    jobs.run_all();
    events.subscribe();
}
