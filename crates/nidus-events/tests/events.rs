use nidus_events::EventBus;

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserCreated(u64);

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
