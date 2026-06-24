use nidus::prelude::{Module, ModuleBuilder, ModuleDefinition};

pub struct DatabaseModule;

impl Module for DatabaseModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("DatabaseModule")
            .provider_typed::<crate::db::Database>()
            .export_typed::<crate::db::Database>()
            .async_initializer(crate::db::initialize_database)
            .build()
    }
}
