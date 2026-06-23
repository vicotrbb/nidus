use validator::Validate;

use nidus_validation::ValidationPipe;

#[derive(Debug, Validate)]
struct CreateUser {
    #[validate(email)]
    email: String,
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
