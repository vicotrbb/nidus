#![deny(missing_docs)]

//! Embedded dashboard for Nidus applications.
//!
//! `nidus-dashboard` serves a protected dashboard UI, JSON APIs, and live
//! introspection stream from the same Axum application as the user's service.

mod auth;
mod collector;
mod config;
mod error;
mod router;
pub mod storage;
mod types;

pub use config::{DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage};
pub use error::{DashboardError, Result};
pub use router::NidusDashboard;
pub use types::{
    DashboardOperation, DashboardOperationKind, DashboardOperationStatus, DashboardRouteSnapshot,
};
