mod support;

use std::{fs, process::Command};

use support::{temp_project_root, workspace_root};

#[test]
fn cargo_nidus_graph_inspects_crate_root_modules() {
    let root = temp_project_root("graph_inspects_crate_root_modules");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();
    assert!(status.success());

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&project)
        .output()
        .unwrap();

    assert!(graph.status.success());
    let stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(stdout.contains("AppModule"));
}

#[test]
fn cargo_nidus_routes_and_graph_inspect_generated_sources() {
    let root = temp_project_root("routes_and_graph_inspect_generated_sources");
    for (kind, name) in [("module", "users"), ("controller", "users")] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", kind, name, "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[guard(AuthGuard)]\n    #[pipe(ValidationPipe)]\n    #[validate]\n    #[openapi(summary=\"Find user\",tags=[\"users\", \"read\"])]",
    );
    fs::write(controller_path, controller).unwrap();

    let routes = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "routes", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(routes.status.success());
    let routes_stdout = String::from_utf8(routes.stdout).unwrap();
    assert!(routes_stdout.contains("GET /users/{id} - Find user"));
    assert!(routes_stdout.contains("[guards: AuthGuard; pipes: ValidationPipe; validates]"));

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(graph.status.success());
    let graph_stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(graph_stdout.contains("UsersModule"));
}

#[test]
fn cargo_nidus_routes_rejects_empty_route_param_names() {
    let root = temp_project_root("routes_rejects_empty_route_param_names");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path)
        .unwrap()
        .replace("#[get(\"/\")]", "#[get(\"/:\")]");
    fs::write(controller_path, controller).unwrap();

    let routes = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "routes", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!routes.status.success());
    let stderr = String::from_utf8(routes.stderr).unwrap();
    assert!(
        stderr.contains("route path `/:` contains a parameter segment without a name after ':'")
    );
}

#[test]
fn cargo_nidus_routes_rejects_duplicate_route_methods() {
    let root = temp_project_root("routes_rejects_duplicate_route_methods");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path)
        .unwrap()
        .replace("#[get(\"/\")]", "#[get(\"/:id\")]\n    #[post(\"/\")]");
    fs::write(controller_path, controller).unwrap();

    let routes = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "routes", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!routes.status.success());
    let stderr = String::from_utf8(routes.stderr).unwrap();
    assert!(stderr.contains("route methods must declare exactly one HTTP method attribute"));
}

#[test]
fn cargo_nidus_routes_and_openapi_reject_duplicate_route_declarations() {
    let root = temp_project_root("routes_and_openapi_reject_duplicate_route_declarations");
    for controller in ["users", "accounts"] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", "controller", controller, "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let accounts_path = root.join("src/controllers/accounts.rs");
    let accounts = fs::read_to_string(&accounts_path)
        .unwrap()
        .replace("#[controller(\"/accounts\")]", "#[controller(\"/users\")]");
    fs::write(accounts_path, accounts).unwrap();

    for command in ["routes", "openapi"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", command, "--path"])
            .arg(&root)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "{command} should reject duplicate route"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("duplicate route declaration for GET /users"));
    }
}

#[test]
fn cargo_nidus_routes_and_openapi_reject_malformed_controller_metadata() {
    let root = temp_project_root("routes_and_openapi_reject_malformed_controller_metadata");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path)
        .unwrap()
        .replace("#[controller(\"/users\")]", "#[controller]");
    fs::write(controller_path, controller).unwrap();

    for command in ["routes", "openapi"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", command, "--path"])
            .arg(&root)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "{command} should reject malformed controller metadata"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("#[controller] requires a string literal path"));
    }
}

#[test]
fn cargo_nidus_routes_and_openapi_reject_malformed_route_type_metadata() {
    let root = temp_project_root("routes_and_openapi_reject_malformed_route_type_metadata");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "controller", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());
    let controller_path = root.join("src/controllers/users.rs");
    let controller = fs::read_to_string(&controller_path)
        .unwrap()
        .replace("#[get(\"/\")]", "#[guard]\n    #[get(\"/\")]");
    fs::write(controller_path, controller).unwrap();

    for command in ["routes", "openapi"] {
        let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", command, "--path"])
            .arg(&root)
            .output()
            .unwrap();

        assert!(
            !output.status.success(),
            "{command} should reject malformed route type metadata"
        );
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(stderr.contains("#[guard] requires a type path"));
    }
}

#[test]
fn cargo_nidus_graph_prints_module_builder_metadata() {
    let root = temp_project_root("graph_prints_module_builder_metadata");
    let modules = root.join("src/modules");
    fs::create_dir_all(&modules).unwrap();
    fs::write(
        modules.join("users.rs"),
        r#"use nidus::prelude::*;

pub struct UsersModule;

impl Module for UsersModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("UsersModule")
            .import("DatabaseModule")
            .provider("UsersService")
            .controller("UsersController")
            .export("UsersService")
            .build()
    }
}
"#,
    )
    .unwrap();

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(graph.status.success());
    let stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(stdout.contains("UsersModule"));
    assert!(stdout.contains("imports: DatabaseModule"));
    assert!(stdout.contains("providers: UsersService"));
    assert!(stdout.contains("controllers: UsersController"));
    assert!(stdout.contains("exports: UsersService"));
}

