# Events

`nidus-events` provides an in-process typed event bus for lightweight
application events.

```rust
let bus = EventBus::<UserCreated>::new();
let subscriber = bus.subscribe();

bus.publish(UserCreated { user_id: 42 });

let events = subscriber.drain();
```

Subscribers receive events published after they subscribe. Dropping a subscriber
handle removes it from future delivery; the bus prunes dropped handles during
publish and subscriber counting.

If a subscriber list or queue mutex is poisoned by a panic, the bus logs a
warning and recovers the inner state so later subscribers and publishes can
continue.

## Observed Events

`ObservedEventBus` wraps an existing `EventBus` and records publication context
while preserving normal subscriber delivery.

```rust
#[derive(Clone)]
struct EventMetrics;

impl EventObserver<UserCreated> for EventMetrics {
    fn on_event_published(&self, context: &ObservedEventContext) {
        tracing::info!(
            event.name = context.event_name(),
            event.operation_id = context.operation_id()
        );
    }
}

let bus = EventBus::<UserCreated>::new();
let observed = bus.clone()
    .observed(EventMetrics)
    .context("request_id", "req-123");
observed.publish_named("user.created", UserCreated { user_id: 42 });
```

Use context attributes to propagate request IDs, tenant IDs, or job run IDs into
event publication metrics and spans.

For the recommended production path, pass `Observability::event_observer()`:

```rust
let observability = Observability::production("users-api").prometheus();
let observed = observability.observed_event_bus::<UserCreated>();
observed.publish_named("user.created", UserCreated { user_id: 42 });
```

Only publications through `ObservedEventBus` are instrumented. Plain
`EventBus::publish` keeps its existing behavior and emits no metrics.

When observation needs slower export work, use a channel-backed observer and
process contexts away from the publish call:

```rust
let (observer, receiver) = event_observer_channel();
let observed = ObservedEventBus::new(bus.clone(), observer);

observed.publish_named("user.created", UserCreated { user_id: 42 });

let context = receiver.recv()?;
```

The publish path only sends the `ObservedEventContext`; a dropped receiver does
not fail event publication.
