use std::sync::Arc;

use nidus_core::{ModuleBuilder, ModuleGraph, NidusError};
use tracing::Level;
use tracing_subscriber::{Layer, fmt::MakeWriter, layer::SubscriberExt};

#[derive(Clone, Default)]
struct SharedLogWriter {
    output: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl SharedLogWriter {
    fn contents(&self) -> String {
        String::from_utf8(self.output.lock().unwrap().clone()).unwrap()
    }

    fn clear(&self) {
        self.output.lock().unwrap().clear();
    }
}

impl<'writer> MakeWriter<'writer> for SharedLogWriter {
    type Writer = SharedLogGuard;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogGuard {
            output: Arc::clone(&self.output),
        }
    }
}

struct SharedLogGuard {
    output: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl std::io::Write for SharedLogGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn module_builder_records_explicit_imports_providers_controllers_and_exports() {
    let definition = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .provider("UsersRepository")
        .provider("UsersService")
        .controller("UsersController")
        .export("UsersService")
        .build();

    assert_eq!(definition.name(), "UsersModule");
    assert_eq!(definition.imports(), ["DatabaseModule"]);
    assert_eq!(definition.providers(), ["UsersRepository", "UsersService"]);
    assert_eq!(definition.controllers(), ["UsersController"]);
    assert_eq!(definition.exports(), ["UsersService"]);
}

#[test]
fn module_graph_detects_circular_imports() {
    let users = ModuleBuilder::new("UsersModule")
        .import("BillingModule")
        .build();
    let billing = ModuleBuilder::new("BillingModule")
        .import("UsersModule")
        .build();

    let error = ModuleGraph::from_modules([users, billing]).unwrap_err();

    assert!(matches!(error, NidusError::CircularModuleImport { .. }));
}

#[test]
fn module_graph_rejects_duplicate_module_names() {
    let first = ModuleBuilder::new("UsersModule")
        .provider("UsersService")
        .build();
    let second = ModuleBuilder::new("UsersModule")
        .provider("UsersRepository")
        .build();

    let error = ModuleGraph::from_modules([first, second]).unwrap_err();

    assert!(matches!(error, NidusError::DuplicateModule { .. }));
    assert!(error.to_string().contains("UsersModule"));
}

#[test]
fn module_graph_iterates_modules_in_name_order() {
    let users = ModuleBuilder::new("UsersModule").build();
    let auth = ModuleBuilder::new("AuthModule").build();
    let billing = ModuleBuilder::new("BillingModule").build();

    let graph = ModuleGraph::from_modules([users, auth, billing]).unwrap();
    let module_names = graph
        .modules()
        .map(|module| module.name())
        .collect::<Vec<_>>();

    assert_eq!(module_names, ["AuthModule", "BillingModule", "UsersModule"]);
}

#[test]
fn module_graph_reports_validation_errors_in_module_name_order() {
    let users = ModuleBuilder::new("UsersModule")
        .provider("UsersService")
        .provider("UsersService")
        .build();
    let auth = ModuleBuilder::new("AuthModule")
        .provider("AuthService")
        .provider("AuthService")
        .build();

    let error = ModuleGraph::from_modules([users, auth]).unwrap_err();

    assert!(matches!(error, NidusError::DuplicateModuleProvider { .. }));
    assert!(error.to_string().contains("AuthModule"));
    assert!(error.to_string().contains("AuthService"));
}

#[test]
fn module_graph_rejects_duplicate_local_providers() {
    let users = ModuleBuilder::new("UsersModule")
        .provider("UsersService")
        .provider("UsersService")
        .build();

    let error = ModuleGraph::from_modules([users]).unwrap_err();

    assert!(matches!(error, NidusError::DuplicateModuleProvider { .. }));
    assert!(error.to_string().contains("UsersModule"));
    assert!(error.to_string().contains("UsersService"));
}

#[test]
fn module_graph_rejects_duplicate_local_controllers() {
    let users = ModuleBuilder::new("UsersModule")
        .controller("UsersController")
        .controller("UsersController")
        .build();

    let error = ModuleGraph::from_modules([users]).unwrap_err();

    assert!(matches!(
        error,
        NidusError::DuplicateModuleController { .. }
    ));
    assert!(error.to_string().contains("UsersModule"));
    assert!(error.to_string().contains("UsersController"));
}

#[test]
fn module_graph_rejects_provider_controller_name_conflicts() {
    let users = ModuleBuilder::new("UsersModule")
        .provider("UsersController")
        .controller("UsersController")
        .build();

    let error = ModuleGraph::from_modules([users]).unwrap_err();

    assert!(matches!(
        error,
        NidusError::ModuleProviderControllerConflict { .. }
    ));
    assert!(error.to_string().contains("UsersModule"));
    assert!(error.to_string().contains("UsersController"));
}

#[test]
fn module_graph_rejects_duplicate_imports() {
    let database = ModuleBuilder::new("DatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let users = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .import("DatabaseModule")
        .build();

    let error = ModuleGraph::from_modules([database, users]).unwrap_err();

    assert!(matches!(error, NidusError::DuplicateModuleImport { .. }));
    assert!(error.to_string().contains("UsersModule"));
    assert!(error.to_string().contains("DatabaseModule"));
}

#[test]
fn module_graph_rejects_duplicate_exports() {
    let users = ModuleBuilder::new("UsersModule")
        .provider("UsersService")
        .export("UsersService")
        .export("UsersService")
        .build();

    let error = ModuleGraph::from_modules([users]).unwrap_err();

    assert!(matches!(error, NidusError::DuplicateModuleExport { .. }));
    assert!(error.to_string().contains("UsersModule"));
    assert!(error.to_string().contains("UsersService"));
}

#[test]
fn module_graph_rejects_exports_that_are_not_local_providers() {
    let users = ModuleBuilder::new("UsersModule")
        .provider("UsersRepository")
        .export("UsersService")
        .build();

    let error = ModuleGraph::from_modules([users]).unwrap_err();

    assert!(matches!(error, NidusError::MissingProviderExport { .. }));
    assert!(error.to_string().contains("UsersService"));
}

#[test]
fn module_graph_rejects_local_providers_that_conflict_with_imported_exports() {
    let database = ModuleBuilder::new("DatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let users = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .provider("DatabasePool")
        .build();

    let error = ModuleGraph::from_modules([database, users]).unwrap_err();

    assert!(matches!(
        error,
        NidusError::ProviderVisibilityConflict { .. }
    ));
    assert!(error.to_string().contains("UsersModule"));
    assert!(error.to_string().contains("DatabasePool"));
    assert!(error.to_string().contains("DatabaseModule"));
}

#[test]
fn module_graph_rejects_ambiguous_visible_providers() {
    let database_a = ModuleBuilder::new("PrimaryDatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let database_b = ModuleBuilder::new("ReplicaDatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let users = ModuleBuilder::new("UsersModule")
        .import("PrimaryDatabaseModule")
        .import("ReplicaDatabaseModule")
        .build();

    let error = ModuleGraph::from_modules([database_a, database_b, users]).unwrap_err();

    assert!(matches!(error, NidusError::AmbiguousProvider { .. }));
    assert!(error.to_string().contains("DatabasePool"));
}

#[test]
fn module_graph_emits_debug_logs() {
    let writer = SharedLogWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer.clone())
            .with_ansi(false)
            .with_target(false)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                Level::DEBUG,
            )),
    );
    let database = ModuleBuilder::new("DatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let users = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .provider("UsersService")
        .build();

    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..16 {
            writer.clear();
            tracing_core::callsite::rebuild_interest_cache();
            ModuleGraph::from_modules([database.clone(), users.clone()]).unwrap();
            let logs = writer.contents();
            if logs.contains("validating module graph")
                && logs.contains("module graph node")
                && logs.contains("UsersModule")
                && logs.contains("module graph validated")
            {
                return;
            }
            std::thread::yield_now();
        }
    });

    let logs = writer.contents();
    assert!(logs.contains("validating module graph"));
    assert!(logs.contains("module graph node"));
    assert!(logs.contains("UsersModule"));
    assert!(logs.contains("module graph validated"));
}
