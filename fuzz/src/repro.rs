//! Prints the query and input a `fuzz_run` artifact decodes to, then runs it.
//!
//! Usage: `cargo run --release --bin repro -- fuzz/artifacts/fuzz_run/<file>`

use arbitrary::{Arbitrary, Unstructured};

fn main() {
    let path = std::env::args().nth(1).expect("path to an artifact");
    let data = std::fs::read(path).unwrap();
    let case = txtql_fuzz::Case::arbitrary(&mut Unstructured::new(&data)).unwrap();
    let (query, input) = txtql_fuzz::render(&case);
    println!("--- query\n{query}--- input\n{input:?}");
    let start = std::time::Instant::now();
    txtql_fuzz::check_run(&case);
    println!("--- ok in {:?}", start.elapsed());
}
