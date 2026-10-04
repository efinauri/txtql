//! Structured grammars and inputs. See `txtql_fuzz::check_run`.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|case: txtql_fuzz::Case| {
    txtql_fuzz::check_run(&case);
});
