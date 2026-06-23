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
