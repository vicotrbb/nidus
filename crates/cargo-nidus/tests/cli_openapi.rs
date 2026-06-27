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
fn cargo_nidus_openapi_accepts_document_title_and_version() {
    let root = temp_project_root("openapi_accepts_document_title_and_version");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args([
            "nidus",
            "openapi",
            "--title",
            "Users API",
            "--version",
            "2026.6",
            "--path",
        ])
        .arg(&root)
        .output()
        .unwrap();

    assert!(openapi.status.success());
    let stdout = String::from_utf8(openapi.stdout).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(json["info"]["title"], "Users API");
    assert_eq!(json["info"]["version"], "2026.6");
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
