use axum::{Router, routing::get};
use criterion::{Criterion, criterion_group, criterion_main};
use nidus_auth::{Guard, GuardContext, GuardError};
use nidus_core::{Container, Inject};
use nidus_http::{controller::Controller, router::RouteDefinition};
use nidus_validation::ValidationPipe;
use std::hint::black_box;
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
    c.bench_function("raw axum baseline", |b| {
        b.iter(|| Router::<()>::new().route("/health", get(|| async { "ok" })));
    });

    c.bench_function("nidus hello-world app", |b| {
        b.iter(|| Controller::new("/").route(RouteDefinition::get("/", || async { "hello" })));
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

    let runtime = tokio::runtime::Runtime::new().unwrap();
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

criterion_group!(benches, request_lifecycle_setup);
criterion_main!(benches);
