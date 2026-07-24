use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use nidus_events::{
    EventBus, EventObserver, ObservedEventBus, ObservedEventContext, event_observer_channel,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct UserCreated(u64);

type ObservedDeliveries = Arc<Mutex<Vec<(String, Vec<UserCreated>)>>>;

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

#[test]
fn event_bus_observed_wraps_bus_without_repeating_type_names() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let observer = RecordingObserver::default();
    let observed = bus
        .observed(observer.clone())
        .operation_id_generator(|| "event-run-3".to_owned());

    observed.publish_named("user.created", UserCreated(44));

    assert_eq!(subscriber.drain(), vec![UserCreated(44)]);
    assert_eq!(observer.events(), ["published user.created event-run-3"]);
}

#[derive(Clone)]
struct DeliveryOrderObserver {
    subscriber: nidus_events::EventSubscriber<UserCreated>,
    observed: ObservedDeliveries,
}

impl EventObserver<UserCreated> for DeliveryOrderObserver {
    fn on_event_published(&self, context: &ObservedEventContext) {
        self.observed
            .lock()
            .unwrap()
            .push((context.event_name().to_owned(), self.subscriber.drain()));
    }
}

#[test]
fn observer_runs_exactly_once_after_subscriber_delivery() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let observed_events = Arc::new(Mutex::new(Vec::new()));
    let observed = bus
        .observed(DeliveryOrderObserver {
            subscriber: subscriber.clone(),
            observed: Arc::clone(&observed_events),
        })
        .operation_id_generator(|| "event-order-1".to_owned());

    observed.publish_named("user.created", UserCreated(45));

    assert_eq!(
        *observed_events.lock().unwrap(),
        vec![("user.created".to_owned(), vec![UserCreated(45)])]
    );
    assert!(subscriber.drain().is_empty());
}

#[derive(Clone)]
struct BlockingObserver {
    entered: mpsc::Sender<()>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
}

impl EventObserver<UserCreated> for BlockingObserver {
    fn on_event_published(&self, _context: &ObservedEventContext) {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
    }
}

#[test]
fn slow_observer_does_not_delay_already_completed_subscriber_delivery() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let observed = bus.observed(BlockingObserver {
        entered: entered_tx,
        release: Arc::new(Mutex::new(release_rx)),
    });

    let publisher = thread::spawn(move || {
        observed.publish_named("user.created", UserCreated(46));
    });
    entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    assert_eq!(subscriber.drain(), vec![UserCreated(46)]);
    release_tx.send(()).unwrap();
    publisher.join().unwrap();
}

#[derive(Clone)]
struct PanickingObserver;

impl EventObserver<UserCreated> for PanickingObserver {
    fn on_event_published(&self, _context: &ObservedEventContext) {
        panic!("panic from event observer");
    }
}

#[test]
fn panicking_observer_preserves_completed_delivery_and_future_bus_use() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let observed = bus.clone().observed(PanickingObserver);

    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        observed.publish_named("user.created", UserCreated(47));
    }));

    assert!(panic.is_err());
    assert_eq!(subscriber.drain(), vec![UserCreated(47)]);
    bus.publish(UserCreated(48));
    assert_eq!(subscriber.drain(), vec![UserCreated(48)]);
}

#[test]
fn disconnected_channel_observer_remains_best_effort() {
    let bus = EventBus::<UserCreated>::new();
    let subscriber = bus.subscribe();
    let (observer, receiver) = event_observer_channel();
    drop(receiver);
    let observed = bus.observed(observer);

    observed.publish_named("user.created", UserCreated(49));

    assert_eq!(subscriber.drain(), vec![UserCreated(49)]);
}
