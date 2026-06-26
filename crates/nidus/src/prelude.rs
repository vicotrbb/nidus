//! Common imports for Nidus applications.

pub use crate::{NidusApplicationBuilder, NidusApplicationExt};

pub use nidus_core::{
    Application, AsyncProviderInitializer, Container, ControllerDescriptor, ControllerRegistrant,
    Factory, Inject, Lazy, Module, ModuleBuilder, ModuleDefinition, ModuleDefinitionFactory,
    ModuleGraph, Nidus, NidusError, Optional, Provider, ProviderEntry, ProviderLifetime,
    ProviderRegistrant, ProviderRegistrar, RequestScope, Result, Scoped, SharedRequestScope,
};
pub use nidus_macros::{
    controller, delete, get, guard, injectable, module, openapi, patch, pipe, post, put, routes,
    validate,
};

#[cfg(feature = "http")]
pub use nidus_http::{
    HeaderMap, IntoResponse, Json, Path, Query, Response, Router, State, StatusCode,
    context::{
        ClientKind, IdentityExtractor, RequestContext, RequestIdentity, api_key_identity,
        client_ip_identity, context_identity,
    },
    controller::Controller,
    error::{ErrorEnvelopeLayer, ErrorEnvelopeService, HttpError, RoutePathError},
    health::{HealthRegistry, HealthState, HealthStatus},
    logging::{LoggingConfig, LoggingFormat, StructuredMakeSpan},
    middleware::{
        ApiDefaults, BodyLimitLayer, BodyLimitService, HttpMetricsHook, InMemoryRateLimitStore,
        MetricsLayer, MetricsService, PrometheusMetrics, RateLimitConfig, RateLimitDecision,
        RateLimitError, RateLimitLayer, RateLimitService, RateLimitStore, RequestContextLayer,
        RequestContextService, RequestIdConfig, RequestIdLayer, RequestIdMode, RequestIdPolicy,
        RequestIdService, RequestScopeLayer, RequestScopeService, RouteMakeSpan,
        SecurityHeadersLayer, SecurityHeadersService, TimeoutResponseLayer, TimeoutResponseService,
        ValidatedRequestIdLayer, ValidatedRequestIdService, body_limit_layer, compression_layer,
        cors_layer, cors_origin_layer, metrics_layer, rate_limit_layer, request_context_layer,
        request_id_layer, request_scope_layer, route_metrics_layer, route_trace_layer,
        security_headers_layer, timeout_layer, timeout_response_layer, trace_layer,
        validated_request_id_layer, webhook_body_limit_layer,
    },
    request::{RequestScopeRejection, RequestScoped},
    router::{RouteDefinition, RouteMetadata},
    server::{ApplicationHttpExt, HttpApplication},
};

#[cfg(feature = "otel")]
pub use nidus_http::otel::{
    OtelConfig, OtelShutdown, TraceContext, extract_trace_context, inject_trace_context,
    record_exception, shutdown_otel, with_observed_span,
};

#[cfg(feature = "auth")]
pub use nidus_auth::{
    AndGuard, Guard, GuardContext, GuardError, GuardExt, GuardLayer, GuardService, OrGuard,
    guard_layer,
};
#[cfg(feature = "config")]
pub use nidus_config::Config;
#[cfg(feature = "events")]
pub use nidus_events::{
    EventBus, EventObserver, EventSubscriber, ObservedEventBus, ObservedEventContext,
};
#[cfg(feature = "jobs")]
pub use nidus_jobs::{
    AsyncJob, AsyncJobQueue, Job, JobError, JobFailure, JobObserver, JobQueue, JobReport,
    JobResultStatus, ObservedJobContext, ObservedJobRunner,
};
#[cfg(feature = "openapi")]
pub use nidus_openapi::{OpenApiDocument, OpenApiDocumentError, OpenApiRoute};
#[cfg(feature = "testing")]
pub use nidus_testing::{TestApp, TestAppBuilder, TestRequest, TestResponse};
#[cfg(feature = "validation")]
pub use nidus_validation::{
    FieldValidationError, Pipe, ValidatedJson, ValidatedJsonRejection, ValidationPipe,
    ValidationPipeError,
};
