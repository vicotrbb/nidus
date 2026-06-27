use nidus_core::ModuleBuilder;
use nidus_sqlx::{PostgresPoolProvider, SqlitePoolProvider};

fn main() {
    let _module = ModuleBuilder::new("DatabaseModule")
        .provider_typed::<SqlitePoolProvider>()
        .provider_typed::<PostgresPoolProvider>()
        .build();
}
