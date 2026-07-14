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

    let single_bus = EventBus::new();
    let single_subscriber = single_bus.subscribe_with_capacity(1);
    single_bus.publish(0u64);

    c.bench_function("nidus single-subscriber bounded event publish", |b| {
        b.iter(|| {
            black_box(&single_subscriber);
            single_bus.publish(black_box(1u64));
        });
    });

    let four_bus = EventBus::new();
    let four_subscribers = [
        four_bus.subscribe_with_capacity(1),
        four_bus.subscribe_with_capacity(1),
        four_bus.subscribe_with_capacity(1),
        four_bus.subscribe_with_capacity(1),
    ];
    four_bus.publish(0u64);

    c.bench_function("nidus four-subscriber bounded event publish", |b| {
        b.iter(|| {
            black_box(&four_subscribers);
            four_bus.publish(black_box(1u64));
        });
    });
}

criterion_group!(benches, event_bus_setup);
criterion_main!(benches);
