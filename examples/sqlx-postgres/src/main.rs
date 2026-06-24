//! SQLx Postgres provider registration example without opening a database connection.

use nidus::prelude::{Container, Inject, PgPoolOptions, injectable};

struct DatabaseOptions(PgPoolOptions);

#[injectable]
struct UsersRepository {
    #[allow(dead_code)]
    options: Inject<DatabaseOptions>,
}

fn container() -> Container {
    let mut container = Container::new();
    container
        .register_singleton(DatabaseOptions(PgPoolOptions::new()))
        .unwrap();
    UsersRepository::register_provider(&mut container).unwrap();
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
