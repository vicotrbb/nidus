# Config

Typed configuration is deserialized into user structs.

```rust
#[derive(serde::Deserialize)]
struct AppConfig {
    name: String,
    port: u16,
}

let config = Config::from_pairs([("name", "nidus"), ("port", "3000")]);
let typed = config.deserialize::<AppConfig>()?;
```

Applications should keep configuration explicit and validate it during startup.

