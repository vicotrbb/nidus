use std::{collections::BTreeSet, path::Path};

use anyhow::Result;
use serde_json::{Value, json};

use crate::routes::{discover_routes, openapi_path_parameters};
use crate::schema::discover_schemas;

pub(crate) fn generate_openapi(root: &Path) -> Result<()> {
    let mut paths = serde_json::Map::new();
    let mut schema_names = BTreeSet::new();
    for route in discover_routes(root)? {
        let parameters = openapi_path_parameters(&route.path);
        let response_status = route.response_status.unwrap_or(200).to_string();
        let operation_id = operation_id(&route.method, &route.path);
        let entry = paths
            .entry(route.path)
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
        if let Value::Object(methods) = entry {
            let mut operation = serde_json::Map::from_iter([
                ("operationId".to_owned(), json!(operation_id)),
                (
                    "responses".to_owned(),
                    json!({
                        response_status.clone(): {
                            "description": "Success"
                        }
                    }),
                ),
            ]);
            if let Some(summary) = route.summary {
                operation.insert("summary".to_owned(), json!(summary));
            }
            if !route.tags.is_empty() {
                operation.insert("tags".to_owned(), json!(route.tags));
            }
            if !route.guards.is_empty() {
                operation.insert("x-nidus-guards".to_owned(), json!(route.guards));
            }
            if !route.pipes.is_empty() {
                operation.insert("x-nidus-pipes".to_owned(), json!(route.pipes));
            }
            if route.validates {
                operation.insert("x-nidus-validates".to_owned(), json!(true));
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
        let schemas = discover_schemas(root, &schema_names)?
            .into_iter()
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

fn operation_id(method: &str, path: &str) -> String {
    let mut parts = vec![method.to_owned()];
    for segment in path.split('/') {
        if segment.is_empty() {
            continue;
        }
        if let Some(name) = segment
            .strip_prefix('{')
            .and_then(|value| value.strip_suffix('}'))
        {
            parts.push("by".to_owned());
            parts.push(identifier_segment(name));
        } else {
            parts.push(identifier_segment(segment));
        }
    }
    if parts.len() == 1 {
        parts.push("root".to_owned());
    }
    parts.join("_")
}

fn identifier_segment(segment: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = true;
    for character in segment.chars() {
        if character.is_ascii_alphanumeric() {
            output.push(character.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            output.push('_');
            previous_was_separator = true;
        }
    }
    if output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "value".to_owned()
    } else {
        output
    }
}
