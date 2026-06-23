use nidus_core::{Container, Inject};
use sqlx::postgres::PgPoolOptions;

struct DatabaseOptions(PgPoolOptions);

struct UsersRepository {
    #[allow(dead_code)]
    options: Inject<DatabaseOptions>,
}

fn main() {
    let mut container = Container::new();
    container
        .register_singleton(DatabaseOptions(PgPoolOptions::new()))
        .unwrap();
    container
        .register_factory(nidus_core::ProviderLifetime::Singleton, |container| {
            Ok(UsersRepository {
                options: container.inject::<DatabaseOptions>()?,
            })
        })
        .unwrap();
    let repository = container.resolve::<UsersRepository>().unwrap();
    let _options = &repository.options.0;
}
