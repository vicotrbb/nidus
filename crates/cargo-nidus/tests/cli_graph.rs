mod support;

use std::{fs, process::Command};

use support::{temp_project_root, workspace_root};

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
fn cargo_nidus_graph_inspects_generated_feature_directories() {
    let root = temp_project_root("graph_inspects_generated_feature_directories");
    for kind in ["controller", "service", "repository"] {
        let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
            .args(["nidus", "generate", kind, "users", "--path"])
            .arg(&root)
            .status()
            .unwrap();
        assert!(status.success(), "generate {kind} failed");
    }

    let graph = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "graph", "--path"])
        .arg(&root)
        .output()
        .unwrap();

    assert!(graph.status.success());
    let stdout = String::from_utf8(graph.stdout).unwrap();
    assert!(stdout.contains("UsersController"), "{stdout}");
    assert!(stdout.contains("UsersService"), "{stdout}");
    assert!(stdout.contains("UsersRepository"), "{stdout}");
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
    imports(crate::database::DatabaseModule),
    providers(crate::users::UsersService, crate::users::UsersRepository),
    controllers(crate::users::UsersController),
    exports(crate::users::UsersService)
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
