use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use nidus_cache::MokaCacheProvider;
use tokio::runtime::Builder;

fn cache_hot_paths(c: &mut Criterion) {
    let runtime = Builder::new_current_thread()
        .build()
        .expect("cache benchmark runtime should build");

    let unnamespaced = MokaCacheProvider::builder().build();
    runtime.block_on(unnamespaced.insert("user-42", Vec::new()));
    c.bench_function("nidus moka cache get without namespace", |b| {
        b.iter(|| {
            black_box(runtime.block_on(unnamespaced.get(black_box("user-42"))));
        });
    });

    let namespaced = MokaCacheProvider::builder().namespace("users").build();
    runtime.block_on(namespaced.insert("42", Vec::new()));
    c.bench_function("nidus moka cache get with namespace", |b| {
        b.iter(|| {
            black_box(runtime.block_on(namespaced.get(black_box("42"))));
        });
    });
}

criterion_group!(benches, cache_hot_paths);
criterion_main!(benches);
