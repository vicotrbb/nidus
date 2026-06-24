use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[test]
fn cargo_nidus_new_generates_compilable_nidus_project() {
    let root = temp_project_root("new_generates_compilable_nidus_project");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();

    assert!(status.success());
    assert!(project.join("Cargo.toml").exists());
    assert!(project.join("src/main.rs").exists());
    let cargo_toml = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(!cargo_toml.contains("axum ="));
    assert!(!cargo_toml.contains("tokio ="));
    let main_rs = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("#[nidus::main]"));
    assert!(!main_rs.contains("#[tokio::main]"));
    assert!(main_rs.contains("Controller::new(\"/\")"));
    assert!(main_rs.contains("RouteDefinition::get(\"/\""));
    assert!(main_rs.contains("Nidus::bootstrap::<AppModule>()"));
    assert!(main_rs.contains("NIDUS_ADDR"));
    assert!(main_rs.contains("#[module]"));
    assert!(main_rs.contains("struct AppModule;"));
    assert!(!main_rs.contains("impl Module for AppModule"));
    assert!(main_rs.contains(".with_router("));
    assert!(main_rs.contains(".listen(address)"));
    assert!(
        fs::read_to_string(project.join("README.md"))
            .unwrap()
            .contains("hello-nidus")
    );
    assert!(
        fs::read_to_string(project.join("README.md"))
            .unwrap()
            .contains("NIDUS_ADDR")
    );

    let check = Command::new("cargo")
        .arg("check")
        .current_dir(&project)
        .status()
        .unwrap();
    assert!(check.success());
}

#[test]
fn cargo_nidus_new_generates_runnable_http_server() {
    let root = temp_project_root("new_generates_runnable_http_server");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();
    assert!(status.success());

    let address = free_loopback_address();
    let child = Command::new("cargo")
        .arg("run")
        .env("NIDUS_ADDR", &address)
        .current_dir(&project)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard::new(child);

    let response = wait_for_http_response(&address, child.as_mut());
    child.stop();

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("hello from nidus"), "{response}");
}

#[test]
fn cargo_nidus_new_refuses_to_overwrite_existing_project() {
    let root = temp_project_root("new_refuses_to_overwrite_existing_project");
    let project = root.join("hello-nidus");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("Cargo.toml"), "# existing manifest\n").unwrap();
    fs::write(project.join("src/main.rs"), "// user edits\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("project already exists"));
    assert_eq!(
        fs::read_to_string(project.join("Cargo.toml")).unwrap(),
        "# existing manifest\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("src/main.rs")).unwrap(),
        "// user edits\n"
    );
}

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
fn cargo_nidus_generate_writes_rust_artifact_scaffolds() {
    let root = temp_project_root("generate_writes_rust_artifact_scaffolds");
    for (kind, expected_path, expected_mod_rs, expected_content) in [
        (
            "module",
            "src/modules/users.rs",
            "src/modules/mod.rs",
            "pub struct UsersModule;",
        ),
        (
            "controller",
            "src/controllers/users.rs",
            "src/controllers/mod.rs",
            "#[controller(\"/users\")]",
        ),
        (
            "service",
            "src/services/users.rs",
            "src/services/mod.rs",
            "pub struct UsersService;",
        ),
        (
            "repository",
            "src/repositories/users.rs",
            "src/repositories/mod.rs",
            "pub struct UsersRepository;",
        ),
    ] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", kind, "users", "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());

        let contents = fs::read_to_string(root.join(expected_path)).unwrap();
        assert!(contents.contains(expected_content));
        let module_index = fs::read_to_string(root.join(expected_mod_rs)).unwrap();
        assert!(module_index.contains("pub mod users;"));
    }
}

