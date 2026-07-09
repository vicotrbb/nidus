#![deny(missing_docs)]

//! Event bus abstractions.
//!
//! The built-in bus is in-process and in-memory. It is useful for local domain
//! events, tests, and adapters that bridge to a real broker, but it is not a
//! durable queue and does not deliver events across processes.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::mpsc,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

/// Bounded buffer backing a single subscriber's event queue.
///
/// The default capacity is unbounded (every published event is retained until
/// [`EventSubscriber::drain`] is called). A bounded buffer evicts the oldest
/// event when pushing beyond its capacity, so a slow or absent drainer can
/// never grow memory without limit.
#[derive(Clone, Debug)]
struct SubscriberBuffer<T> {
    events: SubscriberEvents<T>,
}

#[derive(Clone, Debug)]
enum SubscriberEvents<T> {
    Unbounded(Vec<T>),
    Bounded {
        events: VecDeque<T>,
        capacity: usize,
    },
}

impl<T> Default for SubscriberBuffer<T> {
    fn default() -> Self {
        Self {
            events: SubscriberEvents::Unbounded(Vec::new()),
        }
    }
}

impl<T> SubscriberBuffer<T> {
    fn push(&mut self, event: T) {
        match &mut self.events {
            SubscriberEvents::Unbounded(events) => events.push(event),
            SubscriberEvents::Bounded { events, capacity } => {
                if *capacity == 0 {
                    // A zero-capacity subscriber keeps nothing.
                    return;
                }
                if events.len() == *capacity {
                    // VecDeque makes oldest-event eviction constant-time.
                    events.pop_front();
                }
                events.push_back(event);
            }
        }
    }

    fn drain(&mut self) -> Vec<T> {
        match &mut self.events {
            SubscriberEvents::Unbounded(events) => std::mem::take(events),
            SubscriberEvents::Bounded { events, .. } => std::mem::take(events).into(),
        }
    }
}

type SubscriberQueue<T> = Arc<Mutex<SubscriberBuffer<T>>>;
type SubscriberHandle<T> = Weak<Mutex<SubscriberBuffer<T>>>;
type SubscriberList<T> = Arc<Mutex<Vec<SubscriberHandle<T>>>>;

/// In-process typed event bus.
///
/// Subscribers receive events published after they subscribe. Events are cloned
/// into each active subscriber queue and remain there until that subscriber
/// calls [`EventSubscriber::drain`]. Dropped subscribers are pruned on the next
/// publish or subscriber-count check.
///
/// ```
/// use nidus_events::EventBus;
///
/// #[derive(Clone, Debug, PartialEq, Eq)]
/// struct UserCreated { id: u64 }
///
/// let bus = EventBus::<UserCreated>::new();
/// let subscriber = bus.subscribe();
///
/// bus.publish(UserCreated { id: 42 });
/// assert_eq!(subscriber.drain(), vec![UserCreated { id: 42 }]);
/// ```
#[derive(Clone, Debug)]
pub struct EventBus<T> {
    subscribers: SubscriberList<T>,
}

/// Context emitted when an event is observed.
///
/// Observed publications receive a generated operation ID, a stable event name
/// supplied by the caller, and any attributes configured on the
/// [`ObservedEventBus`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedEventContext {
    operation_id: String,
    event_name: String,
    attributes: BTreeMap<String, String>,
}

impl ObservedEventContext {
    /// Creates observed event context.
    pub fn new(operation_id: impl Into<String>, event_name: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            event_name: event_name.into(),
            attributes: BTreeMap::new(),
        }
    }

    /// Adds a context attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Returns the operation id.
    pub fn operation_id(&self) -> &str {
        &self.operation_id
    }

    /// Returns the event name.
    pub fn event_name(&self) -> &str {
        &self.event_name
    }

    /// Returns context attributes.
    pub fn attributes(&self) -> &BTreeMap<String, String> {
        &self.attributes
    }
}

/// Observer hook for event publication.
///
/// The hook runs synchronously after the event has been published to in-memory
/// subscribers. Keep implementations fast and non-blocking, or forward to your
/// own async/export pipeline.
pub trait EventObserver<T>: Clone + Send + Sync + 'static
where
    T: Clone + Send + Sync + 'static,
{
    /// Called after an event is published.
    fn on_event_published(&self, context: &ObservedEventContext);
}

