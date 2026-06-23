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
