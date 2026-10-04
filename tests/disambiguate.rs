//! Disambiguation rules and ambiguity reporting. Most examples split digit strings, where
//! every character is a DIGIT and there are many ways to divide the text.

mod common;
use common::*;
use insta::assert_snapshot;
use serde_json::json;
use txtql::{Options, RunError};

fn out(q: &str, input: &str) -> (serde_json::Value, usize) {
    let o = run_opts(q, input, &Options::default());
    (o.value, o.ambiguities.len())
}

// ---- rule 1: earlier parts of a sequence take as much as they can ----

#[test]
fn earlier_parts_are_greedy() {
    let (v, amb) = out("TEXT = 1 TO n first:DIGIT 1 TO n rest:DIGIT", "1234");
    assert_eq!(v, json!({"first": ["1", "2", "3"], "rest": ["4"]}));
    assert_eq!(amb, 1, "the split is ambiguous and changes the output");
}

#[test]
fn lazy_parts_take_as_little_as_they_can() {
    let (v, amb) = out("TEXT = 1 TO n LAZY first:DIGIT 1 TO n rest:DIGIT", "1234");
    assert_eq!(v, json!({"first": ["1"], "rest": ["2", "3", "4"]}));
    assert_eq!(amb, 0, "LAZY is an explicit choice, not an ambiguity");
}

#[test]
fn greedy_iterations_are_longest_first() {
    let (v, _) = out("TEXT = 1 TO n xs:(DIGIT OR DIGIT DIGIT)", "123");
    assert_eq!(v, json!(["12", "3"]));
}

#[test]
fn lazy_until_the_next_literal() {
    let (v, amb) = out("TEXT = 1 TO n LAZY key:ANY ':' 1 TO n value:ANY", "ab:c:d");
    assert_eq!(v, json!({"key": ["a", "b"], "value": ["c", ":", "d"]}));
    assert_eq!(amb, 0);
}

// ---- rule 2: ordered OR among alternatives covering the same text ----

#[test]
fn first_alternative_wins() {
    let (v, amb) = out("TEXT = n:FLOAT OR w:ANY", "5");
    assert_eq!(v, json!({"n": "5"}));
    assert_eq!(amb, 1);
}

#[test]
fn longer_match_beats_alternative_order() {
    // The OR group is a single step of the sequence; it takes the longest span first.
    let (v, _) = out("TEXT = x:('1' OR '1' '2') 0 TO n rest:DIGIT", "123");
    assert_eq!(v, json!({"x": "12", "rest": ["3"]}));
}

// ---- rule 3: repetitions prefer another iteration ----

#[test]
fn optional_prefers_to_match() {
    let (v, amb) = out("TEXT = 0 TO 1 x:DIGIT 0 TO 1 y:DIGIT", "1");
    assert_eq!(v, json!({"x": "1", "y": null}));
    assert_eq!(amb, 1);
}

#[test]
fn lazy_optional_prefers_not_to_match() {
    let (v, amb) = out("TEXT = 0 TO 1 LAZY x:DIGIT 0 TO 1 y:DIGIT", "1");
    assert_eq!(v, json!({"x": null, "y": "1"}));
    assert_eq!(amb, 0);
}

// ---- skipping ----

#[test]
fn skipping_prefers_matches_and_never_warns() {
    let (v, amb) = out("w = n:FLOAT ' kg' AS NUM(n)\nTEXT = 1 TO n w SKIPPING ANY", "x 12 kg y 3 kg 5 z");
    assert_eq!(v, json!([12, 3]));
    assert_eq!(amb, 0);
}

#[test]
fn skipping_leading_and_trailing_text() {
    let (v, _) = out("TEXT = 1 TO n FLOAT SKIPPING ANY", ". . 1 x 2 . .");
    assert_eq!(v, json!(["1", "2"]));
}

// ---- only output-changing ambiguity is reported ----

#[test]
fn identical_output_is_not_ambiguous() {
    let (v, amb) = out("TEXT = 1 TO n (DIGIT OR '1' OR '2')", "12");
    assert_eq!(v, json!(["1", "2"]));
    assert_eq!(amb, 0);
}

