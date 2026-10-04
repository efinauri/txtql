//! Stress tests: scaling (on deterministic step counts, so they are not flaky), pathological
//! grammars, deep nesting, and big inputs.
//!
//! Wall-clock tests on multi-megabyte inputs are `#[ignore]`d; run them in release mode with
//! `cargo test --release --test stress -- --ignored`.

mod common;
use common::*;
use serde_json::json;
use std::time::{Duration, Instant};
use txtql::error::MatchError;
use txtql::{Options, Query, RunError};

fn steps(q: &Query, input: &str) -> u64 {
    match q.run(input, &Options::default()) {
        Ok(out) => {
            assert!(out.ambiguity_check_complete, "ambiguity check should finish");
            out.steps
        }
        Err(e) => panic!("{e:?}"),
    }
}

/// Asserts that 4x the input costs less than 6x the steps (linear is ~4x, quadratic ~16x).
#[track_caller]
fn assert_linear(query: &str, make_input: impl Fn(usize) -> String, n: usize) {
    let q = compile(query);
    let small = steps(&q, &make_input(n));
    let big = steps(&q, &make_input(4 * n));
    let ratio = big as f64 / small as f64;
    assert!(ratio < 6.0, "{query}\nsteps {small} -> {big} (ratio {ratio:.1}) is not linear");
}

fn rhymes(n: usize) -> String {
    let nouns = ["roses", "violets", "bees", "skies", "seas", "trees", "cats"];
    let colors = ["red", "blue and green", "black, white and grey"];
    (0..n).map(|i| format!("{} are {}\n", nouns[i % 7], colors[i % 3])).collect()
}

const RHYMES: &str =
    "color = WORD\ncolors = 1 TO n color SPLITBY (' and ' OR ', ')\nrhyme = noun:WORD ' are ' colors NL";

#[test]
fn rhyme_list_is_linear() {
    assert_linear(&format!("{RHYMES}\nTEXT = 1 TO n rhyme"), rhymes, 1000);
    assert_linear(&format!("{RHYMES} AS {{ noun: colors }}\nTEXT = 1 TO n rhyme AS {{ rhyme }}"), rhymes, 1000);
}

#[test]
fn list_followed_by_a_literal_is_linear() {
    // The repetition can end after any item; extraction must not build each prefix.
    assert_linear(&format!("{RHYMES}\nTEXT = 1 TO n xs:rhyme 'END'"), |n| rhymes(n) + "END", 1000);
}

#[test]
fn harmless_ambiguity_everywhere_is_linear() {
    // Every item is ambiguous, but never in a way that changes the output.
    assert_linear("TEXT = 1 TO n (WORD OR 1 TO 1 WORD) SPLITBY ' '", |n| vec!["w"; n].join(" "), 2000);
}

#[test]
fn skipping_is_linear() {
    assert_linear("TEXT = 1 TO n n:FLOAT SKIPPING ANY", |n| "noise 42 more , text ".repeat(n), 1000);
}

#[test]
fn line_based_extraction_is_linear() {
    assert_linear("TEXT = 1 TO n (LINE NL)", |n| "some words on a line\n".repeat(n), 1000);
    assert_linear(
        "field = k:(ANY UNTILBEFORE ':') ': ' v:(ANY UNTILBEFORE NL) NL AS { k: v }\nTEXT = 1 TO n field AS [ field FOR field ]",
        |n| "key name: some value here\n".repeat(n),
        1000,
    );
}

#[test]
fn nested_lists_are_linear() {
    assert_linear(
        "vec = '[' 1 TO n FLOAT SPLITBY ', ' ']'\nTEXT = 1 TO n vec SPLITBY ' ; '",
        |n| vec!["[1, 2, 3]"; n].join(" ; "),
        500,
    );
}

/// Runs `f` on another thread and fails if it takes longer than `limit`.
fn within<T: Send + 'static>(limit: Duration, f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).expect("did not finish in time")
}

fn limited(max_steps: u64) -> Options {
    Options { max_steps, ambiguity_steps: max_steps / 4, ..Options::default() }
}

#[test]
fn nested_unbounded_repetition_finishes_or_gives_up() {
    // Exponentially many ways to split the digits; must stay polynomial or stop cleanly.
    let q = compile("TEXT = 1 TO n 1 TO n p:DIGIT");
    for n in [50, 200, 800] {
        let input = "1".repeat(n);
        let q = q.clone();
        let r = within(Duration::from_secs(60), move || q.run(&input, &limited(5_000_000)).map(|o| o.value));
        match r {
            Ok(v) => assert_eq!(v.as_array().unwrap().len(), 1, "greedy: one item takes every digit"),
            Err(RunError::Match(MatchError::TooExpensive { .. })) => {}
            Err(e) => panic!("{e:?}"),
        }
    }
}

