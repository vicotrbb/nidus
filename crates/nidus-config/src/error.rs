use serde::de::DeserializeOwned;
use serde_json::Value;

/// Result type for configuration operations.
pub type Result<T> = std::result::Result<T, ConfigError>;

/// Errors emitted by typed configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Parsing a JSON configuration source failed.
    #[error("configuration JSON parse error: {0}")]
    Parse(#[source] serde_json::Error),

    /// The configuration root was not a JSON object.
    #[error("configuration root must be a JSON object")]
    RootNotObject,

    /// Reading a JSON configuration file failed.
    #[error("configuration file `{path}` read error: {source}")]
    ReadFile {
        /// Configuration file path.
        path: String,
        /// Underlying IO error.
        #[source]
        source: std::io::Error,
    },

    /// Parsing a JSON configuration file failed.
    #[error("configuration file `{path}` JSON parse error: {source}")]
    ParseFile {
        /// Configuration file path.
        path: String,
        /// Underlying serde error.
        #[source]
        source: serde_json::Error,
    },

    /// A JSON configuration file root was not an object.
    #[error("configuration file `{path}` root must be a JSON object")]
    FileRootNotObject {
        /// Configuration file path.
        path: String,
    },

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

    /// A required configuration value was missing.
    #[error("missing required configuration value `{path}`")]
    MissingValue {
        /// Missing configuration key or dot-separated path.
        path: String,
    },
}

pub(crate) fn deserialize_value<T>(path: String, value: Value) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value(value).map_err(|source| ConfigError::ValueDeserialize { path, source })
}
