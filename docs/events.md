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
let observed = ObservedEventBus::new(bus.clone(), EventMetrics)
    .context("request_id", "req-123");
observed.publish_named("user.created", UserCreated { user_id: 42 });
```

Use context attributes to propagate request IDs, tenant IDs, or job run IDs into
event publication metrics and spans.
