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
