#[nidus::main]
async fn run_on_nidus_runtime(value: usize) -> usize {
    value + 1
}

fn main() {
    assert_eq!(run_on_nidus_runtime(41), 42);
}
