use serde_json::{Map, Value};

pub(crate) fn parse_scalar(value: &str) -> Value {
    match value {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => value
            .parse::<i64>()
            .map(Value::from)
            .or_else(|_| value.parse::<f64>().map(Value::from))
            .unwrap_or_else(|_| Value::String(value.to_owned())),
    }
}

pub(crate) fn prefixed_key_start(prefix: &str) -> String {
    if prefix.is_empty() || prefix.ends_with('_') {
        prefix.to_owned()
    } else {
        format!("{prefix}_")
    }
}

pub(crate) fn insert_path(values: &mut Map<String, Value>, path: &[String], value: Value) {
    if let Some((head, tail)) = path.split_first() {
        if tail.is_empty() {
            values.insert(head.clone(), value);
        } else {
            let child = values
                .entry(head.clone())
                .or_insert_with(|| Value::Object(Map::new()));
            if !child.is_object() {
                *child = Value::Object(Map::new());
            }
            if let Value::Object(child_values) = child {
                insert_path(child_values, tail, value);
            }
        }
    }
}

pub(crate) fn merge_maps(target: &mut Map<String, Value>, source: Map<String, Value>) {
    for (key, source_value) in source {
        match (target.get_mut(&key), source_value) {
            (Some(Value::Object(target_child)), Value::Object(source_child)) => {
                merge_maps(target_child, source_child);
            }
            (_, source_value) => {
                target.insert(key, source_value);
            }
        }
    }
}
