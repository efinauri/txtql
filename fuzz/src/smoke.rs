//! Stable-toolchain smoke fuzzer: feeds random bytes through the same checks as the libFuzzer
//! targets. Not coverage-guided; use `cargo +nightly fuzz run <target>` for real fuzzing.
//!
//! Usage: `cargo run --release --bin smoke -- [seconds] [seed]`

use arbitrary::{Arbitrary, Unstructured};
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<u64> = std::env::args().skip(1).map(|a| a.parse().expect("numeric argument")).collect();
    let seconds = args.first().copied().unwrap_or(30);
    let mut state = args.get(1).copied().unwrap_or(0x5eed) | 1;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let trace = std::env::var_os("SMOKE_TRACE").is_some();
    let (mut runs, mut compiled) = (0u64, 0u64);
    let mut buf = Vec::new();
    while Instant::now() < deadline {
        let len = (next() % 512) as usize;
        buf.clear();
        buf.extend((0..len).map(|_| next() as u8));
        txtql_fuzz::check_input(&buf);
        txtql_fuzz::check_parse(&buf);
        if let Ok(case) = txtql_fuzz::Case::arbitrary(&mut Unstructured::new(&buf))
            && {
                // `SMOKE_TRACE=1` prints every query before running it, to find one that hangs.
                if trace {
                    eprintln!("---\n{}", txtql_fuzz::query(&case));
                }
                txtql_fuzz::check_run(&case)
            }
        {
            compiled += 1;
        }
        runs += 1;
    }
    println!("{runs} cases, {compiled} structured queries compiled and ran, no failures");
}