impl<T> EventObserver<T> for ()
where
    T: Clone + Send + Sync + 'static,
{
    fn on_event_published(&self, _context: &ObservedEventContext) {}
}

/// Observer implementation that sends observed event contexts to a channel.
///
/// Use [`event_observer_channel`] when the publish path should only enqueue
/// telemetry and another thread or task will do slower export work. Sending to
/// the channel is best-effort: if the receiver has been dropped, publication
/// still succeeds and the context is discarded.
#[derive(Clone)]
pub struct EventObserverChannel {
    sender: mpsc::Sender<ObservedEventContext>,
}

impl EventObserverChannel {
    /// Creates a channel observer from an existing sender.
    pub fn new(sender: mpsc::Sender<ObservedEventContext>) -> Self {
        Self { sender }
    }
}

impl<T> EventObserver<T> for EventObserverChannel
where
    T: Clone + Send + Sync + 'static,
{
    fn on_event_published(&self, context: &ObservedEventContext) {
        let _ = self.sender.send(context.clone());
    }
}

/// Creates a channel-backed event observer and its receiver.
///
/// The returned observer can be passed to [`ObservedEventBus::new`]. The
/// receiver yields [`ObservedEventContext`] values in publication order for a
/// separate exporter thread or task.
pub fn event_observer_channel() -> (EventObserverChannel, mpsc::Receiver<ObservedEventContext>) {
    let (sender, receiver) = mpsc::channel();
    (EventObserverChannel::new(sender), receiver)
}

/// Event bus wrapper that records publication context.
///
/// `ObservedEventBus` adds a tracing span and observer callback around
/// [`EventBus::publish`]. It does not change delivery semantics: publication is
/// still in-process, non-durable fan-out to current subscribers.
///
/// ```
/// use std::sync::{Arc, Mutex};
/// use nidus_events::{EventBus, EventObserver, ObservedEventBus, ObservedEventContext};
///
/// #[derive(Clone)]
/// struct UserCreated;
///
/// #[derive(Clone)]
/// struct Observer(Arc<Mutex<Vec<String>>>);
///
/// impl EventObserver<UserCreated> for Observer {
///     fn on_event_published(&self, context: &ObservedEventContext) {
///         self.0.lock().unwrap().push(context.event_name().to_owned());
///     }
/// }
///
/// let events = Arc::new(Mutex::new(Vec::new()));
/// let observed = ObservedEventBus::new(EventBus::new(), Observer(Arc::clone(&events)))
///     .context("service", "users-api");
///
/// observed.publish_named("user.created", UserCreated);
/// assert_eq!(events.lock().unwrap().as_slice(), ["user.created"]);
/// ```
#[derive(Clone)]
pub struct ObservedEventBus<T, O = ()>
where
    T: Clone + Send + Sync + 'static,
    O: EventObserver<T>,
{
    bus: EventBus<T>,
    observer: O,
    attributes: BTreeMap<String, String>,
    operation_id_generator: Arc<dyn Fn() -> String + Send + Sync>,
}

impl<T, O> ObservedEventBus<T, O>
where
    T: Clone + Send + Sync + 'static,
    O: EventObserver<T>,
{
    /// Creates an observed wrapper around an event bus.
    pub fn new(bus: EventBus<T>, observer: O) -> Self {
        Self {
            bus,
            observer,
            attributes: BTreeMap::new(),
            operation_id_generator: Arc::new(|| uuid::Uuid::new_v4().to_string()),
        }
    }

    /// Adds a context attribute propagated to future observed publications.
    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Replaces the operation id generator.
    pub fn operation_id_generator(
        mut self,
        generator: impl Fn() -> String + Send + Sync + 'static,
    ) -> Self {
        self.operation_id_generator = Arc::new(generator);
        self
    }

    /// Publishes an event with an explicit stable event name.
    ///
    /// The event is first published to current subscribers, then the observer is
    /// called with the generated [`ObservedEventContext`].
    pub fn publish_named(&self, event_name: impl Into<String>, event: T) {
        let event_name = event_name.into();
        let mut context = ObservedEventContext::new((self.operation_id_generator)(), &event_name);
        for (key, value) in &self.attributes {
            context = context.with_attribute(key.clone(), value.clone());
        }
        let span = tracing::info_span!(
            "event.publish",
            event.name = %context.event_name(),
            event.operation_id = %context.operation_id()
        );
        let _entered = span.enter();
        self.bus.publish(event);
        self.observer.on_event_published(&context);
    }

    /// Returns the wrapped event bus.
    pub fn bus(&self) -> &EventBus<T> {
        &self.bus
    }
}

