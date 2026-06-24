use nidus_http::router::RouteMetadata;
use nidus_http::{StatusCode, error::RoutePathError};
use serde_json::{Value, json};
use utoipa::ToSchema;

/// OpenAPI route metadata.
#[derive(Clone, Debug)]
pub struct OpenApiRoute {
    method: String,
    path: String,
    path_parameters: Vec<String>,
    summary: Option<String>,
    tags: Vec<String>,
    response_status: StatusCode,
    request_schema: Option<String>,
    response_schema: Option<String>,
}

impl OpenApiRoute {
    /// Creates GET route metadata.
    pub fn get(path: impl Into<String>) -> Self {
        Self::try_get(path).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create GET route metadata.
    pub fn try_get(path: impl Into<String>) -> Result<Self, RoutePathError> {
        Self::try_new("get", path)
    }

    /// Creates POST route metadata.
    pub fn post(path: impl Into<String>) -> Self {
        Self::try_post(path).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create POST route metadata.
    pub fn try_post(path: impl Into<String>) -> Result<Self, RoutePathError> {
        Self::try_new("post", path)
    }

    /// Creates PUT route metadata.
    pub fn put(path: impl Into<String>) -> Self {
        Self::try_put(path).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create PUT route metadata.
    pub fn try_put(path: impl Into<String>) -> Result<Self, RoutePathError> {
        Self::try_new("put", path)
    }

    /// Creates PATCH route metadata.
    pub fn patch(path: impl Into<String>) -> Self {
        Self::try_patch(path).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create PATCH route metadata.
    pub fn try_patch(path: impl Into<String>) -> Result<Self, RoutePathError> {
        Self::try_new("patch", path)
    }

    /// Creates DELETE route metadata.
    pub fn delete(path: impl Into<String>) -> Self {
        Self::try_delete(path).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Tries to create DELETE route metadata.
    pub fn try_delete(path: impl Into<String>) -> Result<Self, RoutePathError> {
        Self::try_new("delete", path)
    }

    /// Sets the route summary.
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Adds an OpenAPI tag to this operation.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Sets the successful response status for this operation.
    pub fn response_status(mut self, status: StatusCode) -> Self {
        self.response_status = status;
        self
    }

    /// Sets the JSON request body schema reference.
    pub fn request_schema<T>(self) -> Self
    where
        T: ToSchema,
    {
        self.request_schema_ref(T::name())
    }

    /// Sets the successful JSON response schema reference.
    pub fn response_schema<T>(self) -> Self
    where
        T: ToSchema,
    {
        self.response_schema_ref(T::name())
    }

    pub(crate) fn method(&self) -> &str {
        &self.method
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn try_from_route_metadata(
        metadata: &RouteMetadata,
    ) -> Result<Self, RoutePathError> {
        Self::try_from_route_metadata_at_path(metadata, metadata.path())
    }

    pub(crate) fn try_from_route_metadata_at_path(
        metadata: &RouteMetadata,
        path: impl AsRef<str>,
    ) -> Result<Self, RoutePathError> {
        let path = openapi_path(path.as_ref())?;
        let path_parameters = openapi_path_parameters(&path);
        let mut route = Self::new(
            metadata.method().to_ascii_lowercase(),
            path,
            path_parameters,
        );
        if let Some(summary) = metadata.summary() {
            route = route.summary(summary);
        }
        for tag in metadata.tags() {
            route = route.tag(*tag);
        }
        if let Some(status) = metadata.response_status() {
            route = route.response_status(status);
        }
        if let Some(schema) = metadata.request_schema() {
            route = route.request_schema_ref(schema);
        }
        if let Some(schema) = metadata.response_schema() {
            route = route.response_schema_ref(schema);
        }
        Ok(route)
    }

    pub(crate) fn to_json_value(&self) -> Value {
        let mut success_response = json!({
            "description": "Success"
        });
        if let Some(schema) = &self.response_schema {
            success_response["content"] = json!({
                "application/json": {
                    "schema": {
                        "$ref": format!("#/components/schemas/{schema}")
                    }
                }
            });
        }

        let mut responses = serde_json::Map::new();
        responses.insert(self.response_status.as_u16().to_string(), success_response);

        let mut operation = json!({
            "responses": responses
        });

        if let Some(summary) = &self.summary {
            operation["summary"] = json!(summary);
        }
        if !self.tags.is_empty() {
            operation["tags"] = json!(self.tags);
        }
        if let Some(schema) = &self.request_schema {
            operation["requestBody"] = json!({
                "required": true,
                "content": {
                    "application/json": {
                        "schema": {
                            "$ref": format!("#/components/schemas/{schema}")
                        }
                    }
                }
            });
        }
        if !self.path_parameters.is_empty() {
            operation["parameters"] = json!(
                self.path_parameters
                    .iter()
                    .map(|name| {
                        json!({
                            "name": name,
                            "in": "path",
                            "required": true,
                            "schema": {
                                "type": "string"
                            }
                        })
                    })
                    .collect::<Vec<_>>()
            );
        }

        operation
    }

    fn request_schema_ref(mut self, schema: impl Into<String>) -> Self {
        self.request_schema = Some(schema.into());
        self
    }

    fn response_schema_ref(mut self, schema: impl Into<String>) -> Self {
        self.response_schema = Some(schema.into());
        self
    }

    fn new(
        method: impl Into<String>,
        path: impl Into<String>,
        path_parameters: Vec<String>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            path_parameters,
            summary: None,
            tags: Vec::new(),
            response_status: StatusCode::OK,
            request_schema: None,
            response_schema: None,
        }
    }

    fn try_new(method: impl Into<String>, path: impl Into<String>) -> Result<Self, RoutePathError> {
        let path = path.into();
        let path = openapi_path(&path)?;
        let path_parameters = openapi_path_parameters(&path);
        Ok(Self::new(method, path, path_parameters))
    }
}

fn openapi_path(path: &str) -> Result<String, RoutePathError> {
    let mut segments = Vec::new();
    for segment in path.split('/') {
        if segment == ":" {
            return Err(RoutePathError::empty_parameter(path));
        }
        if let Some(name) = segment.strip_prefix(':') {
            segments.push(format!("{{{name}}}"));
        } else {
            segments.push(segment.to_owned());
        }
    }
    Ok(segments.join("/"))
}

fn openapi_path_parameters(path: &str) -> Vec<String> {
    path.split('/')
        .filter_map(|segment| {
            let name = segment.strip_prefix('{')?.strip_suffix('}')?;
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}
