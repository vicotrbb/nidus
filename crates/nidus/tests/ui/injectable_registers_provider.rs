use nidus::prelude::*;

#[derive(Debug)]
struct Database(&'static str);
#[derive(Debug)]
struct Cache;

#[injectable]
#[derive(Debug)]
struct UsersRepository {
    database: Inject<Database>,
    cache: Optional<Cache>,
}

fn main() {
    let mut container = Container::new();
    container.register_singleton(Database("primary")).unwrap();
    UsersRepository::register_provider(&mut container).unwrap();

    let repository = container.resolve::<UsersRepository>().unwrap();
    assert_eq!(repository.database.0, "primary");
    assert!(repository.cache.is_none());
}
