use nidus::prelude::*;

#[injectable(transient)]
#[derive(Debug)]
struct GenericRepository<T>
where
    T: Default + Send + Sync + 'static,
{
    value: T,
}

fn main() {
    let mut container = Container::new();
    GenericRepository::<String>::register_provider(&mut container).unwrap();

    let repository = container.resolve::<GenericRepository<String>>().unwrap();
    assert_eq!(repository.value, String::default());
}
