#![deny(missing_docs)]

//! Validation pipe support.

use std::ops::{Deref, DerefMut};

use axum::{
    Json,
    extract::{FromRequest, Request, rejection::JsonRejection},
    response::IntoResponse,
};
use garde::Validate;
use http::StatusCode;
use serde::{Serialize, de::DeserializeOwned};

/// Typed request transformation or validation pipe.
pub trait Pipe<Input>: Send + Sync + 'static {
    /// Output produced by this pipe.
    type Output;

    /// Error emitted when transformation or validation fails.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Transforms or validates the input value.
    fn transform(&self, input: Input) -> std::result::Result<Self::Output, Self::Error>;
}

/// Request validation pipe backed by the `garde` crate.
#[derive(Clone, Debug, Default)]
pub struct ValidationPipe;

impl ValidationPipe {
    /// Creates a validation pipe.
    pub fn new() -> Self {
        Self
    }

    /// Validates and returns the input value unchanged when valid.
    pub fn transform<T>(&self, input: T) -> Result<T>
    where
        T: Validate<Context = ()>,
    {
        input.validate().map_err(ValidationPipeError::Validation)?;
        Ok(input)
    }
}

impl<T> Pipe<T> for ValidationPipe
where
    T: Validate<Context = ()>,
{
    type Output = T;
    type Error = ValidationPipeError;

    fn transform(&self, input: T) -> std::result::Result<Self::Output, Self::Error> {
        ValidationPipe::transform(self, input)
    }
}

/// Axum extractor that deserializes a JSON body and validates it with [`ValidationPipe`].
///
/// JSON parsing errors keep Axum's normal JSON rejection response. Values that
/// parse successfully but fail validation return Nidus's stable validation
/// error response.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedJson<T>(pub T);

impl<T> ValidatedJson<T> {
    /// Consumes the extractor and returns the validated value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> Deref for ValidatedJson<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for ValidatedJson<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate<Context = ()>,
{
    type Rejection = ValidatedJsonRejection;

    async fn from_request(req: Request, state: &S) -> std::result::Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(ValidatedJsonRejection::Json)?;
        let value = ValidationPipe::new()
            .transform(value)
            .map_err(ValidatedJsonRejection::Validation)?;
        Ok(Self(value))
    }
}

/// Rejection emitted by [`ValidatedJson`].
#[derive(Debug, thiserror::Error)]
pub enum ValidatedJsonRejection {
    /// The request body was not valid JSON for the target type.
    #[error(transparent)]
    Json(#[from] JsonRejection),
    /// The parsed JSON failed validation.
    #[error(transparent)]
    Validation(#[from] ValidationPipeError),
}

impl IntoResponse for ValidatedJsonRejection {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::Json(error) => error.into_response(),
            Self::Validation(error) => error.into_response(),
        }
    }
}

/// Result type for validation pipes.
pub type Result<T> = std::result::Result<T, ValidationPipeError>;

/// Errors emitted by validation pipes.
#[derive(Debug, thiserror::Error)]
pub enum ValidationPipeError {
    /// The input failed validation.
    #[error("validation failed: {0}")]
    Validation(#[from] garde::Report),
}

impl ValidationPipeError {
    /// Returns the HTTP status code corresponding to this validation failure.
    pub fn status_code(&self) -> StatusCode {
        StatusCode::UNPROCESSABLE_ENTITY
    }

    /// Returns the stable machine-readable error code.
    pub fn code(&self) -> &'static str {
        "validation_failed"
    }

    /// Returns field-level validation errors in deterministic order.
    pub fn field_errors(&self) -> Vec<FieldValidationError> {
        match self {
            Self::Validation(errors) => {
                let mut field_errors = errors
                    .iter()
                    .map(|(path, error)| field_error(&path.to_string(), error.message()))
                    .collect::<Vec<_>>();
                field_errors.sort_by(|left, right| {
                    left.field
                        .cmp(&right.field)
                        .then_with(|| left.code.cmp(&right.code))
                });
                field_errors
            }
        }
    }
}

fn field_error(field: &str, message: &str) -> FieldValidationError {
    FieldValidationError {
        field: field.to_owned(),
        code: validation_code(message).to_owned(),
        message: Some(message.to_owned()),
    }
}

fn validation_code(message: &str) -> &'static str {
    if message.starts_with("not a valid email") {
        "email"
    } else if message.starts_with("length is ") {
        "length"
    } else if message.starts_with("not a valid url") {
        "url"
    } else if message.starts_with("not a valid IP") || message.starts_with("not a valid IPv") {
        "ip"
    } else if message.starts_with("lower than ") || message.starts_with("greater than ") {
        "range"
    } else {
        "invalid"
    }
}

impl IntoResponse for ValidationPipeError {
    fn into_response(self) -> axum::response::Response {
        let status = self.status_code();
        let body = Json(ValidationErrorBody {
            error: ValidationErrorDetails {
                code: self.code(),
                message: "request validation failed",
                fields: self.field_errors(),
            },
        });
        (status, body).into_response()
    }
}

#[derive(Debug, Serialize)]
struct ValidationErrorBody {
    error: ValidationErrorDetails,
}

#[derive(Debug, Serialize)]
struct ValidationErrorDetails {
    code: &'static str,
    message: &'static str,
    fields: Vec<FieldValidationError>,
}

/// Stable field-level validation error summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FieldValidationError {
    field: String,
    code: String,
    message: Option<String>,
}

impl FieldValidationError {
    /// Returns the invalid field name.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the validation rule error code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the optional validation message.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}
