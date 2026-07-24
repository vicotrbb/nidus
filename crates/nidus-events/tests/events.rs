use std::{
    collections::VecDeque,
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

use nidus_events::EventBus;

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserCreated(u64);

#[derive(Debug)]
struct CloneCountedEvent {
    id: u64,
    clone_count: Arc<AtomicUsize>,
}

impl CloneCountedEvent {
    fn new(id: u64, clone_count: &Arc<AtomicUsize>) -> Self {
        Self {
            id,
            clone_count: Arc::clone(clone_count),
        }
    }
}

impl Clone for CloneCountedEvent {
    fn clone(&self) -> Self {
        self.clone_count.fetch_add(1, Ordering::Relaxed);
        Self {
            id: self.id,
            clone_count: Arc::clone(&self.clone_count),
        }
    }
}

#[test]
fn event_bus_delivers_typed_events_to_subscribers() {
    let bus = EventBus::<UserCreated>::new();
    let received = bus.subscribe();

    bus.publish(UserCreated(42));

    assert_eq!(received.drain(), vec![UserCreated(42)]);
}

#[test]
fn event_bus_delivers_to_each_live_subscriber() {
    let bus = EventBus::<UserCreated>::new();
    let first = bus.subscribe();
    let second = bus.subscribe();

    bus.publish(UserCreated(42));

    assert_eq!(bus.subscriber_count(), 2);
    assert_eq!(first.drain(), vec![UserCreated(42)]);
    assert_eq!(second.drain(), vec![UserCreated(42)]);
}

#[test]
fn event_bus_prunes_dropped_subscribers() {
    let bus = EventBus::<UserCreated>::new();
    let retained = bus.subscribe();
    let dropped = bus.subscribe();

    assert_eq!(bus.subscriber_count(), 2);
    drop(dropped);

    bus.publish(UserCreated(42));

    assert_eq!(bus.subscriber_count(), 1);
    assert_eq!(retained.drain(), vec![UserCreated(42)]);
}

#[test]
fn event_bus_moves_to_one_subscriber_and_clones_only_for_additional_subscribers() {
    let bus = EventBus::<CloneCountedEvent>::new();

    let no_subscriber_clones = Arc::new(AtomicUsize::new(0));
    bus.publish(CloneCountedEvent::new(0, &no_subscriber_clones));
    assert_eq!(no_subscriber_clones.load(Ordering::Relaxed), 0);

    let first = bus.subscribe();
    let one_subscriber_clones = Arc::new(AtomicUsize::new(0));
    bus.publish(CloneCountedEvent::new(1, &one_subscriber_clones));
    assert_eq!(one_subscriber_clones.load(Ordering::Relaxed), 0);

    let second = bus.subscribe();
    let two_subscriber_clones = Arc::new(AtomicUsize::new(0));
    bus.publish(CloneCountedEvent::new(2, &two_subscriber_clones));
    assert_eq!(two_subscriber_clones.load(Ordering::Relaxed), 1);

    assert_eq!(
        first
            .drain()
            .into_iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        second
            .drain()
            .into_iter()
            .map(|event| event.id)
            .collect::<Vec<_>>(),
        vec![2]
    );
}

#[test]
fn cloned_subscriber_handles_share_one_queue_without_duplication() {
    let bus = EventBus::<u64>::new();
    let first_handle = bus.subscribe();
    let second_handle = first_handle.clone();
    for event in 0..10_000 {
        bus.publish(event);
    }

    let first = thread::spawn(move || first_handle.drain());
    let second = thread::spawn(move || second_handle.drain());
    let mut combined = first.join().unwrap();
    combined.extend(second.join().unwrap());
    combined.sort_unstable();

    assert_eq!(combined, (0..10_000).collect::<Vec<_>>());
}

#[test]
fn concurrent_publishers_preserve_per_publisher_order_without_duplication() {
    const PUBLISHERS: u16 = 4;
    const EVENTS: u16 = 2_000;
    let bus = EventBus::<(u16, u16)>::new();
    let subscribers = (0..4).map(|_| bus.subscribe()).collect::<Vec<_>>();
    let barrier = Arc::new(Barrier::new(PUBLISHERS as usize));
    let publishers = (0..PUBLISHERS)
        .map(|publisher| {
            let bus = bus.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                for sequence in 0..EVENTS {
                    bus.publish((publisher, sequence));
                }
            })
        })
        .collect::<Vec<_>>();
    for publisher in publishers {
        publisher.join().unwrap();
    }

    let expected = (0..PUBLISHERS)
        .flat_map(|publisher| (0..EVENTS).map(move |sequence| (publisher, sequence)))
        .collect::<Vec<_>>();
    for subscriber in subscribers {
        let delivered = subscriber.drain();
        assert_eq!(delivered.len(), expected.len());
        let mut sorted = delivered.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, expected);
        for publisher in 0..PUBLISHERS {
            let sequence = delivered
                .iter()
                .filter_map(|(candidate, sequence)| (*candidate == publisher).then_some(*sequence))
                .collect::<Vec<_>>();
            assert_eq!(sequence, (0..EVENTS).collect::<Vec<_>>());
        }
    }
}

#[test]
fn generated_publish_drain_schedules_match_bounded_queue_model() {
    // Keep the full deterministic property surface on native runners. Miri's
    // interpreter exercises a representative subset so the memory-model pass
    // remains bounded while using the same state machine and assertions.
    let (seed_count, operations_per_seed) = if cfg!(miri) {
        (4_u64, 200)
    } else {
        (64_u64, 2_000)
    };
    for capacity in [0, 1, 2, 16, 1024] {
        for seed in 1..=seed_count {
            let bus = EventBus::<u64>::new();
            let subscriber = bus.subscribe_with_capacity(capacity);
            let mut model = VecDeque::new();
            let mut state = seed;
            let mut next_event = 0u64;

            for _ in 0..operations_per_seed {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                if state & 0b111 == 0 {
                    assert_eq!(subscriber.drain(), model.drain(..).collect::<Vec<_>>());
                } else {
                    bus.publish(next_event);
                    if capacity > 0 {
                        if model.len() == capacity {
                            model.pop_front();
                        }
                        model.push_back(next_event);
                    }
                    next_event += 1;
                }
            }

            assert_eq!(subscriber.drain(), model.into_iter().collect::<Vec<_>>());
        }
    }
}

#[test]
fn mixed_zero_and_nonzero_capacity_subscribers_preserve_newest_values() {
    let bus = EventBus::<u64>::new();
    let zero = bus.subscribe_with_capacity(0);
    let one = bus.subscribe_with_capacity(1);
    let sixteen = bus.subscribe_with_capacity(16);
    let unbounded = bus.subscribe();

    for event in 0..100 {
        bus.publish(event);
    }

    assert!(zero.drain().is_empty());
    assert_eq!(one.drain(), vec![99]);
    assert_eq!(sixteen.drain(), (84..100).collect::<Vec<_>>());
    assert_eq!(unbounded.drain(), (0..100).collect::<Vec<_>>());
}
