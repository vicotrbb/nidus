//! OpenAPI document generation and serving support.

mod document;
mod html;
mod path;
mod route;

pub use document::{OpenApiDocument, OpenApiDocumentError};
pub use route::OpenApiRoute;
