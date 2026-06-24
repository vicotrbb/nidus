use nidus::prelude::*;

#[injectable]
struct TupleRepository(Inject<Database>);

struct Database;

fn main() {}
