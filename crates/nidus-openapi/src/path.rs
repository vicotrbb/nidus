use nidus_http::error::RoutePathError;

pub(crate) use nidus_core::route::{openapi_path_parameters, operation_id};

pub(crate) fn openapi_path(path: &str) -> Result<String, RoutePathError> {
    nidus_core::route::convert_parameters(path).map_err(Into::into)
}
