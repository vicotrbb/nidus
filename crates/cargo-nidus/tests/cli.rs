use std::{fs, path::PathBuf, process::Command};

#[test]
fn cargo_nidus_new_generates_compilable_axum_project() {
    let root = temp_project_root("new_generates_compilable_axum_project");
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
    assert!(!cargo_toml.contains("tokio ="));
    let main_rs = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("#[nidus::main]"));
    assert!(!main_rs.contains("#[tokio::main]"));
    assert!(main_rs.contains("Nidus::bootstrap::<AppModule>()"));
    assert!(main_rs.contains(".with_router("));
    assert!(main_rs.contains(".listen(\"127.0.0.1:3000\")"));
    assert!(
        fs::read_to_string(project.join("README.md"))
            .unwrap()
            .contains("hello-nidus")
    );

    let check = Command::new("cargo")
        .arg("check")
        .current_dir(&project)
        .status()
        .unwrap();
    assert!(check.success());
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
fn cargo_nidus_generate_writes_rust_artifact_scaffolds() {
    let root = temp_project_root("generate_writes_rust_artifact_scaffolds");
    for (kind, expected_path, expected_content) in [
        ("module", "src/modules/users.rs", "pub struct UsersModule;"),
        (
            "controller",
            "src/controllers/users.rs",
            "#[controller(\"/users\")]",
        ),
        (
            "service",
            "src/services/users.rs",
            "pub struct UsersService;",
        ),
        (
            "repository",
            "src/repositories/users.rs",
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
    }
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
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user\")]",
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
    assert!(stderr.contains("src/main.rs"));
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
    let controller = fs::read_to_string(&controller_path).unwrap().replace(
        "#[get(\"/\")]",
        "#[get(\"/:id\")]\n    #[openapi(summary = \"Find user\")]",
    );
    fs::write(controller_path, controller).unwrap();

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(openapi.status.success());
    let stdout = String::from_utf8(openapi.stdout).unwrap();
    assert!(stdout.contains(r#""openapi":"3.1.0""#));
    assert!(stdout.contains(r#""/users/{id}""#));
    assert!(stdout.contains(r#""get""#));
    assert!(stdout.contains(r#""summary":"Find user""#));
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
