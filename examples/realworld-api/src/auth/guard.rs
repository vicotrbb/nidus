use async_trait::async_trait;
use nidus::prelude::{Guard, GuardContext, GuardError, Inject, injectable};

use crate::auth::AuthService;

#[injectable]
#[derive(Clone, Debug)]
pub struct ApiKeyGuard {
    auth: Inject<AuthService>,
}

#[async_trait]
impl Guard<()> for ApiKeyGuard {
    async fn check(&self, ctx: GuardContext<()>) -> Result<(), GuardError> {
        let Some(api_key) = ctx
            .headers()
            .get("x-api-key")
            .and_then(|value| value.to_str().ok())
        else {
            return Err(GuardError::unauthorized("missing or invalid x-api-key"));
        };

        if self.auth.is_valid_api_key(api_key) {
            Ok(())
        } else {
            Err(GuardError::unauthorized("missing or invalid x-api-key"))
        }
    }
}

#[cfg(test)]
pub fn unauthorized_status() -> axum::http::StatusCode {
    use axum::http::StatusCode;

    StatusCode::UNAUTHORIZED
}
