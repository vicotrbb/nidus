use nidus::prelude::*;

#[derive(Debug)]
struct Database(&'static str);

#[injectable]
#[derive(Debug)]
struct UsersRepository {
    database: Inject<Database>,
}

fn main() {
    let mut container = Container::new();
    container.register_singleton(Database("primary")).unwrap();
    UsersRepository::register_provider(&mut container).unwrap();

    let repository = container.resolve::<UsersRepository>().unwrap();
    assert_eq!(repository.database.0, "primary");
}
