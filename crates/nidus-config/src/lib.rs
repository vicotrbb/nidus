//! Typed configuration support.

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

/// Typed configuration document assembled from explicit sources.
#[derive(Clone, Debug, Default)]
pub struct Config {
    values: Map<String, Value>,
}

impl Config {
    /// Creates an empty configuration document.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates configuration from key/value pairs.
    pub fn from_pairs<K, V>(pairs: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<String>,
        V: AsRef<str>,
    {
        let values = pairs
            .into_iter()
            .map(|(key, value)| (key.into(), parse_scalar(value.as_ref())))
            .collect();
        Self { values }
    }

    /// Inserts a raw JSON configuration value.
    pub fn insert_value(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
    }

    /// Deserializes the configuration into a strongly typed settings struct.
    pub fn deserialize<T>(&self) -> Result<T>
    where
        T: DeserializeOwned,
    {
        serde_json::from_value(Value::Object(self.values.clone())).map_err(ConfigError::Deserialize)
    }
}

/// Result type for configuration operations.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Errors emitted by typed configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Deserialization into the requested type failed.
    #[error("configuration deserialize error: {0}")]
    Deserialize(#[from] serde_json::Error),
}

fn parse_scalar(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => value
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| value.parse::<f64>().map(Value::from))
            .unwrap_or_else(|_| Value::String(value.to_owned())),
    }
}
