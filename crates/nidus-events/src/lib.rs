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
    #[must_use]
    fn push(&mut self, event: T) -> Option<T> {
        match &mut self.events {
            SubscriberEvents::Unbounded(events) => {
                events.push(event);
                None
            }
            SubscriberEvents::Bounded { events, capacity } => {
                if *capacity == 0 {
                    // A zero-capacity subscriber keeps nothing.
                    return Some(event);
                }
                let evicted = if events.len() == *capacity {
                    // VecDeque makes oldest-event eviction constant-time.
                    events.pop_front()
                } else {
                    None
                };
                events.push_back(event);
                evicted
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

struct LiveSubscribers<T> {
    first: Option<SubscriberQueue<T>>,
    additional: Vec<SubscriberQueue<T>>,
}

impl<T> LiveSubscribers<T> {
    fn new() -> Self {
        Self {
            first: None,
            additional: Vec::new(),
        }
    }

    fn push(&mut self, subscriber: SubscriberQueue<T>) {
        if self.first.is_none() {
            self.first = Some(subscriber);
        } else {
            // Keep live-subscriber collection allocation-free for the common
            // zero/one-subscriber case. This vector allocates only when fan-out
            // actually has a second target.
            self.additional.push(subscriber);
        }
    }
}

/// In-process typed event bus.
///
/// Subscribers receive events published after they subscribe. Events are cloned
/// for every active subscriber except the final one, which receives the original
/// value, and remain queued until that subscriber calls [`EventSubscriber::drain`].
/// Dropped subscribers are pruned on the next publish or subscriber-count check.
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
/// [`ObservedEventBus`]. Cloned contexts share their immutable attributes until
/// [`Self::with_attribute`] enriches one of them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedEventContext {
    operation_id: String,
    event_name: String,
    attributes: Arc<BTreeMap<String, String>>,
}

impl ObservedEventContext {
    /// Creates observed event context.
    pub fn new(operation_id: impl Into<String>, event_name: impl Into<String>) -> Self {
        Self {
            operation_id: operation_id.into(),
            event_name: event_name.into(),
            attributes: Arc::new(BTreeMap::new()),
        }
    }

    /// Adds a context attribute.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.attributes).insert(key.into(), value.into());
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
    attributes: Arc<BTreeMap<String, String>>,
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
            attributes: Arc::new(BTreeMap::new()),
            operation_id_generator: Arc::new(|| uuid::Uuid::new_v4().to_string()),
        }
    }

    /// Adds a context attribute propagated to future observed publications.
    pub fn context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        // Cloned wrappers retain value semantics while sharing read-mostly
        // configuration until one clone is enriched.
        Arc::make_mut(&mut self.attributes).insert(key.into(), value.into());
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
        let context = self.context_for(event_name);
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

    fn context_for(&self, event_name: String) -> ObservedEventContext {
        ObservedEventContext {
            operation_id: (self.operation_id_generator)(),
            event_name,
            // Attributes are configuration: publication only needs a shared
            // snapshot, while later enrichment remains copy-on-write.
            attributes: Arc::clone(&self.attributes),
        }
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
    /// The event is cloned for every active subscriber except the final one,
    /// which receives the original value. Bounded subscribers may evict the
    /// oldest event to honor their capacity.
    pub fn publish(&self, event: T) {
        let LiveSubscribers {
            first,
            mut additional,
        } = self.live_subscribers();
        let Some(first) = first else {
            return;
        };

        let Some(last) = additional.pop() else {
            let evicted = {
                let mut queue = lock_unpoisoned(&first);
                queue.push(event)
            };
            drop(evicted);
            return;
        };

        let evicted = {
            let mut queue = lock_unpoisoned(&first);
            queue.push(event.clone())
        };
        drop(evicted);
        for subscriber in additional {
            let evicted = {
                let mut queue = lock_unpoisoned(&subscriber);
                queue.push(event.clone())
            };
            drop(evicted);
        }
        let evicted = {
            let mut queue = lock_unpoisoned(&last);
            queue.push(event)
        };
        drop(evicted);
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
        let mut subscribers = lock_unpoisoned(&self.subscribers);
        subscribers.retain(|subscriber| subscriber.upgrade().is_some());
        subscribers.len()
    }

    fn live_subscribers(&self) -> LiveSubscribers<T> {
        let mut subscribers = lock_unpoisoned(&self.subscribers);
        let mut live = LiveSubscribers::new();
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
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
            mpsc,
        },
        thread,
        time::Duration,
    };

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

    #[derive(Clone)]
    struct DropActionEvent {
        id: u64,
        action: Option<Arc<dyn Fn() + Send + Sync>>,
    }

    impl DropActionEvent {
        fn plain(id: u64) -> Self {
            Self { id, action: None }
        }

        fn with_action(id: u64, action: impl Fn() + Send + Sync + 'static) -> Self {
            Self {
                id,
                action: Some(Arc::new(action)),
            }
        }
    }

    impl Drop for DropActionEvent {
        fn drop(&mut self) {
            if let Some(action) = self.action.take() {
                action();
            }
        }
    }

    struct PanicCloneEvent {
        id: u64,
        clone_calls: Arc<AtomicUsize>,
        panic_on_call: usize,
    }

    impl PanicCloneEvent {
        fn new(id: u64, clone_calls: Arc<AtomicUsize>, panic_on_call: usize) -> Self {
            Self {
                id,
                clone_calls,
                panic_on_call,
            }
        }
    }

    impl Clone for PanicCloneEvent {
        fn clone(&self) -> Self {
            let call = self.clone_calls.fetch_add(1, Ordering::Relaxed) + 1;
            assert_ne!(call, self.panic_on_call, "injected clone panic");
            Self::new(self.id, Arc::clone(&self.clone_calls), self.panic_on_call)
        }
    }

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
    fn first_clone_panic_poisoning_and_recovery_match_existing_contract() {
        let bus = EventBus::<PanicCloneEvent>::new();
        let first = bus.subscribe();
        let last = bus.subscribe();
        let clone_calls = Arc::new(AtomicUsize::new(0));

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.publish(PanicCloneEvent::new(1, Arc::clone(&clone_calls), 1));
        }));

        assert!(panic.is_err());
        assert!(first.queue.lock().is_err());
        assert!(last.queue.lock().is_ok());
        assert!(first.drain().is_empty());
        assert!(last.drain().is_empty());

        bus.publish(PanicCloneEvent::new(2, Arc::clone(&clone_calls), 1));
        assert_eq!(first.drain().pop().unwrap().id, 2);
        assert_eq!(last.drain().pop().unwrap().id, 2);
    }

    #[test]
    fn later_clone_panic_preserves_partial_delivery_and_recovers() {
        let bus = EventBus::<PanicCloneEvent>::new();
        let first = bus.subscribe();
        let middle = bus.subscribe();
        let last = bus.subscribe();
        let clone_calls = Arc::new(AtomicUsize::new(0));

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.publish(PanicCloneEvent::new(1, Arc::clone(&clone_calls), 2));
        }));

        assert!(panic.is_err());
        assert!(first.queue.lock().is_ok());
        assert!(middle.queue.lock().is_err());
        assert!(last.queue.lock().is_ok());
        assert_eq!(first.drain().pop().unwrap().id, 1);
        assert!(middle.drain().is_empty());
        assert!(last.drain().is_empty());

        bus.publish(PanicCloneEvent::new(2, Arc::clone(&clone_calls), 2));
        assert_eq!(first.drain().pop().unwrap().id, 2);
        assert_eq!(middle.drain().pop().unwrap().id, 2);
        assert_eq!(last.drain().pop().unwrap().id, 2);
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
    fn bounded_eviction_drops_reentrant_payload_after_unlocking_queue() {
        let bus = EventBus::<DropActionEvent>::new();
        let subscriber = bus.subscribe_with_capacity(1);
        let reentrant_bus = bus.clone();
        let reentrant_subscriber = subscriber.clone();
        let (result_tx, result_rx) = mpsc::channel();

        bus.publish(DropActionEvent::with_action(1, move || {
            reentrant_bus.publish(DropActionEvent::plain(3));
            let ids = reentrant_subscriber
                .drain()
                .into_iter()
                .map(|event| event.id)
                .collect::<Vec<_>>();
            result_tx.send(ids).unwrap();
        }));
        bus.publish(DropActionEvent::plain(2));

        assert_eq!(
            result_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
            vec![3]
        );
        assert!(subscriber.drain().is_empty());
    }

    #[test]
    fn zero_capacity_rejection_drops_reentrant_payload_after_unlocking_queue() {
        let bus = EventBus::<DropActionEvent>::new();
        let subscriber = bus.subscribe_with_capacity(0);
        let reentrant_bus = bus.clone();
        let (dropped_tx, dropped_rx) = mpsc::channel();

        bus.publish(DropActionEvent::with_action(1, move || {
            reentrant_bus.publish(DropActionEvent::plain(2));
            dropped_tx.send(()).unwrap();
        }));

        dropped_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(subscriber.drain().is_empty());
    }

    #[test]
    fn bounded_eviction_drop_panic_does_not_poison_queue() {
        let bus = EventBus::<DropActionEvent>::new();
        let subscriber = bus.subscribe_with_capacity(1);
        bus.publish(DropActionEvent::with_action(1, || {
            panic!("panic from evicted payload destructor");
        }));

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            bus.publish(DropActionEvent::plain(2));
        }));

        assert!(panic.is_err());
        assert!(subscriber.queue.lock().is_ok());
        assert_eq!(
            subscriber
                .drain()
                .into_iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn bounded_eviction_slow_drop_does_not_hold_queue_lock() {
        let bus = EventBus::<DropActionEvent>::new();
        let subscriber = bus.subscribe_with_capacity(1);
        let (drop_started_tx, drop_started_rx) = mpsc::channel();
        let (release_drop_tx, release_drop_rx) = mpsc::channel();
        let release_drop_rx = Arc::new(Mutex::new(release_drop_rx));

        bus.publish(DropActionEvent::with_action(1, move || {
            drop_started_tx.send(()).unwrap();
            release_drop_rx
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(1))
                .unwrap();
        }));

        let publisher = {
            let bus = bus.clone();
            thread::spawn(move || bus.publish(DropActionEvent::plain(2)))
        };
        drop_started_rx
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        let (drained_tx, drained_rx) = mpsc::channel();
        let drainer = {
            let subscriber = subscriber.clone();
            thread::spawn(move || {
                let ids = subscriber
                    .drain()
                    .into_iter()
                    .map(|event| event.id)
                    .collect::<Vec<_>>();
                drained_tx.send(ids).unwrap();
            })
        };

        assert_eq!(
            drained_rx.recv_timeout(Duration::from_millis(250)).unwrap(),
            vec![2]
        );
        release_drop_tx.send(()).unwrap();
        publisher.join().unwrap();
        drainer.join().unwrap();
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

    #[test]
    fn observed_context_shares_configured_attributes_until_enriched() {
        let observed = EventBus::<UserCreated>::new()
            .observed(())
            .operation_id_generator(|| "event-run".to_owned())
            .context("service", "users-api");

        let enriched_observed = observed.clone().context("region", "sa-east-1");
        assert!(!Arc::ptr_eq(
            &enriched_observed.attributes,
            &observed.attributes
        ));
        assert!(!observed.attributes.contains_key("region"));
        assert_eq!(
            enriched_observed.attributes.get("region").unwrap(),
            "sa-east-1"
        );

        let context = observed.context_for("user.created".to_owned());
        assert!(Arc::ptr_eq(&context.attributes, &observed.attributes));

        let enriched = context.clone().with_attribute("request_id", "request-42");
        assert!(!Arc::ptr_eq(&enriched.attributes, &context.attributes));
        assert_eq!(context.attributes().get("service").unwrap(), "users-api");
        assert!(!context.attributes().contains_key("request_id"));
        assert_eq!(
            enriched.attributes().get("request_id").unwrap(),
            "request-42"
        );
    }
}