#[test]
fn ambiguity_in_uncaptured_text_is_not_reported() {
    let (v, amb) = out("TEXT = 1 TO n (DIGIT OR DIGIT DIGIT) '.' n:WORD", "123.x");
    assert_eq!(v, json!({"n": "x"}));
    assert_eq!(amb, 0);
}

#[test]
fn ambiguity_hidden_by_an_enclosing_template_is_not_reported() {
    // The inner split changes `inner`'s value, but the outer template ignores it.
    let (v, amb) = out("inner = 1 TO n a:DIGIT 1 TO n b:DIGIT\nTEXT = i:inner AS 'constant'", "123");
    assert_eq!(v, json!("constant"));
    assert_eq!(amb, 0);
}

#[test]
fn ambiguity_in_separators_is_not_reported() {
    let (v, amb) = out("TEXT = 1 TO n x:WORD SPLITBY (', ' OR ',' ' ')", "a, b");
    // A body that is a single repetition gives the list of its items.
    assert_eq!(v, json!(["a", "b"]));
    assert_eq!(amb, 0);
}

#[test]
fn nested_rule_ambiguity_names_the_rule() {
    let o = run_opts(
        "pair = 1 TO n a:DIGIT 1 TO n b:DIGIT\nTEXT = 1 TO n pair SPLITBY ';'",
        "123;456",
        &Options::default(),
    );
    assert_eq!(o.ambiguities.len(), 2);
    assert!(o.ambiguities.iter().all(|a| a.rule == "pair"));
}

#[test]
fn ambiguity_warning_rendering() {
    let q = "TEXT = 1 TO n first:DIGIT 1 TO n rest:DIGIT";
    let input = "1234";
    let o = run_opts(q, input, &Options::default());
    assert_snapshot!(render(&txtql::ambiguity_reports(&o.ambiguities, "input", input)));
}

// ---- strictness ----

#[test]
fn strict_option_turns_ambiguity_into_an_error() {
    let q = compile("TEXT = 1 TO n first:DIGIT 1 TO n rest:DIGIT");
    let err = q.run("123", &Options { strict: true, ..Options::default() }).unwrap_err();
    let RunError::Ambiguous(ambs) = err else { panic!("{err:?}") };
    assert_eq!(ambs.len(), 1);
    assert!(ambs[0].strict);
}

#[test]
fn strict_rule() {
    let q =
        "STRICT pair = 1 TO n a:LETTER 1 TO n b:LETTER\nloose = 1 TO n c:DIGIT 1 TO n d:DIGIT\nTEXT = pair ';' loose";
    // Only the STRICT rule's ambiguity is an error.
    let err = run_error(q, "xyz;123");
    let RunError::Ambiguous(ambs) = err else { panic!("{err:?}") };
    assert_eq!(ambs.len(), 1);
    assert_eq!(ambs[0].rule, "pair");
    // The loose rule alone only warns.
    let (_, amb) = out(q, "xy;123");
    assert_eq!(amb, 1);
}

#[test]
fn ambiguity_check_can_be_disabled() {
    let o = run_opts(
        "TEXT = 1 TO n first:DIGIT 1 TO n rest:DIGIT",
        "123",
        &Options { check_ambiguity: false, ..Options::default() },
    );
    assert!(o.ambiguities.is_empty());
    assert!(!o.ambiguity_check_complete);
}

#[test]
fn ambiguity_check_budget_stops_quietly() {
    let o = run_opts(
        "TEXT = 1 TO n 1 TO n p:DIGIT",
        &"1".repeat(200),
        &Options { ambiguity_steps: 50, ..Options::default() },
    );
    assert!(!o.ambiguity_check_complete);
}

// ---- WHERE takes part in disambiguation ----

#[test]
fn where_rejects_a_reading_and_the_next_one_is_used() {
    let (v, _) = out("TEXT = 1 TO n x:DIGIT 1 TO n y:DIGIT WHERE COUNT(x) = 1", "123");
    assert_eq!(v, json!({"x": ["1"], "y": ["2", "3"]}));
}

