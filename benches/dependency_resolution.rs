use criterion::{Criterion, criterion_group, criterion_main};
use nidus_core::Container;

#[derive(Clone)]
struct DatabasePool;

fn dependency_resolution(c: &mut Criterion) {
    let mut container = Container::new();
    container.register_singleton(DatabasePool).unwrap();

    c.bench_function("nidus singleton dependency resolution", |b| {
        b.iter(|| container.resolve::<DatabasePool>().unwrap());
    });
}

criterion_group!(benches, dependency_resolution);
criterion_main!(benches);
