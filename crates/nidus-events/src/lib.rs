//! Event bus abstractions.

use std::sync::{Arc, Mutex, Weak};

type SubscriberQueue<T> = Arc<Mutex<Vec<T>>>;
type SubscriberHandle<T> = Weak<Mutex<Vec<T>>>;
type SubscriberList<T> = Arc<Mutex<Vec<SubscriberHandle<T>>>>;

/// In-process typed event bus.
#[derive(Clone, Debug)]
pub struct EventBus<T> {
    subscribers: SubscriberList<T>,
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
        self.subscribers
            .lock()
            .unwrap()
            .push(Arc::downgrade(&queue));
        EventSubscriber { queue }
    }

    /// Publishes an event to current subscribers.
    pub fn publish(&self, event: T) {
        for subscriber in self.live_subscribers() {
            subscriber.lock().unwrap().push(event.clone());
        }
    }

    /// Returns the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.live_subscribers().len()
    }

    fn live_subscribers(&self) -> Vec<SubscriberQueue<T>> {
        let mut subscribers = self.subscribers.lock().unwrap();
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
        std::mem::take(&mut *self.queue.lock().unwrap())
    }
}
