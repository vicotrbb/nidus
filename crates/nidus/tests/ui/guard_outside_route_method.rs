use nidus::prelude::*;

struct AuthGuard;

#[guard(AuthGuard)]
fn guarded() {}

fn main() {}
