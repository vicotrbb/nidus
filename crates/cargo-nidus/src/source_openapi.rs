use anyhow::{Result, bail};
use syn::{
    Expr, Lit, LitInt, MetaNameValue, PathArguments, Token, parse::Parser, punctuated::Punctuated,
};

#[derive(Debug)]
pub(crate) struct OpenApiMetadata {
    pub(crate) summary: String,
    pub(crate) tags: Vec<String>,
    pub(crate) response_status: Option<u16>,
    pub(crate) request_schema: Option<String>,
    pub(crate) response_schema: Option<String>,
}

pub(crate) fn parse_openapi_args(args: &str) -> Result<OpenApiMetadata> {
    let args = Punctuated::<MetaNameValue, Token![,]>::parse_terminated
        .parse_str(args)
        .map_err(|error| anyhow::anyhow!("{error}"))?;
    let mut summary = None;
    let mut tags = Vec::new();
    let mut response_status = None;
    let mut request_schema = None;
    let mut response_schema = None;

    for arg in args {
        if arg.path.is_ident("summary") {
            summary = Some(summary_literal(arg)?);
        } else if arg.path.is_ident("tags") {
            tags = tag_literals(arg)?;
        } else if arg.path.is_ident("request") {
            request_schema = Some(schema_name(&arg.value, "request")?);
        } else if arg.path.is_ident("response") {
            response_schema = Some(schema_name(&arg.value, "response")?);
        } else if arg.path.is_ident("status") {
            response_status = Some(response_status_code(&arg.value)?);
        } else {
            bail!(
                "#[openapi] supports only summary = \"...\", tags = [\"...\"], status = 201, request = Type, and response = Type metadata"
            );
        }
    }

    let Some(summary) = summary else {
        bail!("#[openapi] requires summary = \"...\" metadata");
    };
    Ok(OpenApiMetadata {
        summary,
        tags,
        response_status,
        request_schema,
        response_schema,
    })
}

fn summary_literal(arg: MetaNameValue) -> Result<String> {
    let Expr::Lit(expr_lit) = arg.value else {
        bail!("#[openapi] summary must be a string literal");
    };
    let Lit::Str(value) = expr_lit.lit else {
        bail!("#[openapi] summary must be a string literal");
    };
    Ok(value.value())
}

fn tag_literals(arg: MetaNameValue) -> Result<Vec<String>> {
    let Expr::Array(array) = arg.value else {
        bail!("#[openapi] tags must be an array of string literals");
    };

    let mut tags = Vec::new();
    for element in array.elems {
        let Expr::Lit(expr_lit) = element else {
            bail!("#[openapi] tags must be string literals");
        };
        let Lit::Str(tag) = expr_lit.lit else {
            bail!("#[openapi] tags must be string literals");
        };
        tags.push(tag.value());
    }
    Ok(tags)
}

fn schema_name(value: &Expr, name: &str) -> Result<String> {
    let Expr::Path(expr_path) = value else {
        bail!("#[openapi] {name} must be a type path");
    };
    let Some(segment) = expr_path.path.segments.last() else {
        bail!("#[openapi] {name} must be a type path");
    };
    if expr_path
        .path
        .segments
        .iter()
        .any(|segment| !matches!(segment.arguments, PathArguments::None))
    {
        bail!("#[openapi] {name} must be a type path");
    }
    Ok(segment.ident.to_string())
}

fn response_status_code(value: &Expr) -> Result<u16> {
    let Expr::Lit(expr_lit) = value else {
        bail!("#[openapi] status must be an HTTP status code integer literal");
    };
    let Lit::Int(status) = &expr_lit.lit else {
        bail!("#[openapi] status must be an HTTP status code integer literal");
    };
    parse_status_literal(status)
}

fn parse_status_literal(status: &LitInt) -> Result<u16> {
    let value = status.base10_parse::<u16>().map_err(|_| {
        anyhow::anyhow!("#[openapi] status must be an HTTP status code integer literal")
    })?;
    if !(100..=599).contains(&value) {
        bail!("#[openapi] status must be in the HTTP status code range 100..=599");
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::parse_openapi_args;

    #[test]
    fn openapi_parser_accepts_commas_inside_string_literals() {
        let metadata = parse_openapi_args(
            r#"summary = "Find, inspect, and return user", tags = ["users,read", "public"]"#,
        )
        .unwrap();

        assert_eq!(metadata.summary, "Find, inspect, and return user");
        assert_eq!(metadata.tags, ["users,read", "public"]);
    }

    #[test]
    fn openapi_parser_extracts_type_names_from_paths() {
        let metadata = parse_openapi_args(
            "summary = \"Create user\", request = crate::dto::CreateUserDto, response = api::UserDto",
        )
        .unwrap();

        assert_eq!(metadata.request_schema.as_deref(), Some("CreateUserDto"));
        assert_eq!(metadata.response_schema.as_deref(), Some("UserDto"));
    }
}
