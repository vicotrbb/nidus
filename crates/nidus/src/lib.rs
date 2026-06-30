#![deny(missing_docs)]

//! Public facade crate for the Nidus framework.

pub mod app;
pub mod prelude;
pub mod runtime {
    //! Tokio runtime types used by Nidus application entrypoint macros.

    pub use tokio::runtime::{Builder, Runtime};
}

pub use nidus_core::*;
pub use nidus_macros::*;

pub use app::{NidusApplicationBuilder, NidusApplicationExt};

/// Registers an OpenAPI schema and nested schemas into the provided schema registry.
#[doc(hidden)]
pub fn register_openapi_schema<T>(
    schemas: &mut Vec<(String, serde_json::Value)>,
) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    T: utoipa::ToSchema,
{
    let mut openapi_schemas: Vec<(
        String,
        utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
    )> = vec![(
        T::name().to_string(),
        <T as utoipa::PartialSchema>::schema(),
    )];
    <T as utoipa::ToSchema>::schemas(&mut openapi_schemas);

    for (name, schema) in openapi_schemas {
        schemas.push((name, serde_json::to_value(schema)?));
    }
    Ok(())
}

#[cfg(feature = "auth")]
pub use nidus_auth as auth;
#[cfg(feature = "config")]
pub use nidus_config as config;
#[cfg(feature = "dashboard")]
pub use nidus_dashboard as dashboard;
#[cfg(feature = "events")]
pub use nidus_events as events;
#[cfg(feature = "http")]
pub use nidus_http as http;
#[cfg(feature = "jobs")]
pub use nidus_jobs as jobs;
#[cfg(feature = "observability")]
pub use nidus_observability as observability;
#[cfg(feature = "openapi")]
pub use nidus_openapi as openapi;
#[cfg(feature = "testing")]
pub use nidus_testing as testing;
#[cfg(feature = "validation")]
pub use nidus_validation as validation;
