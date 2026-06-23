//! Public facade crate for the Nidus framework.

pub mod prelude;
pub mod runtime {
    //! Tokio runtime types used by Nidus application entrypoint macros.

    pub use tokio::runtime::{Builder, Runtime};
}

pub use nidus_core::*;
pub use nidus_macros::*;

#[cfg(feature = "http")]
pub use nidus_http as http;
