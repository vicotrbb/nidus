use anyhow::{Context, Result, bail};

#[derive(Debug)]
pub(crate) struct OpenApiMetadata {
    pub(crate) summary: String,
    pub(crate) tags: Vec<String>,
    pub(crate) response_status: Option<u16>,
    pub(crate) request_schema: Option<String>,
    pub(crate) response_schema: Option<String>,
}

pub(crate) fn parse_openapi_args(args: &str) -> Result<OpenApiMetadata> {
    validate_openapi_args(args)?;
    let Some(summary) = extract_openapi_summary(args)? else {
        bail!("#[openapi] requires summary = \"...\" metadata");
    };
    Ok(OpenApiMetadata {
        summary,
        tags: extract_openapi_tags(args)?,
        response_status: extract_openapi_status(args)?,
        request_schema: extract_openapi_schema(args, "request")?,
        response_schema: extract_openapi_schema(args, "response")?,
    })
}

fn extract_openapi_summary(args: &str) -> Result<Option<String>> {
    for arg in split_openapi_args(args) {
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key.trim() != "summary" {
            continue;
        }
        let value = value.trim();
        let Some(value) = value
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
        else {
            bail!("#[openapi] summary must be a string literal");
        };
        return Ok(Some(value.to_owned()));
    }
    Ok(None)
}

fn validate_openapi_args(args: &str) -> Result<()> {
    for arg in split_openapi_args(args) {
        let Some((key, _value)) = arg.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if !matches!(key, "summary" | "tags" | "status" | "request" | "response") {
            bail!(
                "#[openapi] supports only summary = \"...\", tags = [\"...\"], status = 201, request = Type, and response = Type metadata"
            );
        }
    }
    Ok(())
}

fn split_openapi_args(args: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut bracket_depth = 0usize;
    let mut in_string = false;
    let mut previous_was_escape = false;

    for (index, character) in args.char_indices() {
        if in_string {
            if character == '"' && !previous_was_escape {
                in_string = false;
            }
            previous_was_escape = character == '\\' && !previous_was_escape;
            continue;
        }

        previous_was_escape = false;
        match character {
            '"' => in_string = true,
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            ',' if bracket_depth == 0 => {
                parts.push(args[start..index].trim());
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(args[start..].trim());
    parts
}

fn extract_openapi_tags(args: &str) -> Result<Vec<String>> {
    for arg in split_openapi_args(args) {
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key.trim() != "tags" {
            continue;
        }
        let value = value.trim();
        let Some(tags) = value
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        else {
            bail!("#[openapi] tags must be an array of string literals");
        };
        let mut values = Vec::new();
        for raw in tags.split(',') {
            let raw = raw.trim();
            if raw.is_empty() {
                continue;
            }
            let Some(value) = raw
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
            else {
                bail!("#[openapi] tags must be string literals");
            };
            values.push(value.to_owned());
        }
        return Ok(values);
    }
    Ok(Vec::new())
}

fn extract_openapi_status(args: &str) -> Result<Option<u16>> {
    for arg in split_openapi_args(args) {
        let Some((key, value)) = arg.split_once('=') else {
            continue;
        };
        if key.trim() != "status" {
            continue;
        }
        let value = value.trim();
        let status = value
            .parse::<u16>()
            .with_context(|| "#[openapi] status must be an HTTP status code integer literal")?;
        if !(100..=599).contains(&status) {
            bail!("#[openapi] status must be in the HTTP status code range 100..=599");
        }
        return Ok(Some(status));
    }
    Ok(None)
}

fn extract_openapi_schema(args: &str, key: &str) -> Result<Option<String>> {
    for arg in split_openapi_args(args) {
        let Some((name, value)) = arg.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        if value.starts_with('"') || value.is_empty() {
            bail!("#[openapi] {key} must be a type path");
        }
        if !is_type_path(value) {
            bail!("#[openapi] {key} must be a type path");
        }
        let schema = value
            .split("::")
            .last()
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .ok_or_else(|| anyhow::anyhow!("#[openapi] {key} must be a type path"))?;
        return Ok(Some(schema.to_owned()));
    }
    Ok(None)
}

fn is_type_path(value: &str) -> bool {
    value.split("::").all(is_type_segment)
}

fn is_type_segment(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}
