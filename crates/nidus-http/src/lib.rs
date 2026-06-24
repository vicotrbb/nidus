#![deny(missing_docs)]

//! HTTP routing, controllers, middleware, request, and response helpers.

pub use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
pub use request::{RequestScopeRejection, RequestScoped};

pub mod context;
pub mod controller;
pub mod error;
pub mod health;
pub mod logging;
pub mod middleware;
#[cfg(feature = "otel")]
pub mod otel;
pub mod request;
pub mod response;
pub mod router;
pub mod server;
