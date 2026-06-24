use nidus::prelude::Config;

#[derive(Clone, Debug, serde::Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_api_key")]
    pub api_key: String,
    #[serde(default = "default_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_database_url")]
    pub database_url: String,
    #[serde(default = "default_allowed_origin")]
    pub allowed_origin: String,
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default = "default_log_format")]
    pub log_format: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        let defaults = Config::from_pairs([
            ("api_key", default_api_key()),
            ("bind_addr", default_bind_addr()),
            ("database_url", default_database_url()),
            ("allowed_origin", default_allowed_origin()),
            ("environment", default_environment()),
            ("log_format", default_log_format()),
        ]);
        let env = Config::from_env_prefix("NIDUS");
        Self::from_nidus_config(
            defaults
                .merge(env)
                .deserialize()
                .expect("NIDUS_* environment config should deserialize"),
        )
    }

    pub fn from_nidus_config(config: Self) -> Self {
        config
    }

    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            api_key: default_api_key(),
            bind_addr: "127.0.0.1:0".to_owned(),
            database_url: default_database_url(),
            allowed_origin: default_allowed_origin(),
            environment: "test".to_owned(),
            log_format: "development".to_owned(),
        }
    }
}

fn default_api_key() -> String {
    "dev-secret".to_owned()
}

fn default_bind_addr() -> String {
    "127.0.0.1:3000".to_owned()
}

fn default_database_url() -> String {
    "sqlite::memory:".to_owned()
}

fn default_allowed_origin() -> String {
    "https://console.nidus.dev".to_owned()
}

fn default_environment() -> String {
    "local".to_owned()
}

fn default_log_format() -> String {
    "development".to_owned()
}
