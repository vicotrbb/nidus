//! Command-line tooling for generating and inspecting Nidus projects.

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command as ProcessCommand,
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

mod artifact_template;
mod check;
mod generate;
mod generate_name;
mod graph;
mod graph_metadata;
mod openapi_doc;
mod route_order;
mod route_path;
mod routes;
mod schema;
mod source_files;
mod source_openapi;

use check::check_project;
use generate::{create_project, generate_artifact};
use graph::inspect_graph;
use openapi_doc::{OpenApiOptions, generate_openapi};
use routes::inspect_routes;

#[derive(Debug, Parser)]
#[command(name = "cargo-nidus", bin_name = "cargo nidus")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new Nidus application.
    New {
        /// Project name.
        name: String,
        /// Directory where the project folder should be created.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Local path to the nidus facade crate, used by tests and unreleased development builds.
        #[arg(long, hide = true)]
        nidus_path: Option<PathBuf>,
    },
    /// Generate a framework artifact.
    Generate {
        /// Artifact kind: module, controller, service, or repository.
        kind: String,
        /// Artifact name.
        name: String,
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print route metadata.
    Routes {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print dependency graph metadata.
    Graph {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Print expanded generated code guidance.
    Expand {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Print the cargo-expand command without running it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Check project structure.
    Check {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
    },
    /// Generate OpenAPI output.
    Openapi {
        /// Project root.
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// OpenAPI document title.
        #[arg(long, default_value = "Nidus API")]
        title: String,
        /// OpenAPI document version.
        #[arg(long, default_value = "0.1.0")]
        version: String,
    },
}

fn main() -> Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    let args = if args.get(1).is_some_and(|arg| arg == "nidus") {
        let mut stripped = Vec::with_capacity(args.len() - 1);
        stripped.push(args[0].clone());
        stripped.extend(args.iter().skip(2).cloned());
        stripped
    } else {
        args
    };
    let cli = Cli::parse_from(args);

    match cli.command {
        Command::New {
            name,
            path,
            nidus_path,
        } => create_project(&name, &path, nidus_path.as_deref()),
        Command::Generate { kind, name, path } => generate_artifact(&kind, &name, &path),
        Command::Routes { path } => inspect_routes(&path),
        Command::Graph { path } => inspect_graph(&path),
        Command::Expand { path, dry_run } => expand_project(&path, dry_run),
        Command::Check { path } => check_project(&path),
        Command::Openapi {
            path,
            title,
            version,
        } => generate_openapi(&path, &OpenApiOptions { title, version }),
    }
}

fn expand_project(root: &Path, dry_run: bool) -> Result<()> {
    let manifest = root.join("Cargo.toml");
    if !manifest.exists() {
        bail!(
            "Nidus expand failed for {}. Missing required file: Cargo.toml",
            root.display()
        );
    }

    if dry_run {
        println!("cargo expand --manifest-path {}", manifest.display());
        return Ok(());
    }

    let output = ProcessCommand::new("cargo")
        .arg("expand")
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .with_context(|| "running cargo expand")?;
    if output.status.success() {
        io::stdout().write_all(&output.stdout)?;
        io::stderr().write_all(&output.stderr)?;
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("no such command") && stderr.contains("expand") {
        bail!(
            "cargo-expand is not installed. Install it with `cargo install cargo-expand`, then rerun `cargo nidus expand --path {}`",
            root.display()
        );
    }

    bail!(
        "cargo expand failed for {}\n{}",
        root.display(),
        stderr.trim()
    );
}
