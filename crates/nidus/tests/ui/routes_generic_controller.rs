use nidus::prelude::*;

#[controller("/users")]
struct UsersController<S> {
    service: S,
}

#[routes]
impl<S> UsersController<S>
where
    S: Clone + Send + Sync + 'static,
{
    #[get("/:id")]
    async fn find_one(&self) {
        let _service = self.service.clone();
    }
}

fn main() {
    let routes = UsersController::<String>::routes();
    assert_eq!(routes[0].method(), "GET");
    assert_eq!(routes[0].path(), "/:id");
}
