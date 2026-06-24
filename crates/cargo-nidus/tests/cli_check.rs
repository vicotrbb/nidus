mod support;

use std::{fs, process::Command};

use support::{temp_project_root, workspace_root};

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
