use axum::{Router, routing::get};
use criterion::{Criterion, criterion_group, criterion_main};
use nidus_http::{controller::Controller, router::RouteDefinition};

fn request_lifecycle_setup(c: &mut Criterion) {
    c.bench_function("raw axum route setup", |b| {
        b.iter(|| Router::<()>::new().route("/health", get(|| async { "ok" })));
    });

    c.bench_function("nidus controller setup", |b| {
        b.iter(|| Controller::new("/health").route(RouteDefinition::get("/", || async { "ok" })));
    });
}

criterion_group!(benches, request_lifecycle_setup);
criterion_main!(benches);
