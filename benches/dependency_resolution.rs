use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use nidus_core::{Container, ModuleBuilder, ModuleDefinition, ModuleGraph};

#[derive(Clone)]
struct DatabasePool;

fn module_definitions() -> Vec<ModuleDefinition> {
    const FEATURE_MODULES: usize = 128;

    let mut root = ModuleBuilder::new("AppModule");
    let mut modules = Vec::with_capacity(FEATURE_MODULES + 1);
    for index in 0..FEATURE_MODULES {
        let module_name = format!("Feature{index}Module");
        let provider_name = format!("Feature{index}Provider");
        root = root.import(module_name.clone());
        modules.push(
            ModuleBuilder::new(module_name)
                .provider(provider_name.clone())
                .export(provider_name)
                .build(),
        );
    }
    modules.push(root.build());
    modules
}

fn dependency_resolution(c: &mut Criterion) {
    let mut container = Container::new();
    container.register_singleton(DatabasePool).unwrap();

    c.bench_function("nidus singleton dependency resolution", |b| {
        b.iter(|| container.resolve::<DatabasePool>().unwrap());
    });

    c.bench_function("nidus singleton first resolution", |b| {
        b.iter_batched(
            || {
                let mut container = Container::new();
                container.register_singleton(DatabasePool).unwrap();
                container
            },
            |container| black_box(container.resolve::<DatabasePool>().unwrap()),
            BatchSize::SmallInput,
        );
    });

    c.bench_function("nidus 128-module graph validation", |b| {
        b.iter_batched(
            module_definitions,
            |modules| black_box(ModuleGraph::from_modules(modules).unwrap()),
            BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, dependency_resolution);
criterion_main!(benches);
