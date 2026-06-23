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

    let routes = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "routes", "--path"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(routes.status.success());
    let routes_stdout = String::from_utf8(routes.stdout).unwrap();
    assert!(routes_stdout.contains("GET /users/"));

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
