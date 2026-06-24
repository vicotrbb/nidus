mod support;

use std::{fs, process::Command};

use support::temp_project_root;

#[test]
fn cargo_nidus_openapi_generates_document_from_controllers() {
    let root = temp_project_root("openapi_generates_document_from_controllers");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path)
        .unwrap()
        .replace(
            "pub struct UsersController;",
            r#"pub struct UsersController;

pub struct CreateUserDto {
    email: String,
    age: Option<u16>,
}

pub struct UserDto {
    #[serde(rename = "user_id")]
    id: u64,
    email: String,
    profile: UserProfile,
    #[serde(default)]
    display_name: String,
    #[serde(skip)]
    internal_notes: String,
    roles: Vec<String>,
}

pub struct UserProfile {
    display_name: String,
}"#,
        )
        .replace(
            "#[get(\"/\")]",
            r#"#[get("/:id")]
    #[guard(AuthGuard)]
    #[pipe(ValidationPipe)]
    #[validate]
    #[openapi(
        summary = "Find user",
        tags = ["users", "read"],
        status = 201,
        request = CreateUserDto,
        response = UserDto
    )]"#,
        );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(openapi.status.success());
    let stdout = String::from_utf8(openapi.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["openapi"], "3.1.0");
    assert_eq!(json["paths"]["/users/{id}"]["get"]["summary"], "Find user");
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["operationId"],
        "get_users_by_id"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["tags"],
        serde_json::json!(["users", "read"])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["x-nidus-guards"],
        serde_json::json!(["AuthGuard"])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["x-nidus-pipes"],
        serde_json::json!(["ValidationPipe"])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["x-nidus-validates"],
        true
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["parameters"],
        serde_json::json!([
            {
                "name": "id",
                "in": "path",
                "required": true,
                "schema": {
                    "type": "string"
                }
            }
        ])
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["requestBody"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/CreateUserDto"
    );
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["responses"]["201"]["content"]["application/json"]["schema"]
            ["$ref"],
        "#/components/schemas/UserDto"
    );
    assert!(json["paths"]["/users/{id}"]["get"]["responses"]["200"].is_null());
    assert_eq!(
        json["components"]["schemas"]["CreateUserDto"]["properties"]["email"]["type"],
        "string"
    );
    assert_eq!(
        json["components"]["schemas"]["CreateUserDto"]["properties"]["age"]["type"],
        "integer"
    );
    assert_eq!(
        json["components"]["schemas"]["CreateUserDto"]["required"],
        serde_json::json!(["email"])
    );
    assert_eq!(
        json["components"]["schemas"]["UserDto"]["properties"]["user_id"]["type"],
        "integer"
    );
    assert!(json["components"]["schemas"]["UserDto"]["properties"]["internal_notes"].is_null());
    assert_eq!(
        json["components"]["schemas"]["UserDto"]["required"],
        serde_json::json!(["user_id", "email", "profile", "roles"])
    );
    assert_eq!(
        json["components"]["schemas"]["UserDto"]["properties"]["roles"]["type"],
        "array"
    );
    assert_eq!(
        json["components"]["schemas"]["UserDto"]["properties"]["roles"]["items"]["type"],
        "string"
    );
    assert_eq!(
        json["components"]["schemas"]["UserDto"]["properties"]["profile"]["$ref"],
        "#/components/schemas/UserProfile"
    );
    assert_eq!(
        json["components"]["schemas"]["UserProfile"]["properties"]["display_name"]["type"],
        "string"
    );
}

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
fn cargo_nidus_openapi_ignores_tags_word_in_summary() {
    let root = temp_project_root("openapi_ignores_tags_word_in_summary");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user tags\")]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(openapi.status.success());
    let stdout = String::from_utf8(openapi.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(
        json["paths"]["/users/{id}"]["get"]["summary"],
        "Find user tags"
    );
    assert!(json["paths"]["/users/{id}"]["get"].get("tags").is_none());
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
