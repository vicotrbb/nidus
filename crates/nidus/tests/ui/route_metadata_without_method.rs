use nidus::prelude::*;

struct UsersController;
struct AuthGuard;

#[routes]
impl UsersController {
    #[guard(AuthGuard)]
    async fn helper(&self) {}
}

fn main() {}
