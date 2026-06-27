use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode, header::CONTENT_TYPE},
    routing::{get, post},
};
use criterion::{Criterion, criterion_group, criterion_main};
use garde::Validate;
use nidus_auth::{Guard, GuardContext, GuardError, guard_layer};
use nidus_core::{Container, Inject, SharedRequestScope};
use nidus_http::{
    controller::Controller,
    error::ErrorEnvelopeLayer,
    middleware::{
        ApiDefaults, HttpMetricsHook, PrometheusMetrics, RequestIdConfig, body_limit_layer,
        request_context_layer, request_id_layer, request_scope_layer, security_headers_layer,
        timeout_response_layer, validated_request_id_layer,
    },
    router::RouteDefinition,
};
use nidus_validation::ValidatedJson;
use serde::Deserialize;
use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tower::ServiceExt;

#[derive(Clone)]
struct AllowGuard;

#[async_trait::async_trait]
impl Guard<()> for AllowGuard {
    async fn check(&self, _ctx: GuardContext<()>) -> Result<(), GuardError> {
        Ok(())
    }
}

#[derive(Clone)]
struct UsersService;

struct UsersController {
    users: Inject<UsersService>,
}

impl UsersController {
    fn new(users: Inject<UsersService>) -> Self {
        Self { users }
    }

    fn route(&self) -> RouteDefinition {
        let _users = self.users.clone();
        RouteDefinition::get("/:id", || async { "user" })
    }
}

struct RequestId(usize);

struct RequestContext {
    request_id: Inject<RequestId>,
}

#[derive(Deserialize, Validate)]
struct CreateUserDto {
    #[garde(email)]
    email: String,
}

