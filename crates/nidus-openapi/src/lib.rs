//! OpenAPI document generation and serving support.

use serde_json::{Value, json};

/// Minimal OpenAPI document metadata builder.
#[derive(Clone, Debug)]
pub struct OpenApiDocument {
    title: String,
    version: String,
    routes: Vec<OpenApiRoute>,
}

impl OpenApiDocument {
    /// Creates an OpenAPI document.
    pub fn new(title: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            version: version.into(),
            routes: Vec::new(),
        }
    }

    /// Adds route metadata to the document.
    pub fn route(mut self, route: OpenApiRoute) -> Self {
        self.routes.push(route);
        self
    }

    /// Renders the document as JSON.
    pub fn to_json_value(&self) -> Value {
        let mut paths = serde_json::Map::new();
        for route in &self.routes {
            let entry = paths
                .entry(route.path.clone())
                .or_insert_with(|| Value::Object(serde_json::Map::new()));
            if let Value::Object(methods) = entry {
                methods.insert(
                    route.method.clone(),
                    json!({
                        "summary": route.summary,
                        "responses": {
                            "200": {
                                "description": "Success"
                            }
                        }
                    }),
                );
            }
        }

        json!({
            "openapi": "3.1.0",
            "info": {
                "title": self.title,
                "version": self.version,
            },
            "paths": paths,
        })
    }
}

/// OpenAPI route metadata.
#[derive(Clone, Debug)]
pub struct OpenApiRoute {
    method: String,
    path: String,
    summary: Option<String>,
}

impl OpenApiRoute {
    /// Creates GET route metadata.
    pub fn get(path: impl Into<String>) -> Self {
        Self::new("get", path)
    }

    /// Creates POST route metadata.
    pub fn post(path: impl Into<String>) -> Self {
        Self::new("post", path)
    }

    /// Sets the route summary.
    pub fn summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            summary: None,
        }
    }
}
