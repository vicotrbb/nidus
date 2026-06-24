#![deny(missing_docs)]

//! Event bus abstractions.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard, Weak},
};

type SubscriberQueue<T> = Arc<Mutex<Vec<T>>>;
type SubscriberHandle<T> = Weak<Mutex<Vec<T>>>;
type SubscriberList<T> = Arc<Mutex<Vec<SubscriberHandle<T>>>>;

/// In-process typed event bus.
#[derive(Clone, Debug)]
pub struct EventBus<T> {
    subscribers: SubscriberList<T>,
}

/// Context emitted when an event is observed.
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

/// Event bus wrapper that records publication context.
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
    pub fn subscribe(&self) -> EventSubscriber<T> {
        let queue = Arc::new(Mutex::new(Vec::new()));
        lock_unpoisoned(&self.subscribers).push(Arc::downgrade(&queue));
        EventSubscriber { queue }
    }

    /// Publishes an event to current subscribers.
    pub fn publish(&self, event: T) {
        for subscriber in self.live_subscribers() {
            lock_unpoisoned(&subscriber).push(event.clone());
        }
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
    queue: Arc<Mutex<Vec<T>>>,
}

impl<T> EventSubscriber<T> {
    /// Drains all received events.
    pub fn drain(&self) -> Vec<T> {
        std::mem::take(&mut *lock_unpoisoned(&self.queue))
    }
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;

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
}
