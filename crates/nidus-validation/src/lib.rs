//! Validation pipe support.

use validator::Validate;

/// Typed request transformation or validation pipe.
pub trait Pipe<Input>: Send + Sync + 'static {
    /// Output produced by this pipe.
    type Output;

    /// Error emitted when transformation or validation fails.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Transforms or validates the input value.
    fn transform(&self, input: Input) -> std::result::Result<Self::Output, Self::Error>;
}

/// Request validation pipe backed by the `validator` crate.
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
        T: Validate,
    {
        input.validate().map_err(ValidationPipeError::Validation)?;
        Ok(input)
    }
}

impl<T> Pipe<T> for ValidationPipe
where
    T: Validate,
{
    type Output = T;
    type Error = ValidationPipeError;

    fn transform(&self, input: T) -> std::result::Result<Self::Output, Self::Error> {
        ValidationPipe::transform(self, input)
    }
}

/// Result type for validation pipes.
pub type Result<T> = std::result::Result<T, ValidationPipeError>;

/// Errors emitted by validation pipes.
#[derive(Debug, thiserror::Error)]
pub enum ValidationPipeError {
    /// The input failed validation.
    #[error("validation failed: {0}")]
    Validation(#[from] validator::ValidationErrors),
}

impl ValidationPipeError {
    /// Returns field-level validation errors in deterministic order.
    pub fn field_errors(&self) -> Vec<FieldValidationError> {
        match self {
            Self::Validation(errors) => {
                let mut field_errors = errors
                    .field_errors()
                    .into_iter()
                    .flat_map(|(field, errors)| {
                        errors.iter().map(move |error| FieldValidationError {
                            field: field.to_string(),
                            code: error.code.to_string(),
                            message: error.message.as_ref().map(ToString::to_string),
                        })
                    })
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

/// Stable field-level validation error summary.
#[derive(Clone, Debug, Eq, PartialEq)]
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

    /// Returns the validator error code.
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the optional validation message.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}
