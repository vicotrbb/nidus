mod support;

use std::{fs, process::Command};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use support::{temp_project_root, workspace_root};

#[test]
fn cargo_nidus_routes_and_openapi_inspect_feature_oriented_sources() {
    let root = temp_project_root("routes_inspect_feature_oriented_sources");
    let controller_directory = root.join("src/domains/users");
    fs::create_dir_all(&controller_directory).unwrap();
    fs::write(
        root.join("src/lib.rs"),
        r##"const OPENAPI_EXAMPLE: &str = "#[openapi(";"##,
    )
    .unwrap();
    fs::write(
        controller_directory.join("controller.rs"),
        r#"
#[controller("/users")]
struct UsersController;

#[routes]
impl UsersController {
    #[get("/:id")]
    #[openapi(summary = "Find nested user", tags = ["users"])]
    async fn find_user(&self) {}
}
"#,
    )
    .unwrap();

    let routes = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "routes", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(routes.status.success());
    assert!(
        String::from_utf8(routes.stdout)
            .unwrap()
            .contains("GET /users/{id} - Find nested user")
    );

    let openapi = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "openapi", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(openapi.status.success());
    let document: serde_json::Value = serde_json::from_slice(&openapi.stdout).unwrap();
    assert_eq!(
        document["paths"]["/users/{id}"]["get"]["summary"],
        "Find nested user"
    );
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

#[test]
#[cfg(unix)]
fn cargo_nidus_expand_reports_missing_cargo_expand() {
    let root = temp_project_root("expand_reports_missing_cargo_expand");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let fake_cargo = fake_bin.join("cargo");
    fs::write(
        &fake_cargo,
        "#!/bin/sh\nprintf 'error: no such command: `expand`\\n' >&2\nexit 101\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_cargo).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_cargo, permissions).unwrap();

    let expand = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "expand", "--path"])
        .arg(&root)
        .env("PATH", &fake_bin)
        .output()
        .unwrap();

    assert!(!expand.status.success());
    let stderr = String::from_utf8(expand.stderr).unwrap();
    assert!(stderr.contains("cargo-expand is not installed"), "{stderr}");
    assert!(stderr.contains("cargo install cargo-expand"), "{stderr}");
}
