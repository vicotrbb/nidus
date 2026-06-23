use std::{fs, path::PathBuf, process::Command};

#[test]
fn cargo_nidus_new_generates_compilable_axum_project() {
    let root = temp_project_root("new_generates_compilable_axum_project");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
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