#[test]
fn cargo_nidus_graph_prints_module_field_metadata() {
    let root = temp_project_root("graph_prints_module_field_metadata");
    let modules = root.join("src/modules");
    fs::create_dir_all(&modules).unwrap();
    fs::write(
        modules.join("users.rs"),
        r#"use nidus::prelude::*;

struct DatabaseModule;
struct UsersService;
struct UsersRepository;
struct UsersController;

#[module]
pub struct UsersModule {
    imports: (crate::DatabaseModule,),
    providers: (UsersService, UsersRepository),
    controllers: (UsersController,),
    exports: [UsersService],
}
"#,
    )
    .unwrap();

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(graph.status.success());
    let stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(stdout.contains("UsersModule"));
    assert!(stdout.contains("imports: DatabaseModule"));
    assert!(stdout.contains("providers: UsersService, UsersRepository"));
    assert!(stdout.contains("controllers: UsersController"));
    assert!(stdout.contains("exports: UsersService"));
}

#[test]
fn cargo_nidus_graph_prints_module_attribute_metadata() {
    let root = temp_project_root("graph_prints_module_attribute_metadata");
    let modules = root.join("src/modules");
    fs::create_dir_all(&modules).unwrap();
    fs::write(
        modules.join("users.rs"),
        r#"use nidus::prelude::*;

struct DatabaseModule;
struct UsersService;
struct UsersRepository;
struct UsersController;

#[module(
    imports(crate::database::DatabaseModule),
    providers(crate::users::UsersService, crate::users::UsersRepository),
    controllers(crate::users::UsersController),
    exports(crate::users::UsersService)
)]
pub struct UsersModule;
"#,
    )
    .unwrap();

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(graph.status.success());
    let stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(stdout.contains("UsersModule"));
    assert!(stdout.contains("imports: DatabaseModule"));
    assert!(stdout.contains("providers: UsersService, UsersRepository"));
    assert!(stdout.contains("controllers: UsersController"));
    assert!(stdout.contains("exports: UsersService"));
}

#[test]
fn cargo_nidus_check_validates_project_structure() {
    let root = temp_project_root("check_validates_project_structure");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();
    assert!(status.success());

    let valid = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&project)
        .output()
        .unwrap();
    assert!(valid.status.success());
    assert!(
        String::from_utf8(valid.stdout)
            .unwrap()
            .contains("Nidus project check passed")
    );

    let invalid = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(root.join("missing"))
        .output()
        .unwrap();
    assert!(!invalid.status.success());
    let stderr = String::from_utf8(invalid.stderr).unwrap();
    assert!(stderr.contains("Cargo.toml"));
}

#[test]
fn cargo_nidus_check_rejects_stale_module_index_entries() {
    let root = temp_project_root("check_rejects_stale_module_index_entries");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/services")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("src/services/mod.rs"), "pub mod missing;\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("stale module index entry"));
    assert!(stderr.contains("src/services/mod.rs"));
    assert!(stderr.contains("missing.rs"));
}

#[test]
fn cargo_nidus_check_rejects_unindexed_generated_sources() {
    let root = temp_project_root("check_rejects_unindexed_generated_sources");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/services")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        root.join("src/services/users.rs"),
        "pub struct UsersService;\n",
    )
    .unwrap();
    fs::write(root.join("src/services/mod.rs"), "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("missing module index entry"));
    assert!(stderr.contains("src/services/mod.rs"));
    assert!(stderr.contains("pub mod users;"));
}

#[test]
fn cargo_nidus_check_rejects_undeclared_generated_directories() {
    let root = temp_project_root("check_rejects_undeclared_generated_directories");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src/services")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(
        root.join("src/services/users.rs"),
        "pub struct UsersService;\n",
    )
    .unwrap();
    fs::write(root.join("src/services/mod.rs"), "pub mod users;\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("missing crate root module declaration"));
    assert!(stderr.contains("src/services"));
    assert!(stderr.contains("mod services;"));
}

#[test]
fn cargo_nidus_check_accepts_generated_module_indexes() {
    let root = temp_project_root("check_accepts_generated_module_indexes");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

    let generate = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(generate.success());

    let check = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(check.status.success());
    assert!(
        String::from_utf8(check.stdout)
            .unwrap()
            .contains("Nidus project check passed")
    );
}

#[test]
fn cargo_nidus_check_accepts_library_crate_roots() {
    let root = temp_project_root("check_accepts_library_crate_roots");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/lib.rs"), "pub mod services;\n").unwrap();

    let generate = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(generate.success());

    let check = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(check.status.success());
    assert!(
        String::from_utf8(check.stdout)
            .unwrap()
            .contains("Nidus project check passed")
    );
}

#[test]
fn cargo_nidus_check_rejects_projects_without_crate_roots() {
    let root = temp_project_root("check_rejects_projects_without_crate_roots");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "check", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Missing required crate root"));
    assert!(stderr.contains("src/main.rs or src/lib.rs"));
}

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

#[test]
fn cargo_nidus_expand_prints_cargo_expand_command_in_dry_run_mode() {
    let root = temp_project_root("expand_prints_cargo_expand_command");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();
    assert!(status.success());

    let expand = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "expand", "--path"])
        .arg(&project)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(expand.status.success());
    let stdout = String::from_utf8(expand.stdout).unwrap();
    assert!(stdout.contains("cargo expand --manifest-path"));
    assert!(stdout.contains("Cargo.toml"));
}

#[test]
fn cargo_nidus_expand_rejects_missing_manifest() {
    let root = temp_project_root("expand_rejects_missing_manifest");

    let expand = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "expand", "--path"])
        .arg(&root)
        .arg("--dry-run")
        .output()
        .unwrap();

    assert!(!expand.status.success());
    let stderr = String::from_utf8(expand.stderr).unwrap();
    assert!(stderr.contains("Nidus expand failed"));
    assert!(stderr.contains("Cargo.toml"));
}
