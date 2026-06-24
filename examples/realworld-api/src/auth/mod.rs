pub mod guard;

use nidus::prelude::{Inject, injectable};

use crate::config::AppConfig;

#[injectable]
#[derive(Debug)]
pub struct AuthService {
    config: Inject<AppConfig>,
}

impl AuthService {
    pub fn is_valid_api_key(&self, api_key: &str) -> bool {
        api_key == self.config.api_key
    }
}
