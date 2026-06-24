use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, header::CONTENT_TYPE},
    routing::{get, post},
};
use criterion::{Criterion, criterion_group, criterion_main};
use nidus_auth::{Guard, GuardContext, GuardError, guard_layer};
use nidus_core::{Container, Inject, SharedRequestScope};
use nidus_http::{
    controller::Controller, middleware::request_scope_layer, router::RouteDefinition,
};
use nidus_validation::ValidatedJson;
use serde::Deserialize;
use std::{
    hint::black_box,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tower::ServiceExt;
use validator::Validate;

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
    #[validate(email)]
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

    c.bench_function("nidus controller + service request", |b| {
        b.iter(|| {
            let response = runtime
                .block_on(service_router.clone().oneshot(get_request("/users/42")))
                .unwrap();
            black_box(response.status());
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
}

fn get_request(path: &'static str) -> Request<Body> {
    Request::builder()
        .uri(path)
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
