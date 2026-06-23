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

Environment variables can be loaded through an explicit prefix. Double
underscores create nested objects:

```rust
#[derive(serde::Deserialize)]
struct AppConfig {
    name: String,
    database: DatabaseConfig,
}

#[derive(serde::Deserialize)]
struct DatabaseConfig {
    url: String,
}

let config = Config::from_env_prefix("APP");
// APP_NAME=nidus
// APP_DATABASE__URL=postgres://localhost/nidus
let typed = config.deserialize::<AppConfig>()?;
```

Applications should keep configuration explicit and validate it during startup.
