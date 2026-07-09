use criterion::{Criterion, criterion_group, criterion_main};
use nidus_events::EventBus;
use std::hint::black_box;

fn event_bus_setup(c: &mut Criterion) {
    let bus = EventBus::new();
    let subscriber = bus.subscribe_with_capacity(10_000);

    for event in 0..10_000u64 {
        bus.publish(event);
    }

    c.bench_function("nidus bounded event publish at 10k capacity", |b| {
        b.iter(|| {
            black_box(&subscriber);
            bus.publish(black_box(10_000));
        });
    });
}

criterion_group!(benches, event_bus_setup);
criterion_main!(benches);
