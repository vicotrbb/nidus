use std::{collections::BTreeSet, path::Path};

use anyhow::Result;
use serde_json::{Value, json};

use crate::routes::{discover_routes, openapi_path_parameters};

pub(crate) fn generate_openapi(root: &Path) -> Result<()> {
    let mut paths = serde_json::Map::new();
    let mut schema_names = BTreeSet::new();
    for route in discover_routes(root)? {
        let parameters = openapi_path_parameters(&route.path);
        let response_status = route.response_status.unwrap_or(200).to_string();
        let entry = paths
            .entry(route.path)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(methods) = entry {
            let mut operation = serde_json::Map::from_iter([(
                "responses".to_owned(),
                json!({
                    response_status.clone(): {
                        "description": "Success"
                    }
                }),
            )]);
            if let Some(summary) = route.summary {
                operation.insert("summary".to_owned(), json!(summary));
            }
            if !route.tags.is_empty() {
                operation.insert("tags".to_owned(), json!(route.tags));
            }
            if let Some(schema) = route.request_schema {
                schema_names.insert(schema.clone());
                operation.insert(
                    "requestBody".to_owned(),
                    json!({
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": schema_ref(&schema)
                            }
                        }
                    }),
                );
            }
            if let Some(schema) = route.response_schema {
                schema_names.insert(schema.clone());
                operation.insert(
                    "responses".to_owned(),
                    json!({
                        response_status: {
                            "description": "Success",
                            "content": {
                                "application/json": {
                                    "schema": schema_ref(&schema)
                                }
                            }
                        }
                    }),
                );
            }
            if !parameters.is_empty() {
                operation.insert(
                    "parameters".to_owned(),
                    json!(
                        parameters
                            .into_iter()
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
                    ),
                );
            }
            methods.insert(route.method, Value::Object(operation));
        }
    }

    let mut document = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Nidus API",
            "version": "0.1.0",
        },
        "paths": paths,
    });

    if !schema_names.is_empty() {
        let schemas = schema_names
            .into_iter()
            .map(|name| {
                (
                    name,
                    json!({
                        "type": "object"
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        document["components"] = json!({
            "schemas": schemas,
        });
    }

    println!("{}", document);
    Ok(())
}

fn schema_ref(schema: &str) -> Value {
    json!({
        "$ref": format!("#/components/schemas/{schema}")
    })
}
