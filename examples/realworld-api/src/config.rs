use std::env;

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub api_key: String,
    pub bind_addr: String,
    pub database_url: String,
}

impl AppConfig {
    pub fn from_env() -> Self {
        Self {
            api_key: env::var("NIDUS_API_KEY").unwrap_or_else(|_| "dev-secret".to_owned()),
            bind_addr: env::var("NIDUS_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned()),
            database_url: env::var("NIDUS_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_owned()),
        }
    }

    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            api_key: "dev-secret".to_owned(),
            bind_addr: "127.0.0.1:0".to_owned(),
            database_url: "sqlite::memory:".to_owned(),
        }
    }
}
