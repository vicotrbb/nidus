//! Validation pipe support.

use validator::Validate;

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

/// Result type for validation pipes.
pub type Result<T> = std::result::Result<T, ValidationPipeError>;

/// Errors emitted by validation pipes.
#[derive(Debug, thiserror::Error)]
pub enum ValidationPipeError {
    /// The input failed validation.
    #[error("validation failed: {0}")]
    Validation(#[from] validator::ValidationErrors),
}
