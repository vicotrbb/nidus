mod support;

use std::{fs, process::Command};

use support::{temp_project_root, workspace_root};

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
