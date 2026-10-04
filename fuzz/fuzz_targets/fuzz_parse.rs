//! Arbitrary bytes as a query. See `txtql_fuzz::check_parse`.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| txtql_fuzz::check_parse(data));
