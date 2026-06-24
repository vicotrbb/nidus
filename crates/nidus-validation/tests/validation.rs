use std::convert::Infallible;

use nidus_validation::{Pipe, ValidationPipe};
use validator::Validate;

#[derive(Debug, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
}

struct TrimEmailPipe;

impl Pipe<CreateUser> for TrimEmailPipe {
    type Output = CreateUser;
    type Error = Infallible;

    fn transform(&self, mut input: CreateUser) -> Result<Self::Output, Self::Error> {
        input.email = input.email.trim().to_owned();
        Ok(input)
    }
}

#[test]
fn validation_pipe_accepts_valid_values() {
    let input = CreateUser {
        email: "user@nidus.dev".to_owned(),
    };

    let output = ValidationPipe::new().transform(input).unwrap();

    assert_eq!(output.email, "user@nidus.dev");
}

#[test]
fn validation_pipe_rejects_invalid_values() {
    let input = CreateUser {
        email: "not-an-email".to_owned(),
    };

    let error = ValidationPipe::new().transform(input).unwrap_err();

    assert!(error.to_string().contains("email"));
}

#[test]
fn validation_errors_expose_field_level_details() {
    let input = CreateUser {
        email: "not-an-email".to_owned(),
    };

    let error = ValidationPipe::new().transform(input).unwrap_err();
    let fields = error.field_errors();

    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].field(), "email");
    assert_eq!(fields[0].code(), "email");
    assert_eq!(fields[0].message(), None);
}

#[test]
fn custom_pipe_transforms_request_values() {
    let input = CreateUser {
        email: " user@nidus.dev ".to_owned(),
    };

    let output = TrimEmailPipe.transform(input).unwrap();

    assert_eq!(output.email, "user@nidus.dev");
}

#[test]
fn validation_pipe_implements_typed_pipe_trait() {
    let input = CreateUser {
        email: "user@nidus.dev".to_owned(),
    };

    let output =
        <ValidationPipe as Pipe<CreateUser>>::transform(&ValidationPipe::new(), input).unwrap();

    assert_eq!(output.email, "user@nidus.dev");
}