#[test]
fn cargo_nidus_generate_module_includes_existing_feature_artifacts() {
    let root = temp_project_root("generate_module_includes_existing_feature_artifacts");
    for kind in ["repository", "service", "controller", "module"] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", kind, "users", "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
    }

    let module = fs::read_to_string(root.join("src/modules/users.rs")).unwrap();
    assert_module_metadata_is_synced(&module);

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(graph.status.success());
    let stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(stdout.contains("providers: UsersRepository, UsersService"));
    assert!(stdout.contains("controllers: UsersController"));
    assert!(stdout.contains("exports: UsersService"));
}

fn assert_module_metadata_is_synced(module: &str) {
    assert!(module.contains(
        "providers(crate::repositories::users::UsersRepository, crate::services::users::UsersService)"
    ));
    assert!(module.contains("controllers(crate::controllers::users::UsersController)"));
    assert!(module.contains("exports(crate::services::users::UsersService)"));
}

#[test]
fn cargo_nidus_generate_artifacts_update_existing_generated_module() {
    let root = temp_project_root("generate_artifacts_update_existing_generated_module");
    for kind in ["module", "repository", "service", "controller"] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", kind, "users", "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
    }

    let module = fs::read_to_string(root.join("src/modules/users.rs")).unwrap();
    assert_module_metadata_is_synced(&module);
}

#[test]
fn cargo_nidus_generate_artifacts_preserve_custom_module_bodies() {
    let root = temp_project_root("generate_artifacts_preserve_custom_module_bodies");
    fs::create_dir_all(root.join("src/modules")).unwrap();
    fs::write(
        root.join("src/modules/users.rs"),
        r#"use nidus::prelude::*;

#[module]
pub struct UsersModule {
    providers: (ExistingProvider,),
}
"#,
    )
    .unwrap();

    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(status.success());

    let module = fs::read_to_string(root.join("src/modules/users.rs")).unwrap();
    assert!(module.contains("providers: (ExistingProvider,)"));
    assert!(!module.contains("UsersService"));
}

#[test]
fn cargo_nidus_generate_maintains_directory_module_indexes() {
    let root = temp_project_root("generate_maintains_directory_module_indexes");
    for name in ["users", "accounts"] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", "service", name, "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
    }

    let duplicate = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "users", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!duplicate.status.success());

    let mod_rs = fs::read_to_string(root.join("src/services/mod.rs")).unwrap();
    assert!(mod_rs.contains("pub mod accounts;"));
    assert!(mod_rs.contains("pub mod users;"));
    assert_eq!(mod_rs.matches("pub mod users;").count(), 1);
}

#[test]
fn cargo_nidus_generate_updates_crate_root_module_declarations() {
    let root = temp_project_root("generate_updates_crate_root_module_declarations");
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "demo"
version = "0.1.0"
edition = "2024"

[dependencies]
nidus = {{ path = {:?} }}
"#,
            workspace_root().join("crates/nidus").display().to_string()
        ),
    )
    .unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();

    for kind in ["service", "controller"] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", kind, "users", "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success());
    }

    let main_rs = fs::read_to_string(root.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("mod controllers;"));
    assert!(main_rs.contains("mod services;"));
    assert_eq!(main_rs.matches("mod controllers;").count(), 1);
    assert_eq!(main_rs.matches("mod services;").count(), 1);

    let check = Command::new("cargo")
        .arg("check")
        .env("RUSTFLAGS", "-Dwarnings")
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(check.success());
}

#[test]
fn cargo_nidus_generate_normalizes_artifact_module_names() {
    let root = temp_project_root("generate_normalizes_artifact_module_names");
    let first = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "user-profile", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(first.success());

    let contents = fs::read_to_string(root.join("src/services/user_profile.rs")).unwrap();
    assert!(contents.contains("pub struct UserProfileService;"));
    let mod_rs = fs::read_to_string(root.join("src/services/mod.rs")).unwrap();
    assert!(mod_rs.contains("pub mod user_profile;"));
    assert!(!root.join("src/services/user-profile.rs").exists());

    let duplicate = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "user_profile", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(!duplicate.status.success());
    let stderr = String::from_utf8(duplicate.stderr).unwrap();
    assert!(stderr.contains("already exists"));
}

