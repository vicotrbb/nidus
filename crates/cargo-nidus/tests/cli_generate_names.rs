mod support;

use std::{fs, process::Command};

use support::temp_project_root;

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
