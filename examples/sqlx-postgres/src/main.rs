use nidus::prelude::{Container, Inject, PgPoolOptions, ProviderLifetime};

struct DatabaseOptions(PgPoolOptions);

struct UsersRepository {
    #[allow(dead_code)]
    options: Inject<DatabaseOptions>,
}

fn container() -> Container {
    let mut container = Container::new();
    container
        .register_singleton(DatabaseOptions(PgPoolOptions::new()))
        .unwrap();
    container
        .register_factory(ProviderLifetime::Singleton, |container| {
            Ok(UsersRepository {
                options: container.inject::<DatabaseOptions>()?,
            })
        })
        .unwrap();
    container
}

fn main() {
    let container = container();
    let repository = container.resolve::<UsersRepository>().unwrap();
    let _options = &repository.options.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_repository_with_sqlx_options() {
        let container = container();

        let repository = container.resolve::<UsersRepository>().unwrap();

        let _options = &repository.options.0;
    }
}
