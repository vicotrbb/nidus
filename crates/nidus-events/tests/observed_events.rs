use std::sync::{Arc, Mutex};

use nidus_events::{
    EventBus, EventObserver, ObservedEventBus, ObservedEventContext, event_observer_channel,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserCreated(u64);

#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<String>>>,
}

impl RecordingObserver {
    fn events(&self) -> Vec<String> {
        self.events.lock().unwrap().clone()
    }
}

impl<T> EventObserver<T> for RecordingObserver
where
    T: Clone + Send + Sync + 'static,
{
    fn on_event_published(&self, context: &ObservedEventContext) {
        self.events.lock().unwrap().push(format!(
            "published {} {}",
            context.event_name(),
            context.operation_id()
        ));
    }
}

#[test]
fn observed_event_bus_publishes_events_and_records_context() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let observer = RecordingObserver::default();
    let observed = ObservedEventBus::new(bus, observer.clone())
        .operation_id_generator(|| "event-run-1".to_owned())
        .context("request_id", "req-123");

    observed.publish_named("user.created", UserCreated(42));

    assert_eq!(subscriber.drain(), vec![UserCreated(42)]);
    assert_eq!(observer.events(), ["published user.created event-run-1"]);
}

#[test]
fn observed_event_bus_can_enqueue_context_for_off_thread_observers() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let (observer, receiver) = event_observer_channel();
    let observed = ObservedEventBus::new(bus, observer)
        .operation_id_generator(|| "event-run-2".to_owned())
        .context("request_id", "req-456");

    observed.publish_named("user.created", UserCreated(43));

    assert_eq!(subscriber.drain(), vec![UserCreated(43)]);
    let context = receiver.try_recv().unwrap();
    assert_eq!(context.event_name(), "user.created");
    assert_eq!(context.operation_id(), "event-run-2");
    assert_eq!(context.attributes().get("request_id").unwrap(), "req-456");
}
