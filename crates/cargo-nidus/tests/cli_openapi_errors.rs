mod support;

use std::{fs, process::Command};

use support::temp_project_root;

#[test]
fn cargo_nidus_openapi_rejects_non_string_tags() {
    let root = temp_project_root("openapi_rejects_non_string_tags");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user\", tags = [42])]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(stderr.contains("#[openapi] tags must be string literals"));
}

#[test]
fn cargo_nidus_openapi_rejects_non_string_summary() {
    let root = temp_project_root("openapi_rejects_non_string_summary");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = 42)]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(stderr.contains("#[openapi] summary must be a string literal"));
}

#[test]
fn cargo_nidus_openapi_rejects_non_type_request_schema() {
    let root = temp_project_root("openapi_rejects_non_type_request_schema");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user\", request = \"CreateUserDto\")]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(stderr.contains("#[openapi] request must be a type path"));
}

#[test]
fn cargo_nidus_openapi_rejects_invalid_status() {
    let root = temp_project_root("openapi_rejects_invalid_status");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user\", status = 99)]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(stderr.contains("#[openapi] status must be in the HTTP status code range 100..=599"));
}

#[test]
fn cargo_nidus_openapi_rejects_unsupported_metadata_keys() {
    let root = temp_project_root("openapi_rejects_unsupported_metadata_keys");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user\", description = \"By ID\")]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(
        stderr.contains("#[openapi] supports only summary = \"...\", tags = [\"...\"], status = 201, request = Type, and response = Type metadata")
    );
}

#[test]
fn cargo_nidus_openapi_requires_summary_metadata() {
    let root = temp_project_root("openapi_requires_summary_metadata");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(tags = [\"users\"])]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(stderr.contains("#[openapi] requires summary = \"...\" metadata"));
}

#[test]
fn cargo_nidus_openapi_rejects_unterminated_metadata() {
    let root = temp_project_root("openapi_rejects_unterminated_metadata");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(\n        summary = \"Find user\"",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!openapi.status.success());
    let stderr = String::from_utf8(openapi.stderr).unwrap();
    assert!(stderr.contains("unterminated #[openapi] metadata"));
}
