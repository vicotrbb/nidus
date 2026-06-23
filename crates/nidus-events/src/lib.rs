//! Event bus abstractions.

use std::sync::{Arc, Mutex};

/// In-process typed event bus.
#[derive(Clone, Debug)]
pub struct EventBus<T> {
    subscribers: Arc<Mutex<Vec<Arc<Mutex<Vec<T>>>>>>,
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
        self.subscribers.lock().unwrap().push(Arc::clone(&queue));
        EventSubscriber { queue }
    }

    /// Publishes an event to current subscribers.
    pub fn publish(&self, event: T) {
        for subscriber in self.subscribers.lock().unwrap().iter() {
            subscriber.lock().unwrap().push(event.clone());
        }
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