#[test]
fn where_falls_back_to_the_next_alternative() {
    let (v, amb) = out("big = n:FLOAT WHERE NUM(n) > 10\nTEXT = 1 TO n (b:big OR s:FLOAT) SPLITBY ' '", "5 20 7");
    assert_eq!(v, json!([{"s": "5"}, {"b": {"n": "20"}}, {"s": "7"}]));
    // `20` can also be read as a small number, which changes the output.
    assert_eq!(amb, 1);
}

#[test]
fn where_rejecting_everything_is_an_error() {
    let e = run_err("TEXT = x:WORD WHERE x = 'nope'", "hello");
    assert!(e.contains("no reading satisfies the WHERE conditions"), "{e}");
}

// ---- alternatives that fail to evaluate are still ambiguity ----

#[test]
fn alternative_failing_evaluation_is_reported_not_fatal() {
    // Chosen: a = [1, 2], b = [3], which ZIPs fine. The alternative a = [1], b = [2, 3]
    // gives ZIP more values than keys, which is an evaluation error.
    let q = "TEXT = 1 TO n a:DIGIT 1 TO n b:DIGIT AS ZIP(a, b)";
    let o = run_opts(q, "123", &Options::default());
    assert_eq!(o.value, json!({"1": "3", "2": null}));
    assert_eq!(o.ambiguities.len(), 1);
    let r = render(&txtql::ambiguity_reports(&o.ambiguities, "input", "123"));
    assert!(r.contains("txtql::input::ambiguous"), "{r}");
    assert!(r.contains("another reading of this text exists but fails to evaluate"), "{r}");
    assert!(r.contains("`ZIP` got 2 values for 1 keys"), "{r}");
    // Strict runs fail on it like on any other ambiguity.
    let err = compile(q).run("123", &Options { strict: true, ..Options::default() }).unwrap_err();
    assert!(matches!(err, RunError::Ambiguous(_)), "{err:?}");
}

#[test]
fn alternative_failing_a_where_condition_evaluation_is_reported() {
    // The chosen reading (a = [1, 2], b = [3]) satisfies `COUNT(b) = 1`. The alternative
    // (a = [1], b = [2, 3]) goes on to compare a number with a list, which is an error.
    let q = "TEXT = 1 TO n a:DIGIT 1 TO n b:DIGIT WHERE COUNT(b) = 1 OR NUM(FIRST(a)) > a AS COUNT(a)";
    let o = run_opts(q, "123", &Options::default());
    assert_eq!(o.value, json!(2));
    assert_eq!(o.ambiguities.len(), 1);
    let r = render(&txtql::ambiguity_reports(&o.ambiguities, "input", "123"));
    assert!(r.contains("another reading of this text exists but fails to evaluate"), "{r}");
}

#[test]
fn alternative_exceeding_max_depth_is_reported_not_fatal() {
    // The chosen reading is the flat first branch; the other branch reads the same text
    // through `r`, which nests one match per letter: deeper than the limit.
    let q = "r = LETTER 0 TO 1 r\nTEXT = x:(1 TO n LETTER) OR y:r";
    let opts = Options { max_depth: 3, ..Options::default() };
    let o = run_opts(q, "abcdefgh", &opts);
    assert_eq!(o.value, json!({"x": "abcdefgh"}));
    assert_eq!(o.ambiguities.len(), 1);
    let r = render(&txtql::ambiguity_reports(&o.ambiguities, "input", "abcdefgh"));
    assert!(r.contains("another reading of this text exists but fails to evaluate"), "{r}");
    assert!(r.contains("more than 3"), "the reason names the limit: {r}");
    // Strict runs fail on it as an ambiguity, not as too_deep.
    let err = compile(q).run("abcdefgh", &Options { strict: true, ..opts }).unwrap_err();
    assert!(matches!(err, RunError::Ambiguous(_)), "{err:?}");
}