#[test]
fn highly_ambiguous_alternatives_finish() {
    let q = compile("TEXT = 1 TO n (DIGIT OR DIGIT DIGIT OR DIGIT DIGIT DIGIT)");
    let input = "1".repeat(3000);
    let out = within(Duration::from_secs(60), move || {
        q.run(&input, &limited(20_000_000)).map(|o| o.value.as_array().unwrap().len())
    });
    // Greedy: as many three-digit items as possible.
    assert_eq!(out.unwrap(), 1000);
}

#[test]
fn skipping_with_no_match_fails_quickly() {
    let q = compile("x = 'needle'\nTEXT = 1 TO n x SKIPPING ANY");
    let input = "hay ".repeat(20_000);
    let r = within(Duration::from_secs(60), move || q.run(&input, &Options::default()).map(|_| ()));
    assert!(matches!(r, Err(RunError::Match(MatchError::NoParse { .. }))));
}

#[test]
fn right_recursion_still_works() {
    // Quadratic (the lint says so), but correct.
    let q = compile("list = WORD ', ' list OR WORD\nTEXT = list");
    let input = vec!["w"; 1500].join(", ");
    assert!(q.run(&input, &Options::default()).is_ok());
}

#[test]
fn step_limit_error() {
    let q = compile("TEXT = 1 TO n LETTER");
    let e = q.run(&"w".repeat(100), &Options { max_steps: 50, ..Options::default() }).unwrap_err();
    assert!(matches!(e, RunError::Match(MatchError::TooExpensive { limit: 50 })));
    insta::assert_snapshot!(render(&e.reports("query", "TEXT = 1 TO n LETTER", "input", "w")));
}

// ---- deep nesting (runs on the default 2 MiB test thread stack) ----

const PARENS: &str = "p = '(' 0 TO 1 inner:p ')' AS [inner]\nTEXT = p";

#[test]
fn deep_nesting_is_stack_safe() {
    let q = compile(PARENS);
    let depth = 20_000;
    let input = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
    let out = q.run(&input, &Options { max_depth: 1_000_000, ..Options::default() }).unwrap();
    // Walk down the result to check its depth without recursion.
    let mut v = &out.value;
    let mut levels = 0;
    // Each level is `[inner]`, and the innermost is `[null]`.
    while let Some(inner) = v.as_array().and_then(|a| a.first()).filter(|x| x.is_array()) {
        v = inner;
        levels += 1;
    }
    assert_eq!(levels, depth - 1);
    txtql::drop_deep(out.value);
}

#[test]
fn nesting_limit_is_a_clean_error() {
    let q = compile(PARENS);
    let input = format!("{}{}", "(".repeat(50_000), ")".repeat(50_000));
    let e = q.run(&input, &Options::default()).unwrap_err();
    assert!(matches!(e, RunError::Match(MatchError::TooDeep { limit: 10_000, .. })), "{e:?}");
}

#[test]
fn deep_left_recursion() {
    let q = compile("sum = sum ' + ' FLOAT OR FLOAT\nTEXT = sum");
    let input = vec!["1"; 5000].join(" + ");
    let out = q.run(&input, &Options::default()).unwrap();
    txtql::drop_deep(out.value);
}

// ---- size extremes ----

#[test]
fn many_alternatives() {
    let words: Vec<String> = (0..500).map(|i| format!("w{i}x")).collect();
    let alts: Vec<String> = words.iter().map(|w| format!("'{w}'")).collect();
    let q = compile(&format!("TEXT = 1 TO n ({}) SPLITBY ' '", alts.join(" OR ")));
    let input = words.iter().rev().cloned().collect::<Vec<_>>().join(" ");
    assert_eq!(q.run(&input, &Options::default()).unwrap().value.as_array().unwrap().len(), 500);
}

#[test]
fn many_rules() {
    let mut src = String::new();
    for i in 0..300 {
        src.push_str(&format!("r{i} = r{} OR 'x{i}'\n", i + 1));
    }
    src.push_str("r300 = WORD\nTEXT = 1 TO n r0 SPLITBY ' '");
    let q = compile(&src);
    assert_eq!(q.run("x5 x250 hello", &Options::default()).unwrap().value, json!(["x5", "x250", "hello"]));
}

