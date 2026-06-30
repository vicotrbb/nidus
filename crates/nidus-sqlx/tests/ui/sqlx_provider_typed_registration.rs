use nidus_core::ModuleBuilder;
use nidus_sqlx::SqlitePoolProvider;

fn main() {
    let _module = ModuleBuilder::new("DatabaseModule")
        .provider_typed::<SqlitePoolProvider>()
        .build();
}
