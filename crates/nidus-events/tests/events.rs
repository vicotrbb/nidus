use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
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
