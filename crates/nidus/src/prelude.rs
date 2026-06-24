//! Common imports for Nidus applications.

pub use nidus_core::{
    Application, Container, Factory, Inject, Lazy, Module, ModuleBuilder, ModuleDefinition,
    ModuleGraph, Nidus, NidusError, Optional, Provider, ProviderEntry, ProviderLifetime,
    RequestScope, Result, Scoped, SharedRequestScope,
};
pub use nidus_macros::{
    controller, delete, get, guard, injectable, module, openapi, patch, pipe, post, put, routes,
    validate,
};

#[cfg(feature = "http")]
pub use nidus_http::{
    HeaderMap, IntoResponse, Json, Path, Query, Response, State, StatusCode,
    controller::Controller,
    error::{HttpError, RoutePathError},
    middleware::{
        HttpMetricsHook, MetricsLayer, MetricsService, RequestIdLayer, RequestIdService,
        RequestScopeLayer, RequestScopeService, RouteMakeSpan, compression_layer, cors_layer,
        metrics_layer, rate_limit_layer, request_id_layer, request_scope_layer,
        route_metrics_layer, route_trace_layer, timeout_layer, trace_layer,
    },
    request::{RequestScopeRejection, RequestScoped},
    router::{RouteDefinition, RouteMetadata},
    server::{ApplicationHttpExt, HttpApplication},
};

#[cfg(feature = "auth")]
pub use nidus_auth::{
    AndGuard, Guard, GuardContext, GuardError, GuardExt, GuardLayer, GuardService, OrGuard,
    guard_layer,
};
#[cfg(feature = "config")]
pub use nidus_config::Config;
#[cfg(feature = "events")]
pub use nidus_events::{EventBus, EventSubscriber};
#[cfg(feature = "jobs")]
pub use nidus_jobs::{AsyncJob, AsyncJobQueue, Job, JobError, JobFailure, JobQueue, JobReport};
#[cfg(feature = "openapi")]
pub use nidus_openapi::{OpenApiDocument, OpenApiDocumentError, OpenApiRoute};
#[cfg(feature = "testing")]
pub use nidus_testing::{TestApp, TestAppBuilder, TestRequest, TestResponse};
#[cfg(feature = "validation")]
pub use nidus_validation::{
    FieldValidationError, Pipe, ValidatedJson, ValidatedJsonRejection, ValidationPipe,
    ValidationPipeError,
};
#[cfg(feature = "sqlx-postgres")]
pub use sqlx::{PgPool, postgres::PgPoolOptions};
