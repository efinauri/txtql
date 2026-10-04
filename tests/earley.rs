//! Earley recogniser: acceptance, whitespace, recursion shapes, nullable rules, limits.

mod common;
use txtql::earley::{Chart, recognize};
use txtql::error::MatchError;
use txtql::input::Input;
use txtql::util::Budget;

fn chart(q: &str, input: &str) -> Chart {
    let q = common::compile(q);
    let input = Input::new(input);
    recognize(&q.grammar, &input, &mut Budget::new(u64::MAX)).unwrap()
}

fn accepts(q: &str, input: &str) -> bool {
    chart(q, input).accepted
}

#[track_caller]
fn check(q: &str, yes: &[&str], no: &[&str]) {
    for y in yes {
        assert!(accepts(q, y), "{q:?} should accept {y:?}");
    }
    for n in no {
        assert!(!accepts(q, n), "{q:?} should reject {n:?}");
    }
}

#[test]
fn sequences_and_literals() {
    check(
        "TEXT = WORD ' are ' WORD",
        &["roses are red"],
        &["roses is red", "roses are", "roses are red ok", "roses  are red", "roses are\nred", "roses are red\n"],
    );
}

#[test]
fn whitespace_is_significant() {
    check("TEXT = WORD WORD", &[], &["a b", "ab"]);
    check("TEXT = WORD ' ' WORD", &["a b"], &["a  b", "a\tb", "a\nb", " a b"]);
    check("TEXT = WORD 1 TO n ' ' WORD", &["a b", "a   b"], &["ab", "a \tb"]);
    check("TEXT = WORD NL", &["a\n", "a\r\n", "a\r"], &["a", "a\n\n", "a \n"]);
}

#[test]
fn alternatives() {
    check("TEXT = 'a' OR 'b' ' ' 'c' OR FLOAT", &["a", "b c", "42"], &["b", "a c", ""]);
}

#[test]
fn words_and_numbers_are_whole_runs() {
    check("TEXT = WORD", &["abc", "café", "e\u{301}te\u{301}"], &["ab1", "don't", ""]);
    check("TEXT = WORD WORD", &[], &["abc"]);
    check("TEXT = FLOAT", &["42", "3.14"], &["4 2", "1."]);
    check("TEXT = FLOAT FLOAT", &[], &["42"]);
    check("TEXT = WORD FLOAT", &["abc123"], &[]);
    check("TEXT = FLOAT '.' FLOAT", &["1.2.3"], &["1.2"]);
}

#[test]
fn int_float_and_helpers() {
    check("TEXT = INT", &["42"], &["4.2", "", "a"]);
    check("TEXT = INT '.' INT", &["3.14"], &[]);
    check("TEXT = FLOAT", &["3.14", "42"], &["3."]);
    check("TEXT = INT '.' INT '.' INT '.' INT", &["199.72.81.55"], &[]);
    // FLOAT takes `199.72` as one number, which is why IP addresses need INT or IPV4.
    check("TEXT = FLOAT '.' FLOAT", &["199.72.81.55", "1.2.3.4", "1.2.3"], &["1.2"]);
    check("TEXT = '0x' HEX", &["0x1F", "0xdeadbeef"], &["0xz1"]);
    check("TEXT = HEX", &["cafe", "3f2a1c9e"], &["cafeteria", "g"]);
    check("TEXT = BIN", &["1010"], &["102", "10a"]);
    check("TEXT = IPV4", &["199.72.81.55", "0.0.0.0"], &["256.1.1.1", "1.2.3", "1.2.3.4.5"]);
    check("TEXT = IPV6", &["::1", "2001:db8::8a2e:370:7334", "::ffff:10.0.0.1"], &["1:2:3", "::1::", "::1:"]);
    check("TEXT = '[' IPV6 ']:' INT", &["[::1]:8080"], &["[::1:8080]"]);
    check("TEXT = WORD TAB WORD NL", &["a\tb\n", "a\tb\r\n"], &["a b\n", "a\t\tb\n"]);
}

#[test]
fn digits_and_letters_match_inside_runs() {
    check("TEXT = 1 TO n DIGIT", &["987654321"], &["98a", ""]);
    check("TEXT = DIGIT DIGIT", &["42"], &["4"]);
    check("TEXT = 1 TO n LETTER", &["XMAS", "e\u{301}t"], &["X1"]);
    check("TEXT = LETTER WORD", &[], &["ab"]);
}

#[test]
fn any_crosses_lines() {
    check("TEXT = 3 TO 3 ANY", &["a\nb", "abc"], &["ab", "a\r\nbc"]);
}

#[test]
fn punctuation() {
    check("TEXT = 1 TO n PUNCT", &["..@#", "😀"], &["a", " ", "\n"]);
}

