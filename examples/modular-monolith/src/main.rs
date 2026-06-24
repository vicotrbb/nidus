//! Macro-defined module graph example for a modular monolith shape.

use nidus::prelude::*;

#[derive(Debug)]
struct DatabasePool {
    dsn: &'static str,
}

impl ProviderRegistrant for DatabasePool {
    fn register_provider(_container: &mut Container) -> Result<()> {
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct UserProfile {
    id: i64,
    email: String,
    tenant: &'static str,
}

#[injectable]
#[derive(Debug)]
struct AuditLog;

impl AuditLog {
    fn record(&self, event: &str) -> String {
        format!("audit:{event}")
    }
}

#[injectable]
#[derive(Debug)]
struct UsersRepository {
    pool: Inject<DatabasePool>,
}

impl UsersRepository {
    fn find_profile(&self, id: i64) -> UserProfile {
        UserProfile {
            id,
            email: format!("user-{id}@nidus.dev"),
            tenant: self.pool.dsn,
        }
    }
}

#[injectable]
#[derive(Debug)]
struct UsersService {
    repository: Inject<UsersRepository>,
    audit_log: Inject<AuditLog>,
}

impl UsersService {
    fn profile(&self, id: i64) -> UserProfile {
        let _event = self.audit_log.record("users.profile");
        self.repository.find_profile(id)
    }
}

#[controller("/users")]
struct UsersController {
    service: Inject<UsersService>,
}

#[routes]
impl UsersController {
    #[get("/:id")]
    async fn profile(&self, Path(id): Path<i64>) -> String {
        self.service.profile(id).email
    }
}

#[module]
struct InfrastructureModule {
    providers: [DatabasePool],
    exports: [DatabasePool],
}

#[module]
struct AuditModule {
    providers: [AuditLog],
    exports: [AuditLog],
}

#[module]
struct UsersModule {
    imports: (InfrastructureModule, AuditModule),
    providers: (UsersRepository, UsersService),
    controllers: [UsersController],
    exports: [UsersService],
}

#[module]
struct AppModule {
    imports: [UsersModule],
}

fn build_graph() -> Result<ModuleGraph> {
    ModuleGraph::from_modules([
        InfrastructureModule::definition(),
        AuditModule::definition(),
        UsersModule::definition(),
        AppModule::definition(),
    ])
}

fn build_container() -> Result<Container> {
    let mut container = Container::new();
    container.register_singleton(DatabasePool {
        dsn: "tenant-primary",
    })?;
    AuditLog::register_provider(&mut container)?;
    UsersRepository::register_provider(&mut container)?;
    UsersService::register_provider(&mut container)?;
    Ok(container)
}

fn main() {
    let graph = build_graph().unwrap();
    let users = graph.get("UsersModule").unwrap();
    println!(
        "module={} imports={:?} providers={:?} controllers={:?} exports={:?}",
        users.name(),
        users.imports(),
        users.providers(),
        users.controllers(),
        users.exports()
    );

    let container = build_container().unwrap();
    let profile = container.resolve::<UsersService>().unwrap().profile(42);
    println!("resolved user {} from {}", profile.email, profile.tenant);
}

#[cfg(test)]
mod tests {
    use super::*;
    use nidus::prelude::NidusError;

    #[test]
    fn graph_contains_realistic_module_boundaries() {
        let graph = build_graph().unwrap();

        let users = graph.get("UsersModule").unwrap();
        assert_eq!(users.imports(), ["InfrastructureModule", "AuditModule"]);
        assert_eq!(users.providers(), ["UsersRepository", "UsersService"]);
        assert_eq!(users.controllers(), ["UsersController"]);
        assert_eq!(users.exports(), ["UsersService"]);

        let app = graph.get("AppModule").unwrap();
        assert_eq!(app.imports(), ["UsersModule"]);
    }

    #[test]
    fn graph_validation_requires_declared_imports_to_exist() {
        let error = ModuleGraph::from_modules([UsersModule::definition()]).unwrap_err();

        assert!(matches!(error, NidusError::MissingModuleImport { .. }));
        assert!(error.to_string().contains("InfrastructureModule"));
    }

    #[test]
    fn container_resolves_exported_user_service_dependencies() {
        let container = build_container().unwrap();

        let profile = container.resolve::<UsersService>().unwrap().profile(42);

        assert_eq!(
            profile,
            UserProfile {
                id: 42,
                email: "user-42@nidus.dev".to_owned(),
                tenant: "tenant-primary",
            }
        );
    }
}
