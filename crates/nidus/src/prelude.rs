//! Common imports for Nidus applications.

pub use nidus_core::{
    Application, Container, Factory, Inject, Lazy, Module, ModuleBuilder, ModuleDefinition,
    ModuleGraph, Nidus, NidusError, Optional, Provider, ProviderEntry, ProviderLifetime, Result,
    Scoped,
};
pub use nidus_macros::{
    controller, delete, get, guard, injectable, module, openapi, patch, pipe, post, put, routes,
    validate,
};

#[cfg(feature = "http")]
pub use nidus_http::{
    controller::Controller,
    error::{HttpError, RoutePathError},
    router::{RouteDefinition, RouteMetadata},
    server::{ApplicationHttpExt, HttpApplication},
};

#[cfg(feature = "auth")]
pub use nidus_auth::{Guard, GuardContext, GuardError};
#[cfg(feature = "config")]
pub use nidus_config::Config;
#[cfg(feature = "events")]
pub use nidus_events::{EventBus, EventSubscriber};
#[cfg(feature = "jobs")]
pub use nidus_jobs::{Job, JobError, JobFailure, JobQueue, JobReport};
#[cfg(feature = "openapi")]
pub use nidus_openapi::{OpenApiDocument, OpenApiRoute};
#[cfg(feature = "testing")]
pub use nidus_testing::{TestApp, TestAppBuilder, TestRequest, TestResponse};
#[cfg(feature = "validation")]
pub use nidus_validation::{FieldValidationError, ValidationPipe, ValidationPipeError};
#[cfg(feature = "sqlx-postgres")]
pub use sqlx::{PgPool, postgres::PgPoolOptions};
