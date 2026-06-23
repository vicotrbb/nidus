use nidus::prelude::*;

#[controller("/users")]
struct UsersController;

#[test]
fn controller_macro_exposes_prefix_metadata() {
    assert_eq!(UsersController::controller_prefix(), "/users");
}
