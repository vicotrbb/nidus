//! Dashboard authentication.

use std::sync::Arc;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::{DashboardAuth, error::Result};

/// Runtime dashboard auth state.
#[derive(Clone, Debug)]
pub enum DashboardAuthState {
    /// Bearer token auth.
    Bearer {
        /// Required bearer token.
        token: Arc<str>,
    },
    /// Auth disabled explicitly for local development.
    UnsafeDisabled,
}

impl DashboardAuthState {
    /// Builds runtime auth state from config.
    pub fn from_config(auth: DashboardAuth) -> Result<Self> {
        match auth {
            DashboardAuth::BearerToken(token) => Ok(Self::Bearer {
                token: Arc::from(token),
            }),
            DashboardAuth::BearerFromEnv(name) => {
                let token = std::env::var(&name).unwrap_or_default();
                Ok(Self::Bearer {
                    token: Arc::from(token),
                })
            }
            DashboardAuth::UnsafeDisabledForLocalDevelopment => Ok(Self::UnsafeDisabled),
        }
    }

    fn allows(&self, headers: &HeaderMap) -> bool {
        match self {
            Self::UnsafeDisabled => true,
            Self::Bearer { token } => headers
                .get(http::header::AUTHORIZATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.strip_prefix("Bearer "))
                .is_some_and(|candidate| candidate == token.as_ref()),
        }
    }

    pub(crate) fn mode_name(&self) -> &'static str {
        match self {
            Self::Bearer { .. } => "bearer",
            Self::UnsafeDisabled => "unsafe_disabled_for_local_development",
        }
    }
}

/// Axum middleware that enforces dashboard authentication.
pub async fn require_dashboard_auth(
    State(auth): State<DashboardAuthState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    if auth.allows(&headers) {
        next.run(request).await
    } else {
        StatusCode::UNAUTHORIZED.into_response()
    }
}
