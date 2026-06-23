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

    /// Creates configuration from process environment variables with a prefix.
    ///
    /// For prefix `APP`, `APP_PORT=3000` maps to `port`, and
    /// `APP_DATABASE__URL=...` maps to `database.url`.
    pub fn from_env_prefix(prefix: &str) -> Self {
        Self::from_prefixed_vars(prefix, std::env::vars())
    }

    /// Creates configuration from prefixed key/value variables.
    ///
    /// This is useful for deterministic tests and for loading from custom
    /// environment sources. Prefix matching is case-sensitive; keys are
    /// normalized to lowercase after the prefix is removed.
    pub fn from_prefixed_vars<K, V>(prefix: &str, vars: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let prefix = prefixed_key_start(prefix);
        let mut config = Self::new();

        for (key, value) in vars {
            let Some(raw_key) = key.as_ref().strip_prefix(&prefix) else {
                continue;
            };
            if raw_key.is_empty() {
                continue;
            }

            let path = raw_key
                .split("__")
                .filter(|segment| !segment.is_empty())
                .map(|segment| segment.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if !path.is_empty() {
                insert_path(&mut config.values, &path, parse_scalar(value.as_ref()));
            }
        }

        config
    }

    /// Inserts a raw JSON configuration value.
    pub fn insert_value(&mut self, key: impl Into<String>, value: Value) {
        self.values.insert(key.into(), value);
    }

    /// Returns a top-level raw configuration value.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// Deserializes a top-level configuration value into a typed value.
    pub fn get_typed<T>(&self, key: &str) -> Result<Option<T>>
    where
        T: DeserializeOwned,
    {
        self.get(key)
            .cloned()
            .map(|value| deserialize_value(key.to_owned(), value))
            .transpose()
    }

    /// Returns a nested raw configuration value by path.
    pub fn get_path<I, S>(&self, path: I) -> Option<&Value>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut path = path.into_iter();
        let first = path.next()?;
        let mut value = self.values.get(first.as_ref())?;

        for segment in path {
            value = value.as_object()?.get(segment.as_ref())?;
        }

        Some(value)
    }

    /// Deserializes a nested configuration value into a typed value.
    pub fn get_path_typed<I, S, T>(&self, path: I) -> Result<Option<T>>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
        T: DeserializeOwned,
    {
        let path = path
            .into_iter()
            .map(|segment| segment.as_ref().to_owned())
            .collect::<Vec<_>>();
        let label = path.join(".");
        self.get_path(path.iter().map(String::as_str))
            .cloned()
            .map(|value| deserialize_value(label, value))
            .transpose()
    }

    /// Merges another configuration source into this one.
    ///
    /// Values from `other` take precedence. Nested objects are merged
    /// recursively so later sources can override one field without replacing an
    /// entire nested configuration section.
    pub fn merge(mut self, other: Self) -> Self {
        self.merge_from(other);
        self
    }

    /// Merges another configuration source into this configuration in place.
    pub fn merge_from(&mut self, other: Self) {
        merge_maps(&mut self.values, other.values);
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

    /// Deserialization of one configuration value failed.
    #[error("configuration deserialize error at `{path}`: {source}")]
    ValueDeserialize {
        /// Configuration key or dot-separated path that failed.
        path: String,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },
}

fn deserialize_value<T>(path: String, value: Value) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|source| ConfigError::ValueDeserialize { path, source })
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

fn prefixed_key_start(prefix: &str) -> String {
    if prefix.is_empty() || prefix.ends_with('_') {
        prefix.to_owned()
    } else {
        format!("{prefix}_")
    }
}

fn insert_path(values: &mut Map<String, Value>, path: &[String], value: Value) {
    if let Some((head, tail)) = path.split_first() {
        if tail.is_empty() {
            values.insert(head.clone(), value);
        } else {
            let child = values
                .entry(head.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if !child.is_object() {
                *child = Value::Object(Map::new());
            }
            if let Value::Object(child_values) = child {
                insert_path(child_values, tail, value);
            }
        }
    }
}

fn merge_maps(target: &mut Map<String, Value>, source: Map<String, Value>) {
    for (key, source_value) in source {
        match (target.get_mut(&key), source_value) {
            (Some(Value::Object(target_child)), Value::Object(source_child)) => {
                merge_maps(target_child, source_child);
            }
            (_, source_value) => {
                target.insert(key, source_value);
            }
        }
    }
}
