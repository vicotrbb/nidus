use axum::{Router, body::Body, http::Request, routing::get};
use criterion::{Criterion, criterion_group, criterion_main};
use nidus_auth::{Guard, GuardContext, GuardError};
use nidus_core::{Container, Inject};
use nidus_http::{controller::Controller, router::RouteDefinition};
use nidus_validation::ValidationPipe;
use std::hint::black_box;
use tower::ServiceExt;
use validator::Validate;

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

#[derive(Validate)]
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
            runtime
                .block_on(AllowGuard.check(GuardContext::new((), "/users/{id}")))
                .unwrap();
        });
    });

    c.bench_function("nidus validation route", |b| {
        b.iter(|| {
            let input = CreateUserDto {
                email: "user@example.com".to_owned(),
            };
            black_box(ValidationPipe::new().transform(input).unwrap());
        });
    });
}

fn get_request(path: &'static str) -> Request<Body> {
    Request::builder()
        .uri(path)
        .body(Body::empty())
        .expect("benchmark request should be valid")
}

criterion_group!(benches, request_lifecycle_setup);
criterion_main!(benches);