#[test]
fn cargo_nidus_generate_derives_type_names_from_normalized_modules() {
    let root = temp_project_root("generate_derives_type_names_from_normalized_modules");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "user.profile", "--path"])
        .arg(&root)
        .status()
        .unwrap();

    assert!(status.success());
    let contents = fs::read_to_string(root.join("src/services/user_profile.rs")).unwrap();
    assert!(contents.contains("pub struct UserProfileService;"));
    assert!(!contents.contains("User.profileService"));
    let mod_rs = fs::read_to_string(root.join("src/services/mod.rs")).unwrap();
    assert!(mod_rs.contains("pub mod user_profile;"));
}

#[test]
fn cargo_nidus_generate_rejects_digit_leading_artifact_names() {
    let root = temp_project_root("generate_rejects_digit_leading_artifact_names");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "123-users", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("artifact name must start"));
    assert!(!root.join("src/services").exists());
}

#[test]
fn cargo_nidus_generate_allows_digits_after_identifier_start() {
    let root = temp_project_root("generate_allows_digits_after_identifier_start");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "user-2", "--path"])
        .arg(&root)
        .status()
        .unwrap();

    assert!(status.success());
    let contents = fs::read_to_string(root.join("src/services/user_2.rs")).unwrap();
    assert!(contents.contains("pub struct User2Service;"));
    let mod_rs = fs::read_to_string(root.join("src/services/mod.rs")).unwrap();
    assert!(mod_rs.contains("pub mod user_2;"));
}

#[test]
fn cargo_nidus_generate_rejects_unknown_artifact_kinds() {
    let root = temp_project_root("generate_rejects_unknown_artifact_kinds");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "widget", "users", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unsupported artifact kind"));
    assert!(!root.join("src/widgets/users.rs").exists());
}

#[test]
fn cargo_nidus_generate_rejects_names_without_module_identifiers() {
    let root = temp_project_root("generate_rejects_names_without_module_identifiers");
    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "!!!", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("artifact name must contain"));
    assert!(!root.join("src/services").exists());
}

#[test]
fn cargo_nidus_generate_refuses_to_overwrite_existing_artifacts() {
    let root = temp_project_root("generate_refuses_to_overwrite_existing_artifacts");
    let service_path = root.join("src/services/users.rs");
    let first = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "users", "--path"])
        .arg(&root)
        .status()
        .unwrap();
    assert!(first.success());
    fs::write(&service_path, "// user edits\n").unwrap();

    let second = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "generate", "service", "users", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(!second.status.success());
    let stderr = String::from_utf8(second.stderr).unwrap();
    assert!(stderr.contains("already exists"));
    assert_eq!(fs::read_to_string(service_path).unwrap(), "// user edits\n");
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
    imports(DatabaseModule),
    providers(UsersService, UsersRepository),
    controllers(UsersController),
    exports(UsersService)
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

fn temp_project_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir()
        .join("nidus-cli-tests")
        .join(format!("{name}-{}", std::process::id()));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    root
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn free_loopback_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

fn wait_for_http_response(address: &str, child: &mut Child) -> String {
    let timeout = Duration::from_secs(90);
    let deadline = Instant::now() + timeout;
    let mut last_error = None;

    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("generated server exited before responding: {status}");
        }

        match TcpStream::connect(address) {
            Ok(mut stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                return response;
            }
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    panic!(
        "generated server did not respond at {address} within {timeout:?}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no connection attempt made".to_owned())
    );
}

struct ChildGuard {
    child: Child,
    stopped: bool,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self {
            child,
            stopped: false,
        }
    }

    fn as_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        if self.stopped {
            return;
        }
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        let _ = self.child.wait();
        self.stopped = true;
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        if !std::thread::panicking() {
            self.stop_inner();
        } else if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
