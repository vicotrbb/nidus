mod support;

use std::{fs, process::Command};

use support::{temp_project_root, workspace_root};

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
    assert!(main_rs.contains("use nidus::prelude::*;"));
    assert!(!main_rs.contains("use nidus::{"));
    assert!(main_rs.contains("#[nidus::main]"));
    assert!(!main_rs.contains("#[tokio::main]"));
    assert!(main_rs.contains("#[controller(\"/\")]"));
    assert!(main_rs.contains("#[routes]"));
    assert!(main_rs.contains("#[get(\"/\")]"));
    assert!(main_rs.contains("Nidus::create::<AppModule>()"));
    assert!(main_rs.contains("GreetingService"));
    assert!(main_rs.contains("greeting: Inject<GreetingService>"));
    assert!(main_rs.contains("ApiDefaults::production(\"hello-nidus\")"));
    assert!(main_rs.contains(".without_metrics()"));
    assert!(main_rs.contains("NIDUS_ADDR"));
    assert!(main_rs.contains("#[module("));
    assert!(main_rs.contains("providers(GreetingService)"));
    assert!(main_rs.contains("controllers(HelloController)"));
    assert!(main_rs.contains("struct AppModule;"));
    assert!(!main_rs.contains("impl Module for AppModule"));
    assert!(main_rs.contains(".map_router("));
    assert!(main_rs.contains(".listen(address)"));
    let readme = fs::read_to_string(project.join("README.md")).unwrap();
    assert!(readme.contains("hello-nidus"));
    assert!(readme.contains("NIDUS_ADDR"));
    assert!(readme.contains("cargo test"));
    assert!(readme.contains("cargo nidus generate controller users"));
    assert!(readme.contains("cargo nidus routes"));
    assert!(readme.contains("curl http://127.0.0.1:3000/"));
    assert!(readme.contains("## Common imports and extension traits"));
    assert!(readme.contains("use nidus::prelude::*;"));
    assert!(readme.contains("ApplicationHttpExt"));
    assert!(readme.contains("NidusApplicationExt"));
    assert!(readme.contains("ApiDefaultsObservabilityExt"));
    assert!(readme.contains("## Common compile errors"));
    assert!(readme.contains("no method named `with_router`"));
    assert!(readme.contains("no method named `listen`"));
    assert!(readme.contains("no method named `observability`"));
    assert!(readme.contains("Next steps"));

    let check = Command::new("cargo")
        .arg("check")
        .current_dir(&project)
        .status()
        .unwrap();
    assert!(check.success());
}

#[test]
fn cargo_nidus_new_defaults_to_published_nidus_dependency() {
    let root = temp_project_root("new_defaults_to_published_nidus_dependency");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .status()
        .unwrap();

    assert!(status.success());
    let cargo_toml = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(cargo_toml.contains(r#"nidus = { package = "nidus-rs", version = "1.0.1" }"#));
    assert!(!cargo_toml.contains("nidus = { path ="));
}

#[test]
fn cargo_nidus_new_uses_project_name_for_service_name() {
    let root = temp_project_root("new_uses_project_name_for_service_name");
    let project = root.join("team-api");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "team-api", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();

    assert!(status.success());
    let main_rs = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("ApiDefaults::production(\"team-api\")"));
    assert!(!main_rs.contains("ApiDefaults::production(\"hello-nidus\")"));
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
