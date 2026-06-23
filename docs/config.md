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

JSON object sources can be loaded explicitly:

```rust
let config = Config::from_json_str(
    r#"{
        "name": "nidus",
        "port": 3000
    }"#,
)?;
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

Multiple sources can be layered with later values taking precedence:

```rust
let defaults = Config::from_pairs([("port", "3000")]);
let env = Config::from_env_prefix("APP");
let config = defaults.merge(env);
```

Raw values can be inspected before or after typed deserialization when tests or
startup checks need targeted assertions:

```rust
let database_url = config
    .get_path(["database", "url"])
    .and_then(serde_json::Value::as_str);
```

Individual values can also be deserialized with path-aware errors:

```rust
let port = config.get_typed::<u16>("port")?;
let database_url: Option<String> = config.get_path_typed(["database", "url"])?;
```

Applications should keep configuration explicit and validate it during startup.
