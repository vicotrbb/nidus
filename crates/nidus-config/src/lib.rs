#![deny(missing_docs)]

//! Typed configuration support.

mod error;
mod value;

use std::{fs, path::Path};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use error::deserialize_value;
pub use error::{ConfigError, Result};
use value::{insert_path, merge_maps, parse_scalar, prefixed_key_start};

/// Typed configuration document assembled from explicit sources.
///
/// Sources are explicit and merge in the order you choose. Later sources
/// override earlier values, while nested objects merge recursively.
///
/// ```
/// use nidus_config::Config;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Debug, PartialEq, Eq)]
/// struct Settings {
///     port: u16,
///     database: DatabaseSettings,
/// }
///
/// #[derive(Deserialize, Debug, PartialEq, Eq)]
/// struct DatabaseSettings {
///     url: String,
///     pool_size: u32,
/// }
///
/// let file = Config::from_json_str(r#"{
///     "port": 3000,
///     "database": { "url": "postgres://localhost/app", "pool_size": 5 }
/// }"#)?;
///
/// let env = Config::from_prefixed_vars("APP", [
///     ("APP_PORT", "8080"),
///     ("APP_DATABASE__POOL_SIZE", "10"),
/// ]);
///
/// let settings: Settings = file.merge(env).deserialize()?;
/// assert_eq!(settings.port, 8080);
/// assert_eq!(settings.database.url, "postgres://localhost/app");
/// assert_eq!(settings.database.pool_size, 10);
/// # Ok::<(), nidus_config::ConfigError>(())
/// ```
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
    ///
    /// Values are parsed as JSON-like scalars, so strings such as `"true"`,
    /// `"42"`, and `"null"` become boolean, number, and null values.
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

    /// Creates configuration from a JSON object value.
    pub fn from_value(value: Value) -> Result<Self> {
        match value {
            Value::Object(values) => Ok(Self { values }),
            _ => Err(ConfigError::RootNotObject),
        }
    }

    /// Creates configuration from a JSON object string.
    pub fn from_json_str(source: &str) -> Result<Self> {
        let value = serde_json::from_str(source).map_err(ConfigError::Parse)?;
        Self::from_value(value)
    }

    /// Creates configuration from a JSON object file.
    pub fn from_json_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let label = path.display().to_string();
        let source = fs::read_to_string(path).map_err(|source| ConfigError::ReadFile {
            path: label.clone(),
            source,
        })?;
        let value = serde_json::from_str(&source).map_err(|source| ConfigError::ParseFile {
            path: label.clone(),
            source,
        })?;
        match value {
            Value::Object(values) => Ok(Self { values }),
            _ => Err(ConfigError::FileRootNotObject { path: label }),
        }
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
    /// normalized to lowercase after the prefix is removed. Double underscores
    /// create nested paths; empty path segments are ignored.
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
            .map(|value| deserialize_value(key.to_owned(), value))
            .transpose()
    }

    /// Deserializes a required top-level configuration value into a typed value.
    pub fn get_required_typed<T>(&self, key: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.get_typed(key)?
            .ok_or_else(|| ConfigError::MissingValue {
                path: key.to_owned(),
            })
    }

    /// Returns a nested raw configuration value by path.
    ///
    /// Object segments match keys. Array segments are zero-based numeric
    /// indexes such as `"0"`.
    pub fn get_path<I, S>(&self, path: I) -> Option<&Value>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut path = path.into_iter();
        let first = path.next()?;
        let mut value = self.values.get(first.as_ref())?;

        for segment in path {
            let segment = segment.as_ref();
            value = match value {
                Value::Object(object) => object.get(segment)?,
                Value::Array(array) => array.get(segment.parse::<usize>().ok()?)?,
                _ => return None,
            };
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
            .map(|value| deserialize_value(label, value))
            .transpose()
    }

    /// Deserializes a required nested configuration value into a typed value.
    pub fn get_required_path_typed<I, S, T>(&self, path: I) -> Result<T>
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
        match self.get_path(path.iter().map(String::as_str)) {
            Some(value) => deserialize_value(label, value),
            None => Err(ConfigError::MissingValue { path: label }),
        }
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
        T::deserialize(&self.values).map_err(ConfigError::Deserialize)
    }
}
