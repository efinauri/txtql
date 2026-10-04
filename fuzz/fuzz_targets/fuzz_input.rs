//! Arbitrary bytes as input text. See `txtql_fuzz::check_input`.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| txtql_fuzz::check_input(data));
