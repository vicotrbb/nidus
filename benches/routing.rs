use axum::{Router, routing};
use criterion::{Criterion, criterion_group, criterion_main};
use nidus_http::{controller::Controller, router::RouteDefinition};

fn routing_composition(c: &mut Criterion) {
    c.bench_function("raw axum route composition", |b| {
        b.iter(|| {
            Router::<()>::new()
                .route("/users/{id}", routing::get(|| async { "ok" }))
                .route("/users", routing::post(|| async { "created" }))
        });
    });

    c.bench_function("nidus controller route composition", |b| {
        b.iter(|| {
            Controller::new("/users")
                .route(RouteDefinition::get("/:id", || async { "ok" }))
                .route(RouteDefinition::post("/", || async { "created" }))
                .into_router()
        });
    });
}

criterion_group!(benches, routing_composition);
criterion_main!(benches);