#[test]
fn left_recursion() {
    let q = "expr = expr ' + ' FLOAT OR FLOAT\nTEXT = expr";
    check(q, &["1", "1 + 2", "1 + 2 + 3 + 4"], &["", "+", "1 +", "1 2", "1+2"]);
}

#[test]
fn right_recursion() {
    let q = "list = WORD ',' list OR WORD\nTEXT = list";
    check(q, &["a", "a,b", "a,b,c"], &["a,", ",a", "", "a, b"]);
}

#[test]
fn center_recursion() {
    let q = "p = '(' 0 TO 1 p ')'\nTEXT = p";
    check(q, &["()", "(())", "((()))"], &["(", "(()", "())", ""]);
}

#[test]
fn nullable_rules() {
    let q = "a = 0 TO 1 'x'\nTEXT = p:a q:a 'y'";
    check(q, &["y", "xy", "xxy"], &["xxxy", "x"]);
}

#[test]
fn nullable_rule_completes_before_and_after_prediction() {
    // `e` is referenced twice at the same position; the second reference must also see it
    // complete empty.
    let q = "e = 0 TO 1 DIGIT\nTEXT = a:e b:e WORD c:e";
    check(q, &["w", "1w", "12w", "12w3", "w3"], &["123w", "w12"]);
}

#[test]
fn bounds() {
    check("TEXT = 0 TO 0 WORD 'x'", &["x"], &["ax"]);
    check("TEXT = 3 TO 3 DIGIT", &["123"], &["12", "1234"]);
    check("TEXT = 2 TO 4 DIGIT", &["12", "123", "1234"], &["1", "12345"]);
    check("TEXT = 0 TO n DIGIT", &["", "1", "123"], &["a"]);
    check("TEXT = 3 TO n DIGIT", &["123", "12345"], &["12"]);
}

#[test]
fn separators() {
    check("TEXT = 1 TO n WORD SPLITBY ','", &["a", "a,b", "a,b,c"], &["a b", "a,", ",a", "", "a, b"]);
    check("TEXT = 1 TO n WORD SPLITBY ', '", &["a, b"], &["a,b"]);
    check("TEXT = 2 TO 3 WORD SPLITBY ','", &["a,b", "a,b,c"], &["a", "a,b,c,d"]);
    check("TEXT = 0 TO n WORD SPLITBY ','", &["", "a", "a,b"], &[","]);
}

#[test]
fn separators_between_empty_items() {
    check("TEXT = 1 TO n LINE SPLITBY NL", &["", "a", "a\n", "a\n\nb", "\n\n"], &[]);
}

#[test]
fn skipping() {
    check("TEXT = 1 TO n FLOAT SKIPPING ANY", &["1", "a 1 b", "ab 1 c 2 d"], &["a b"]);
}

#[test]
fn case_insensitive() {
    check("TEXT = i'ARE'", &["are", "Are", "ARE"], &["ar", "aren"]);
    check("TEXT = 'Are'", &["Are"], &["are"]);
    // Simple per-character lower-casing, not full case folding: ß does not match SS.
    check("TEXT = i'straße'", &["STRAßE", "Straße"], &["STRASSE"]);
}

#[test]
fn lines() {
    check("TEXT = LINE", &["a b", "", "x y z"], &["a\nb", "a\n"]);
    check("TEXT = LINE NL LINE", &["a b\nc d", "\n", "a\n"], &["a b c"]);
    check(
        "TEXT = WORD ': ' (ANY UNTILBEFORE NL) NL",
        &["key: some value\n", "key: \n"],
        &["key: a\nb\n", "key: some value"],
    );
    check("TEXT = 1 TO n (WORD ': ' (ANY UNTIL NL))", &["a: 1\nb: x y\n", "a: 1 b: 2\n"], &["a:\nb: 2\n", "a: 1"]);
    check("TEXT = 'x' LINE", &[], &["xy"]);
    // `\r\n` is one line break: the position between its two characters is not a line start.
    check("TEXT = '\r' LINE NL", &[], &["\r\n"]);
    check("TEXT = '\r' LINE NL", &["\r\r"], &[]);
}

#[test]
fn until() {
    check("TEXT = (ANY UNTILBEFORE ':') ':' WORD", &[":x", "a b c :x"], &["a b"]);
    check("TEXT = (ANY UNTILBEFORE WORD) WORD", &["12 ,x", "x"], &["12"]);
    check("TEXT = (ANY UNTILBEFORE 'a b') 'a b'", &["x a y a b"], &["x a y"]);
    check("TEXT = (ANY UNTILBEFORE ';') ';'", &["a\nb\nc;"], &[]);
    check("TEXT = (ANY UNTILBEFORE NL) NL", &["a b\n", "\n"], &["a"]);
    // Alternatives: stop at whichever comes first.
    check("TEXT = (ANY UNTILBEFORE (' ' OR '\"')) ('\"' OR ' x')", &["/a.mpg\"", "/b x"], &["/a b\"", "/a\" x"]);
    check("TEXT = (ANY UNTILBEFORE (DIGIT OR NL)) (DIGIT OR NL)", &["ab1", "ab\n"], &["a\nb1"]);
}