impl<T> EventBus<T>
where
    T: Clone,
{
    /// Creates an empty event bus.
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Subscribes to future events.
    ///
    /// The returned subscriber does not replay events published before this
    /// call. Its queue is unbounded; use [`Self::subscribe_with_capacity`] to
    /// bound memory when the subscriber may drain slowly or never.
    pub fn subscribe(&self) -> EventSubscriber<T> {
        self.subscribe_with_buffer(SubscriberBuffer::default())
    }

    /// Subscribes to future events with a bounded queue.
    ///
    /// The subscriber retains at most `capacity` events. When a new event would
    /// exceed the capacity, the oldest event is evicted, so memory stays bounded
    /// even if the subscriber never calls [`EventSubscriber::drain`]. A capacity
    /// of `0` keeps no events (useful when only the [`ObservedEventBus`]
    /// observer side-effect matters).
    pub fn subscribe_with_capacity(&self, capacity: usize) -> EventSubscriber<T> {
        self.subscribe_with_buffer(SubscriberBuffer {
            events: SubscriberEvents::Bounded {
                // Preserve the previous lazy-allocation behavior: declaring a
                // large bound should not allocate until events arrive.
                events: VecDeque::new(),
                capacity,
            },
        })
    }

    fn subscribe_with_buffer(&self, buffer: SubscriberBuffer<T>) -> EventSubscriber<T> {
        let queue = Arc::new(Mutex::new(buffer));
        lock_unpoisoned(&self.subscribers).push(Arc::downgrade(&queue));
        EventSubscriber { queue }
    }

    /// Publishes an event to current subscribers.
    ///
    /// The event is cloned once per active subscriber (bounded subscribers may
    /// evict the oldest event to honor their capacity).
    pub fn publish(&self, event: T) {
        for subscriber in self.live_subscribers() {
            lock_unpoisoned(&subscriber).push(event.clone());
        }
    }

    /// Wraps this bus with an observer.
    pub fn observed<O>(self, observer: O) -> ObservedEventBus<T, O>
    where
        T: Send + Sync + 'static,
        O: EventObserver<T>,
    {
        ObservedEventBus::new(self, observer)
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.live_subscribers().len()
    }

    fn live_subscribers(&self) -> Vec<SubscriberQueue<T>> {
        let mut subscribers = lock_unpoisoned(&self.subscribers);
        let mut live = Vec::new();
        subscribers.retain(|subscriber| {
            if let Some(queue) = subscriber.upgrade() {
                live.push(queue);
                true
            } else {
                false
            }
        });
        live
    }
}

impl<T> Default for EventBus<T>
where
    T: Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

/// Subscription handle for an event bus.
#[derive(Clone, Debug)]
pub struct EventSubscriber<T> {
    queue: SubscriberQueue<T>,
}

