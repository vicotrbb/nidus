#![deny(missing_docs)]

//! Event bus abstractions.

use std::sync::{Arc, Mutex, MutexGuard, Weak};

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