#[test]
fn untilbefore_needs_its_stop() {
    // The stop must follow, so the repetition cannot end early or at the end of the text.
    check("TEXT = (ANY UNTILBEFORE ',') LETTER", &[], &["abc", "ab,c"]);
    check("TEXT = (ANY UNTILBEFORE NL)", &[], &["abc", "abc\n"]);
    check("TEXT = (ANY UNTILBEFORE NL) NL", &["abc\n", "\n"], &["abc"]);
}

#[test]
fn untilafter_consumes_its_stop() {
    check("TEXT = ANY UNTIL ','", &["abc,", ","], &["abc", "a,b,"]);
    check("TEXT = ANY UNTIL NL WORD", &["a b\nc", "\nc"], &["a b c"]);
}

#[test]
fn the_stop_is_checked_where_an_iteration_begins() {
    // Not inside a word: `WORD` iterations begin at word starts only.
    check("TEXT = (0 TO n WORD SPLITBY ' ' UNTILBEFORE 'ing') 'ing'", &["ing"], &["saying"]);
    // With a separator, before the separator: after it, an item is read even if it is the stop.
    check("TEXT = (1 TO n WORD SPLITBY ' ' UNTILBEFORE 'ing') 'ing'", &[], &["say ing"]);
    // Before the separator: the stop ends the list even where a separator would fit.
    check("TEXT = (1 TO n WORD SPLITBY ', ' UNTILBEFORE ', and') ', and ' WORD", &["a, b, and c"], &[]);
    // Bounds still apply.
    check("TEXT = 0 TO 2 ANY UNTIL ','", &["ab,", ","], &["abc,"]);
    check("TEXT = 2 TO n ANY UNTIL ','", &["ab,"], &["a,"]);
}

#[test]
fn empty_input() {
    check("TEXT = 0 TO n WORD", &[""], &[" ", "\n"]);
    check("TEXT = WORD", &[], &["", "  "]);
    check("TEXT = LINE", &[""], &[]);
}

#[test]
fn chart_ends_index() {
    let c = chart("TEXT = 1 TO n DIGIT", "123");
    // The repetition (nt1) can end after 1, 2 or 3 digits when started at 0.
    assert_eq!(c.ends(1, 0).collect::<Vec<_>>(), vec![1, 2, 3]);
    assert!(!c.has(1, 0, 0));
    // Repetitions loop inside their NFA, so they are only ever started at their first position.
    assert!(!c.has(1, 1, 3));
    assert_eq!(c.ends(1, 1).count(), 0);
}

#[test]
fn multi_character_terminals_skip_positions() {
    // WORD spans the whole word in one step; the positions inside it have no items.
    let c = chart("w = WORD\nTEXT = x:w ' ' y:w", "hello world");
    assert!(c.accepted);
    assert!(c.has(0, 0, 5) && c.has(0, 6, 11));
}

#[test]
fn chart_size_is_polynomial_for_ambiguous_grammars() {
    // Exponentially many parses, but the chart stays polynomial.
    let q = "TEXT = 1 TO n (DIGIT OR DIGIT DIGIT)";
    let c50 = chart(q, &"1".repeat(50)).completion_count();
    let c100 = chart(q, &"1".repeat(100)).completion_count();
    assert!(c100 <= c50 * 5, "{c50} -> {c100}");
}

#[test]
fn linear_chart_for_lists() {
    let q = "TEXT = 1 TO n (WORD ',')";
    let small = chart(q, &"w,".repeat(1000)).completion_count();
    let big = chart(q, &"w,".repeat(2000)).completion_count();
    assert!(big <= small * 2 + 10, "{small} -> {big}");
}

#[test]
fn step_limit() {
    let q = common::compile("TEXT = 1 TO n WORD SPLITBY ' '");
    let input = Input::new("a b c d e f g h");
    let err = recognize(&q.grammar, &input, &mut Budget::new(5)).map(|_| ()).unwrap_err();
    assert_eq!(err, MatchError::TooExpensive { limit: 5 });
}

#[test]
fn furthest_position_and_expected() {
    let q = common::compile("TEXT = 1 TO n (WORD ' are ' WORD) SPLITBY NL");
    let input = Input::new("roses are red\nviolets is blue");
    let c = recognize(&q.grammar, &input, &mut Budget::new(u64::MAX)).unwrap();
    assert!(!c.accepted);
    assert_eq!(c.furthest, 21, "right after `violets`");
    let expected: Vec<String> = c.expected.iter().map(|&(s, _)| q.grammar.describe_sym(s)).collect();
    assert_eq!(expected, ["' are '"]);
}