#[test]
fn one_huge_token() {
    let word = "a".repeat(1_000_000);
    let q = compile("TEXT = w:WORD");
    assert_eq!(q.run(&word, &Options::default()).unwrap().value["w"].as_str().unwrap().len(), 1_000_000);
}

#[test]
fn many_tiny_tokens() {
    let q = compile("TEXT = 0 TO n PUNCT");
    let input = ",".repeat(200_000);
    assert_eq!(q.run(&input, &Options::default()).unwrap().value.as_array().unwrap().len(), 200_000);
}

// ---- wall clock (release only) ----

fn timed(query: &str, input: &str) -> Duration {
    let q = compile(query);
    let start = Instant::now();
    let out = q.run(input, &Options::default()).unwrap();
    let elapsed = start.elapsed();
    txtql::drop_deep(out.value);
    elapsed
}

#[test]
#[ignore = "slow in debug builds; run with --release -- --ignored"]
fn ten_megabytes_of_rhymes() {
    let mut input = String::new();
    while input.len() < 10_000_000 {
        input.push_str(&rhymes(1000));
    }
    let t = timed(&format!("{RHYMES} AS {{ noun: colors }}\nTEXT = 1 TO n rhyme AS {{ rhyme }}"), &input);
    eprintln!("10 MB of rhymes: {t:?}");
    assert!(t < Duration::from_secs(60), "{t:?}");
}

#[test]
#[ignore = "slow in debug builds; run with --release -- --ignored"]
fn ten_megabytes_of_log_lines() {
    let line = "2024-01-01 12:00:01 [INFO] server started port=8080\n2024-01-01 12:00:02 [DEBUG] config loaded\n";
    let input = line.repeat(10_000_000 / line.len());
    let q = "date = FLOAT '-' FLOAT '-' FLOAT\ntime = FLOAT ':' FLOAT ':' FLOAT\n\
             entry = d:date ' ' t:time ' [' level:WORD '] ' msg:(ANY UNTILBEFORE NL) NL WHERE level != 'DEBUG' AS { 'level': level, 'msg': msg }\n\
             TEXT = 1 TO n entry SKIPPING (LINE NL) AS [ entry FOR entry ]";
    let t = timed(q, &input);
    eprintln!("10 MB of log lines: {t:?}");
    assert!(t < Duration::from_secs(60), "{t:?}");
}

// ---- runtime safety without the static checks ----

/// Found by fuzzing: with an empty cycle (`TEXT` deriving itself over no text), an alternative
/// derivation could contain the node it replaced, and building its value recursed forever.
/// The checker rejects such grammars; the runtime must stay safe anyway.
#[test]
fn unchecked_empty_cycles_stay_bounded() {
    let src = "TEXT = (3 TO 6 ((((1 TO 4 LAZY (((ANY)))) OR (ANY OR TEXT OR (2 TO 5 (('a')))) OR (ANY UNTILBEFORE ',')) \
               OR (ANY OR TEXT OR (2 TO 5 (FLOAT) SPLITBY 'a') OR (0 TO n ((0 TO n ((0 TO n ((0 TO n (WORD))))))))))))";
    assert!(txtql::Query::compile(src).is_err(), "the checker rejects it");
    let q = txtql::Query::compile_unchecked(src).unwrap();
    for input in ["", "a , 1 x", "a a a a a a a a"] {
        let q = q.clone();
        let input = input.to_string();
        let r = within(Duration::from_secs(30), move || q.run(&input, &limited(200_000)).map(|o| o.steps));
        assert!(matches!(r, Ok(_) | Err(RunError::Match(MatchError::TooExpensive { .. }))), "{r:?}");
    }
}

#[test]
fn unchecked_nullable_loops_stay_bounded() {
    for src in [
        "TEXT = 0 TO n (0 TO n WORD)",
        "TEXT = 1 TO n (TEXT OR 0 TO 1 WORD)",
        "a = b OR WORD\nb = a OR 0 TO 1 FLOAT\nTEXT = 1 TO n a",
        "TEXT = 1 TO n (ANY UNTILBEFORE ',') SKIPPING (0 TO n PUNCT)",
    ] {
        let q = txtql::Query::compile_unchecked(src).unwrap();
        for input in ["", "a b 1 , c", "1 2 3 4 5 6 7 8 9"] {
            let (q, input) = (q.clone(), input.to_string());
            let r = within(Duration::from_secs(30), move || q.run(&input, &limited(200_000)).map(|o| o.steps));
            assert!(
                matches!(r, Ok(_) | Err(RunError::Match(MatchError::TooExpensive { .. } | MatchError::NoParse { .. }))),
                "{src}: {r:?}"
            );
        }
    }
}