impl<T> EventSubscriber<T> {
    /// Drains all received events.
    pub fn drain(&self) -> Vec<T> {
        lock_unpoisoned(&self.queue).drain()
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("event bus mutex poisoned; recovering inner state");
        poisoned.into_inner()
    })
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::*;
    use tracing::Level;
    use tracing_subscriber::{Layer, fmt::MakeWriter, layer::SubscriberExt};

    #[derive(Clone, Default)]
    struct SharedLogWriter {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl SharedLogWriter {
        fn contents(&self) -> String {
            String::from_utf8(self.output.lock().unwrap().clone()).unwrap()
        }

        fn clear(&self) {
            self.output.lock().unwrap().clear();
        }
    }

    impl<'writer> MakeWriter<'writer> for SharedLogWriter {
        type Writer = SharedLogGuard;

        fn make_writer(&'writer self) -> Self::Writer {
            SharedLogGuard {
                output: Arc::clone(&self.output),
            }
        }
    }

    struct SharedLogGuard {
        output: Arc<Mutex<Vec<u8>>>,
    }

    impl std::io::Write for SharedLogGuard {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct UserCreated(u64);

    #[test]
    fn event_bus_recovers_from_poisoned_subscriber_list() {
        let bus = EventBus::<UserCreated>::new();
        let subscribers = Arc::clone(&bus.subscribers);

        let panic = thread::spawn(move || {
            let _subscribers = subscribers.lock().unwrap();
            panic!("poison subscriber list");
        });
        assert!(panic.join().is_err());

        let subscriber = bus.subscribe();
        bus.publish(UserCreated(42));

        assert_eq!(subscriber.drain(), vec![UserCreated(42)]);
    }

    #[test]
    fn event_bus_warns_when_recovering_from_poisoned_subscriber_list() {
        let bus = EventBus::<UserCreated>::new();
        let subscribers = Arc::clone(&bus.subscribers);
        let panic = thread::spawn(move || {
            let _subscribers = subscribers.lock().unwrap();
            panic!("poison subscriber list");
        });
        assert!(panic.join().is_err());

        let writer = SharedLogWriter::default();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer.clone())
                .with_ansi(false)
                .with_target(false)
                .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                    Level::WARN,
                )),
        );

        tracing::subscriber::with_default(subscriber, || {
            for _ in 0..16 {
                writer.clear();
                tracing_core::callsite::rebuild_interest_cache();
                let _subscriber = bus.subscribe();
                let logs = writer.contents();
                if logs.contains("event bus mutex poisoned") {
                    return;
                }
                std::thread::yield_now();
            }
        });

        let logs = writer.contents();
        assert!(logs.contains("event bus mutex poisoned"), "{logs}");
    }

    #[test]
    fn event_bus_recovers_from_poisoned_subscriber_queue() {
        let bus = EventBus::<UserCreated>::new();
        let subscriber = bus.subscribe();
        let queue = Arc::clone(&subscriber.queue);

        let panic = thread::spawn(move || {
            let _queue = queue.lock().unwrap();
            panic!("poison subscriber queue");
        });
        assert!(panic.join().is_err());

        bus.publish(UserCreated(42));

        assert_eq!(subscriber.drain(), vec![UserCreated(42)]);
    }

    #[test]
    fn bounded_subscriber_drops_oldest_events_beyond_capacity() {
        let bus = EventBus::<UserCreated>::new();
        let bounded = bus.subscribe_with_capacity(2);

        bus.publish(UserCreated(1));
        bus.publish(UserCreated(2));
        bus.publish(UserCreated(3));

        // Capacity is 2: the oldest event is evicted to keep the buffer
        // bounded, so a slow/absent drainer can never grow memory unbounded.
        assert_eq!(bounded.drain(), vec![UserCreated(2), UserCreated(3)]);

        // A second batch after draining continues to respect the cap.
        bus.publish(UserCreated(4));
        bus.publish(UserCreated(5));
        bus.publish(UserCreated(6));
        assert_eq!(bounded.drain(), vec![UserCreated(5), UserCreated(6)]);
    }

    #[test]
    fn zero_capacity_subscriber_never_retains_events() {
        let bus = EventBus::<UserCreated>::new();
        let subscriber = bus.subscribe_with_capacity(0);

        bus.publish(UserCreated(1));
        bus.publish(UserCreated(2));

        assert!(subscriber.drain().is_empty());
    }

    #[test]
    fn unbounded_subscriber_keeps_all_events_by_default() {
        let bus = EventBus::<UserCreated>::new();
        let subscriber = bus.subscribe();

        for id in 1..=50u64 {
            bus.publish(UserCreated(id));
        }

        let drained: Vec<u64> = subscriber
            .drain()
            .into_iter()
            .map(|event| event.0)
            .collect();
        assert_eq!(drained, (1..=50).collect::<Vec<_>>());
    }
}
