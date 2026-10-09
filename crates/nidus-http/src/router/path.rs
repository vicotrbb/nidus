use crate::error::RoutePathError;

pub(crate) use nidus_core::route::join_normalized_paths;

pub(crate) fn join_paths(prefix: &str, path: &str) -> Result<String, RoutePathError> {
    nidus_core::route::join_paths(prefix, path).map_err(Into::into)
}

pub(crate) fn normalize_mount_prefix(prefix: impl AsRef<str>) -> Result<String, RoutePathError> {
    nidus_core::route::normalize_mount_prefix(prefix).map_err(Into::into)
}

pub(crate) fn normalize_path(path: impl AsRef<str>) -> Result<String, RoutePathError> {
    nidus_core::route::normalize_path(path).map_err(Into::into)
}