fn request_lifecycle_setup(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let raw_router = Router::<()>::new().route("/health", get(|| async { "ok" }));
    let hello_router = Controller::new("/")
        .route(RouteDefinition::get("/", || async { "hello" }))
        .into_router();
    let mut container = Container::new();
    container.register_singleton(UsersService).unwrap();
    let controller = UsersController::new(container.inject::<UsersService>().unwrap());
    let service_router = Controller::new("/users")
        .route(controller.route())
        .into_router();
    let guarded_router = Controller::new("/")
        .route(RouteDefinition::get("/guarded", || async { "guarded" }))
        .into_router()
        .layer(guard_layer((), "/guarded", AllowGuard));
    let validation_router = Router::new().route(
        "/users",
        post(
            |ValidatedJson(input): ValidatedJson<CreateUserDto>| async move {
                black_box(input.email);
                "created"
            },
        ),
    );
    let request_id_calls = Arc::new(AtomicUsize::new(0));
    let mut request_container = Container::new();
    request_container
        .register_request::<RequestId, _>({
            let request_id_calls = Arc::clone(&request_id_calls);
            move |_container| Ok(RequestId(request_id_calls.fetch_add(1, Ordering::Relaxed)))
        })
        .unwrap();
    request_container
        .register_request_scoped::<RequestContext, _>(|scope| {
            Ok(RequestContext {
                request_id: scope.inject::<RequestId>()?,
            })
        })
        .unwrap();
    let request_scope_router = Router::new()
        .route(
            "/scope",
            get(
                |Extension(scope): Extension<SharedRequestScope>| async move {
                    let context = scope.resolve::<RequestContext>().unwrap();
                    black_box(context.request_id.0);
                    "scoped"
                },
            ),
        )
        .layer(request_scope_layer(Arc::new(request_container)));
    let middleware_base_router = Router::new().route("/middleware", get(|| async { "ok" }));
    let security_headers_router = middleware_base_router
        .clone()
        .layer(security_headers_layer());
    let body_limit_router = middleware_base_router
        .clone()
        .layer(body_limit_layer(1024 * 1024));
    let legacy_request_id_router = middleware_base_router.clone().layer(request_id_layer());
    let request_id_router = middleware_base_router
        .clone()
        .layer(validated_request_id_layer(RequestIdConfig::production()));
    let request_context_router = middleware_base_router
        .clone()
        .layer(request_context_layer());
    let error_envelope_success_router = middleware_base_router
        .clone()
        .layer(ErrorEnvelopeLayer::new());
    let timeout_response_router = middleware_base_router
        .clone()
        .layer(timeout_response_layer(Duration::from_secs(30)));
    let production_defaults_router =
        ApiDefaults::production("bench-api").apply(Router::new().route(
            "/production",
            get(
                |context: nidus_http::middleware::RequestContext| async move {
                    black_box(context.request_id().len());
                    "production"
                },
            ),
        ));
    let production_metrics = PrometheusMetrics::new();
    let production_with_metrics_router = ApiDefaults::production("bench-api")
        .metrics(production_metrics.clone())
        .apply(Router::new().route(
            "/production",
            get(
                |context: nidus_http::middleware::RequestContext| async move {
                    black_box(context.request_id().len());
                    "production"
                },
            ),
        ));

    c.bench_function("raw axum baseline request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(raw_router.clone().oneshot(get_request("/health")))
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus hello-world request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(hello_router.clone().oneshot(get_request("/")))
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus hello-world app", |b| {
        b.iter(|| {
            Controller::new("/")
                .route(RouteDefinition::get("/", || async { "hello" }))
                .into_router()
        });
    });

    c.bench_function("nidus controller + service request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(service_router.clone().oneshot(get_request("/users/42")))
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus controller + service app", |b| {
        b.iter(|| {
            let mut container = Container::new();
            container.register_singleton(UsersService).unwrap();
            let controller = UsersController::new(container.inject::<UsersService>().unwrap());
            Controller::new("/users")
                .route(controller.route())
                .into_router()
        });
    });

    c.bench_function("nidus controller setup", |b| {
        b.iter(|| Controller::new("/health").route(RouteDefinition::get("/", || async { "ok" })));
    });

    c.bench_function("nidus guarded route", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(guarded_router.clone().oneshot(get_request("/guarded")))
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus validation route", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    validation_router
                        .clone()
                        .oneshot(json_request("/users", r#"{"email":"user@example.com"}"#)),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus request-scoped route", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(request_scope_router.clone().oneshot(get_request("/scope")))
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware security headers request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    security_headers_router
                        .clone()
                        .oneshot(get_request("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware body limit request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    body_limit_router
                        .clone()
                        .oneshot(get_request("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware legacy request id request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    legacy_request_id_router
                        .clone()
                        .oneshot(get_request("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware validated request id request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    request_id_router
                        .clone()
                        .oneshot(get_request_with_id("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware request context request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    request_context_router
                        .clone()
                        .oneshot(get_request_with_id("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware error envelope success request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    error_envelope_success_router
                        .clone()
                        .oneshot(get_request_with_id("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus middleware timeout response request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    timeout_response_router
                        .clone()
                        .oneshot(get_request("/middleware")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus api defaults production request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    production_defaults_router
                        .clone()
                        .oneshot(get_request_with_id("/production")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    c.bench_function("nidus api defaults production with metrics request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(
                    production_with_metrics_router
                        .clone()
                        .oneshot(get_request_with_id("/production")),
                )
                .unwrap();
            black_box(response.status());
        });
    });

    let metrics = PrometheusMetrics::new();
    c.bench_function("nidus metrics record response", |b| {
        b.iter(|| {
            metrics.on_request(&Method::GET, Some("/metrics-bench/{id}"));
            metrics.on_response(
                &Method::GET,
                Some("/metrics-bench/{id}"),
                StatusCode::OK,
                Duration::from_millis(12),
            );
        });
    });

    let error_metrics = PrometheusMetrics::new();
    c.bench_function("nidus metrics record inner error", |b| {
        b.iter(|| {
            error_metrics.on_request(&Method::GET, Some("/metrics-bench/{id}"));
            error_metrics.on_error(
                &Method::GET,
                Some("/metrics-bench/{id}"),
                Duration::from_millis(12),
            );
        });
    });

    let render_metrics = PrometheusMetrics::new();
    for route in [
        "/metrics-bench/0",
        "/metrics-bench/1",
        "/metrics-bench/2",
        "/metrics-bench/3",
        "/metrics-bench/4",
        "/metrics-bench/5",
        "/metrics-bench/6",
        "/metrics-bench/7",
        "/metrics-bench/8",
        "/metrics-bench/9",
    ] {
        for _ in 0..100 {
            render_metrics.on_request(&Method::GET, Some(route));
            render_metrics.on_response(
                &Method::GET,
                Some(route),
                StatusCode::OK,
                Duration::from_millis(12),
            );
        }
    }
    c.bench_function("nidus metrics render text", |b| {
        b.iter(|| black_box(render_metrics.render()));
    });
}

fn get_request(path: &'static str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("benchmark request should be valid")
}

fn get_request_with_id(path: &'static str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d78")
        .body(Body::empty())
        .expect("benchmark request should be valid")
}

fn json_request(path: &'static str, body: &'static str) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .expect("benchmark request should be valid")
}

criterion_group!(benches, request_lifecycle_setup);
criterion_main!(benches);
