//! Parser: syntax tree shape for every construct, rendered syntax errors, printer round trip.

mod common;
use common::*;
use insta::assert_snapshot;

// ---- patterns ----

#[test]
fn literals_and_primitives() {
    assert_snapshot!(parse_sexpr(r#"TEXT = 'are' "x" i'Are' ' ' WORD FLOAT PUNCT ANY DIGIT LETTER LINE NL"#), @r#"(rule TEXT (seq "are" "x" i"Are" " " WORD FLOAT PUNCT ANY DIGIT LETTER LINE NL))"#);
}

#[test]
fn references_and_labels() {
    assert_snapshot!(parse_sexpr("TEXT = noun:WORD 'are' colors adjs:colors"), @r#"(rule TEXT (seq (label noun WORD) "are" colors (label adjs colors)))"#);
}

#[test]
fn alternatives_bind_looser_than_sequences() {
    assert_snapshot!(parse_sexpr("TEXT = a b OR c OR d e"), @"(rule TEXT (or (seq a b) c (seq d e)))");
}

#[test]
fn parentheses_group_without_a_node() {
    assert_snapshot!(parse_sexpr("TEXT = a (b OR c) ((d))"), @"(rule TEXT (seq a (or b c) d))");
    assert_snapshot!(parse_sexpr("TEXT = x:(a b)"), @"(rule TEXT (label x (seq a b)))");
}

#[test]
fn repetitions() {
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n WORD"), @"(rule TEXT (repeat 1 n WORD))");
    assert_snapshot!(parse_sexpr("TEXT = 0 TO 1 WORD 2 TO 5 LAZY ANY"), @"(rule TEXT (seq (repeat 0 1 WORD) (repeat 2 5 lazy ANY)))");
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n color SPLITBY ('and' OR ',')"), @r#"(rule TEXT (repeat 1 n color (sep (or "and" ","))))"#);
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n w SKIPPING ANY SPLITBY ','"), @r#"(rule TEXT (repeat 1 n w (sep ",") (skip ANY)))"#);
}

#[test]
fn repetition_item_is_a_single_item() {
    // The repeated pattern is one item; the rest of the sequence follows the repetition.
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n WORD 'end'"), @r#"(rule TEXT (seq (repeat 1 n WORD) "end"))"#);
}

#[test]
fn labels_on_and_inside_repetitions() {
    assert_snapshot!(parse_sexpr("TEXT = xs:1 TO n WORD"), @"(rule TEXT (label xs (repeat 1 n WORD)))");
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n x:WORD"), @"(rule TEXT (repeat 1 n (label x WORD)))");
}

#[test]
fn nested_repetitions_take_clauses_innermost() {
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n 1 TO 2 WORD SPLITBY ','"), @r#"(rule TEXT (repeat 1 n (repeat 1 2 WORD (sep ","))))"#);
    assert_snapshot!(parse_sexpr("TEXT = 1 TO n (1 TO 2 WORD) SPLITBY ','"), @r#"(rule TEXT (repeat 1 n (repeat 1 2 WORD) (sep ",")))"#);
}

#[test]
fn until() {
    assert_eq!(
        parse_sexpr("TEXT = key:(ANY UNTILBEFORE ':') ':' ANY UNTIL WORD"),
        r#"(rule TEXT (seq (label key (repeat 0 n ANY (until-before ":"))) ":" (repeat 0 n ANY (until-after WORD))))"#
    );
}

#[test]
fn until_binds_after_the_label() {
    // `name:x` takes the nearest pattern, so the short form repeats the labelled item.
    assert_eq!(
        parse_sexpr("TEXT = k:ANY UNTILBEFORE '='"),
        r#"(rule TEXT (repeat 0 n (label k ANY) (until-before "=")))"#
    );
    // With bounds the label comes first and takes the whole repetition, clauses included.
    assert_eq!(
        parse_sexpr("TEXT = k:1 TO n ANY UNTILBEFORE '='"),
        r#"(rule TEXT (label k (repeat 1 n ANY (until-before "="))))"#
    );
}

#[test]
fn until_is_a_clause_in_any_order() {
    assert_eq!(
        parse_sexpr("TEXT = 1 TO n WORD UNTIL '.' SPLITBY ' '"),
        r#"(rule TEXT (repeat 1 n WORD (sep " ") (until-after ".")))"#
    );
    assert_eq!(
        parse_sexpr("TEXT = 1 TO n LAZY WORD SKIPPING PUNCT UNTILBEFORE NL"),
        r#"(rule TEXT (repeat 1 n lazy WORD (skip PUNCT) (until-before NL)))"#
    );
}

#[test]
fn until_errors() {
    assert_snapshot!(parse_err("TEXT = ANY UNTILBEFORE"));
    // Only one stop per repetition.
    assert_snapshot!(parse_err("TEXT = ANY UNTILBEFORE ',' UNTIL ';'"));
}

#[test]
fn several_rules_and_strict() {
    assert_snapshot!(parse_sexpr("a = WORD\nSTRICT b = a a\nTEXT = b"), @r"
    (rule a WORD)
    (rule STRICT b (seq a a))
    (rule TEXT b)
    ");
}

#[test]
fn keywords_in_any_case() {
    assert_snapshot!(parse_sexpr("text = 1 to n Word splitby ', ' as w"), @r#"(rule text (repeat 1 n WORD (sep ", ")) (as w))"#);
    // A field after `.` may be any word, since fields come from the data.
    assert_snapshot!(parse_sexpr("TEXT = x:WORD AS x.line"), @"(rule TEXT (label x WORD) (as x.line))");
}

#[test]
fn aliases() {
    assert_snapshot!(parse_sexpr("ALIAS eol = NL\nALIAS sp = 1 TO n ' '\nTEXT = WORD sp WORD eol"), @r#"
    (alias eol NL)
    (alias sp (repeat 1 n " "))
    (rule TEXT (seq WORD sp WORD eol))
    "#);
    // `ALIAS` ends the previous rule's pattern, like the start of another rule.
    assert_snapshot!(parse_sexpr("TEXT = WORD\nALIAS a = 'x' OR 'y'"), @r#"
    (alias a (or "x" "y"))
    (rule TEXT WORD)
    "#);
}

#[test]
fn error_alias_with_template() {
    assert_snapshot!(parse_err("ALIAS eol = NL AS 'x'"));
}

#[test]
fn comments() {
    assert_snapshot!(parse_sexpr("-- header\nTEXT = WORD -- trailing\n-- end"), @"(rule TEXT WORD)");
}

#[test]
fn empty_query_parses() {
    assert_snapshot!(parse_sexpr(""), @"");
}

// ---- conditions ----

#[test]
fn where_operators() {
    assert_snapshot!(parse_sexpr(
        "TEXT = x:WORD WHERE x = 'a' OR x != 'b' AND x < 1 AND x <= 2 OR x > 3 OR x >= 4"
    ), @r#"(rule TEXT (label x WORD) (where (or (or (or (= x "a") (and (and (!= x "b") (< x 1)) (<= x 2))) (> x 3)) (>= x 4))))"#);
    assert_snapshot!(parse_sexpr(
        "TEXT = x:WORD WHERE x CONTAINS 'a' AND x STARTSWITH 'b' AND x ENDSWITH 'c'"
    ), @r#"(rule TEXT (label x WORD) (where (and (and (CONTAINS x "a") (STARTSWITH x "b")) (ENDSWITH x "c"))))"#);
}

#[test]
fn where_not_parens_truthy() {
    assert_snapshot!(parse_sexpr("TEXT = x:WORD WHERE NOT (x OR NOT x = 'a') AND LOWER(x)"), @r#"(rule TEXT (label x WORD) (where (and (not (or (truthy x) (not (= x "a")))) (truthy (LOWER x)))))"#);
}

#[test]
fn where_then_next_rule() {
    assert_snapshot!(parse_sexpr("a = x:WORD WHERE x = y\nTEXT = a"), @r"
    (rule a (label x WORD) (where (= x y)))
    (rule TEXT a)
    ");
}

// ---- templates ----

#[test]
fn template_values() {
    // Number literals are unsigned (lang/Lexicon.NumberLiterals).
    assert_snapshot!(parse_sexpr("TEXT = WORD AS ['s', 1, 2, 3.5, 0.5, true, false, null, a.b.c, NUM(x), JOIN(a, ', ')]"), @r#"(rule TEXT WORD (as ["s" 1 2 3.5 0.5 true false null a.b.c (NUM x) (JOIN a ", ")]))"#);
}

#[test]
fn template_objects() {
    assert_snapshot!(parse_sexpr("TEXT = WORD AS { 'k': v, key: value, rhyme, n: c FOR r, }"), @r#"(rule TEXT WORD (as {("k" v) (key value) (merge rhyme) (n c (for r))}))"#);
    assert_snapshot!(parse_sexpr("TEXT = WORD AS {}"), @"(rule TEXT WORD (as {}))");
}

#[test]
fn template_arrays_and_comprehensions() {
    assert_snapshot!(parse_sexpr("TEXT = WORD AS [ noun FOR rhyme, [c FOR c IN rhyme.colors] FOR rhyme, ]"), @"(rule TEXT WORD (as [noun (for rhyme) [c (for c in rhyme.colors)] (for rhyme)]))");
    assert_snapshot!(parse_sexpr("TEXT = WORD AS []"), @"(rule TEXT WORD (as []))");
}

#[test]
fn where_and_as_together() {
    assert_snapshot!(parse_sexpr("TEXT = x:WORD WHERE x AS { 'x': x }"), @r#"(rule TEXT (label x WORD) (where (truthy x)) (as {("x" x)}))"#);
}

// ---- syntax errors ----

#[test]
fn error_pipe() {
    assert_snapshot!(parse_err("TEXT = 'a' | 'b'"));
}

#[test]
fn error_regex_quantifier() {
    assert_snapshot!(parse_err("TEXT = WORD+"));
}

#[test]
fn error_missing_equals() {
    assert_snapshot!(parse_err("TEXT WORD"));
}

#[test]
fn error_keyword_as_rule_name() {
    assert_snapshot!(parse_err("WORD = 'x'"));
}

#[test]
fn error_keyword_as_label() {
    assert_snapshot!(parse_err("TEXT = line:LINE"));
}

#[test]
fn error_missing_pattern() {
    assert_snapshot!(parse_err("TEXT = "));
    assert_snapshot!(parse_err("TEXT = a OR"));
}

#[test]
fn error_missing_to() {
    assert_snapshot!(parse_err("TEXT = 1 n WORD"));
}

#[test]
fn error_bad_upper_bound() {
    assert_snapshot!(parse_err("TEXT = 1 TO m WORD"));
}

#[test]
fn error_repeat_without_item() {
    assert_snapshot!(parse_err("TEXT = 1 TO n"));
}

#[test]
fn error_unclosed_paren() {
    assert_snapshot!(parse_err("TEXT = (a b"));
}

#[test]
fn error_label_without_pattern() {
    assert_snapshot!(parse_err("TEXT = x: OR y"));
}

#[test]
fn error_where_after_as() {
    assert_snapshot!(parse_err("TEXT = x:WORD AS x WHERE x"));
}

#[test]
fn error_unclosed_object() {
    assert_snapshot!(parse_err("TEXT = x:WORD AS { 'a': x"));
}

#[test]
fn error_object_entry_without_colon() {
    assert_snapshot!(parse_err("TEXT = x:WORD AS { x y }"));
}

#[test]
fn error_unterminated_string() {
    assert_snapshot!(parse_err("TEXT = 'abc\nb = WORD"));
}

#[test]
fn error_bad_escape() {
    assert_snapshot!(parse_err(r"TEXT = 'a\qb'"));
}

#[test]
fn error_huge_number() {
    assert_snapshot!(parse_err("TEXT = 1 TO 99999999999999999999999 WORD"));
}

#[test]
fn error_bad_template_start() {
    assert_snapshot!(parse_err("TEXT = x:WORD AS )"));
}

#[test]
fn error_stray_token_after_rule() {
    assert_snapshot!(parse_err("TEXT = WORD )"));
}

#[test]
fn error_nesting_too_deep() {
    let deep = format!("TEXT = {}WORD{}", "(".repeat(200), ")".repeat(200));
    let e = parse_err(&deep);
    assert!(e.contains("txtql::parse::too_deep"), "{e}");
    let deep = format!("TEXT = WORD AS {}1{}", "[".repeat(500), "]".repeat(500));
    assert!(parse_err(&deep).contains("txtql::parse::too_deep"));
    let deep = format!("TEXT = WORD WHERE {} x", "NOT ".repeat(500));
    assert!(parse_err(&deep).contains("txtql::parse::too_deep"));
}

// ---- case-insensitive literals are pattern-only (lang/Lexicon.Strings) ----

/// Asserts `src` is an `unexpected_token` error located at `at` (its first occurrence),
/// with a help line containing every one of `help`.
fn assert_unexpected_at(src: &str, at: &str, help: &[&str]) {
    let e = parse_err(src);
    assert!(e.contains("txtql::parse::unexpected_token"), "{src}\n{e}");
    let col = src.find(at).unwrap_or_else(|| panic!("{at:?} not in {src:?}")) + 1;
    assert!(e.contains(&format!("[query:1:{col}]")), "{src}: expected error at column {col}\n{e}");
    let hint = e.split("help:").nth(1).unwrap_or_else(|| panic!("{src}: no help line\n{e}"));
    for h in help {
        assert!(hint.contains(h), "{src}: help should mention {h:?}\n{e}");
    }
}

const CI_HINT: &[&str] = &["only available in patterns"];

#[test]
fn ci_literal_in_template_is_a_syntax_error() {
    assert_unexpected_at("TEXT = w:WORD AS i'Q'", "i'Q'", CI_HINT);
    assert_unexpected_at(r#"TEXT = w:WORD AS i"Q""#, r#"i"Q""#, CI_HINT);
}

#[test]
fn ci_literal_nested_in_template_is_a_syntax_error() {
    // Object value and key, array element, function argument.
    assert_unexpected_at("TEXT = w:WORD AS { 'k': i'v' }", "i'v'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD AS { i'k': w }", "i'k'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD AS [w, i'x']", "i'x'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD AS LOWER(i'X')", "i'X'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD AS { 'k': [LOWER(i\"X\")] }", "i\"X\"", CI_HINT);
}

#[test]
fn ci_literal_in_where_is_a_syntax_error() {
    assert_unexpected_at("TEXT = w:WORD WHERE w = i'hello' AS w", "i'hello'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD WHERE i'hello' = w", "i'hello'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD WHERE i'hello'", "i'hello'", CI_HINT);
    assert_unexpected_at("TEXT = w:WORD WHERE w CONTAINS i'el' AS w", "i'el'", CI_HINT);
    assert_unexpected_at(r#"TEXT = w:WORD WHERE w STARTSWITH i"h""#, r#"i"h""#, CI_HINT);
    assert_unexpected_at("TEXT = w:WORD WHERE LOWER(w) = LOWER(i'A')", "i'A'", CI_HINT);
}

#[test]
fn ci_literal_still_parses_in_patterns_and_i_is_still_a_name() {
    assert_eq!(parse_sexpr("TEXT = i'hello' AS 'matched'"), r#"(rule TEXT i"hello" (as "matched"))"#);
    assert_eq!(parse_sexpr("TEXT = ANY UNTIL i'end'"), r#"(rule TEXT (repeat 0 n ANY (until-after i"end")))"#);
    // `i` then a space then a quote: a name followed by a plain literal.
    assert_eq!(parse_sexpr("i = DIGIT\nTEXT = i 'abc'"), "(rule i DIGIT)\n(rule TEXT (seq i \"abc\"))");
    // `i` as a label used in WHERE and AS.
    assert_eq!(
        parse_sexpr("TEXT = i:WORD WHERE i = 'abc' AS { 'k': i }"),
        r#"(rule TEXT (label i WORD) (where (= i "abc")) (as {("k" i)}))"#
    );
}

// ---- one stop per repetition (lang/RepetitionMatching.OneStopPerRepetition) ----

const STOP_HINT: &[&str] = &["UNTIL (", "OR"];

/// The second stop keyword is the last `UNTIL`/`UNTILBEFORE` in these queries.
fn assert_second_stop_error(src: &str) {
    let e = parse_err(src);
    assert!(e.contains("txtql::parse::unexpected_token"), "{src}\n{e}");
    let col = src.rfind("UNTIL").unwrap() + 1;
    assert!(e.contains(&format!("[query:1:{col}]")), "{src}: expected error at column {col}\n{e}");
    let hint = e.split("help:").nth(1).unwrap_or_else(|| panic!("{src}: no help line\n{e}"));
    for h in STOP_HINT {
        assert!(hint.contains(h), "{src}: help should mention {h:?}\n{e}");
    }
}

#[test]
fn second_until_is_a_syntax_error() {
    assert_second_stop_error("TEXT = 1 TO n ANY UNTIL ';' UNTIL '!'");
    assert_second_stop_error("TEXT = ANY UNTIL ';' UNTIL '!'");
    assert_second_stop_error("TEXT = k:(1 TO n ANY UNTIL ';' UNTIL '!')");
    // Other clauses in between do not reset the count.
    assert_second_stop_error("TEXT = 1 TO n WORD UNTIL ';' SPLITBY ' ' UNTIL '!'");
}

#[test]
fn until_and_untilbefore_together_is_a_syntax_error() {
    assert_second_stop_error("TEXT = 1 TO n ANY UNTIL ';' UNTILBEFORE '!'");
    assert_second_stop_error("TEXT = ANY UNTIL ';' UNTILBEFORE '!'");
    assert_second_stop_error("TEXT = 1 TO n ANY UNTILBEFORE ';' UNTIL '!'");
    assert_second_stop_error("TEXT = ANY UNTILBEFORE ';' UNTIL '!'");
}

#[test]
fn second_untilbefore_is_a_syntax_error() {
    assert_second_stop_error("TEXT = 1 TO n ANY UNTILBEFORE ';' UNTILBEFORE '!'");
    assert_second_stop_error("TEXT = ANY UNTILBEFORE ';' UNTILBEFORE '!'");
}

#[test]
fn one_stop_and_parenthesised_stops_are_legal() {
    assert_eq!(
        parse_sexpr("TEXT = 1 TO n ANY UNTIL (';' OR '!')"),
        r#"(rule TEXT (repeat 1 n ANY (until-after (or ";" "!"))))"#
    );
    // A group is a separate item: the outer stop is a new short-form repetition.
    assert_eq!(
        parse_sexpr("TEXT = (0 TO n ANY UNTIL ';') UNTIL '!'"),
        r#"(rule TEXT (repeat 0 n (repeat 0 n ANY (until-after ";")) (until-after "!")))"#
    );
    assert_eq!(
        parse_sexpr("TEXT = (ANY UNTIL ';') UNTILBEFORE '!'"),
        r#"(rule TEXT (repeat 0 n (repeat 0 n ANY (until-after ";")) (until-before "!")))"#
    );
}

#[test]
fn second_splitby_or_skipping_is_still_a_syntax_error() {
    for (src, kw) in [
        ("TEXT = 1 TO n WORD SPLITBY ',' SPLITBY ';'", "SPLITBY ';'"),
        ("TEXT = 1 TO n WORD SKIPPING ',' SKIPPING ';'", "SKIPPING ';'"),
    ] {
        let e = parse_err(src);
        assert!(e.contains("txtql::parse::unexpected_token"), "{src}\n{e}");
        let col = src.find(kw).unwrap() + 1;
        assert!(e.contains(&format!("[query:1:{col}]")), "{src}: expected error at column {col}\n{e}");
    }
}

// ---- number literal ranges (lang/Lexicon.NumberLiterals) ----

#[test]
fn number_literals_reach_the_integer_limits() {
    assert_eq!(parse_sexpr("TEXT = WORD AS 18446744073709551615"), "(rule TEXT WORD (as 18446744073709551615))");
    assert_eq!(
        parse_sexpr("TEXT = WORD AS [0, 18446744073709551615]"),
        "(rule TEXT WORD (as [0 18446744073709551615]))"
    );
    assert_eq!(
        parse_sexpr("TEXT = n:INT WHERE NUM(n) < 18446744073709551615"),
        "(rule TEXT (label n INT) (where (< (NUM n) 18446744073709551615)))"
    );
}

/// Asserts `src` is a `bad_number` error located at `at` (its first occurrence).
fn assert_bad_number_at(src: &str, at: &str) {
    let e = parse_err(src);
    assert!(e.contains("txtql::parse::bad_number"), "{src}\n{e}");
    let col = src.find(at).unwrap_or_else(|| panic!("{at:?} not in {src:?}")) + 1;
    assert!(e.contains(&format!("[query:1:{col}]")), "{src}: expected error at column {col}\n{e}");
}

#[test]
fn number_literals_beyond_the_integer_limits_are_bad_numbers() {
    // Reported at the digits.
    assert_bad_number_at("TEXT = WORD AS 18446744073709551616", "18446744073709551616");
    assert_bad_number_at("TEXT = WORD AS [1, 18446744073709551616]", "18446744073709551616");
    assert_bad_number_at("TEXT = WORD AS { 'k': 99999999999999999999 }", "99999999999999999999");
    assert_bad_number_at("TEXT = n:INT WHERE NUM(n) > 18446744073709551616", "18446744073709551616");
}

#[test]
fn non_finite_float_literals_are_bad_numbers() {
    assert_bad_number_at("TEXT = WORD AS 1e999", "1e999");
    assert_bad_number_at("TEXT = WORD AS [1.5e400]", "1.5e400");
    assert_bad_number_at("TEXT = n:INT WHERE NUM(n) > 1E+999", "1E+999");
}

#[test]
fn exponent_signs_are_part_of_the_float_literal() {
    for src in [
        "TEXT = WORD AS 1e-5",
        "TEXT = WORD AS 2.5e-7",
        "TEXT = WORD AS 1E+3",
        "TEXT = WORD AS [1e-5, 2.5e-7, 1E+3, 1e-999]",
        "TEXT = n:INT WHERE NUM(n) > 1e-5 AS { 'k': 2.5E-7 }",
    ] {
        parse_sexpr(src);
    }
    // Too small to represent rounds to zero.
    assert_eq!(parse_sexpr("TEXT = WORD AS 1e-999"), "(rule TEXT WORD (as 0.0))");
}

/// Asserts `src` is an `unexpected_token` error at the first `-`, which the message names.
fn assert_minus_rejected(src: &str) {
    let e = parse_err(src);
    assert!(e.contains("txtql::parse::unexpected_token"), "{src}\n{e}");
    assert!(!e.contains("txtql::parse::bad_number"), "{src}\n{e}");
    let col = src.find('-').unwrap() + 1;
    assert!(e.contains(&format!("[query:1:{col}]")), "{src}: expected error at column {col}\n{e}");
    assert!(e.contains("found `-`"), "{src}: message should say found `-`\n{e}");
}

#[test]
fn negative_number_literals_are_syntax_errors() {
    for src in [
        "TEXT = WORD AS -5",
        "TEXT = WORD AS - 5",
        "TEXT = WORD AS -1.5",
        "TEXT = WORD AS -2.5e-7",
        "TEXT = WORD AS [-1.5]",
        "TEXT = WORD AS [1, -2]",
        "TEXT = WORD AS { 'k': -1 }",
        "TEXT = WORD AS { 'k': [0, - 1] }",
        "TEXT = w:WORD AS LOWER(-1)",
        "TEXT = w:WORD AS JOIN(w, -1)",
        "TEXT = WORD AS -9223372036854775808",
        "TEXT = WORD AS -18446744073709551615",
        "TEXT = n:INT WHERE NUM(n) > -5",
        "TEXT = n:INT WHERE NUM(n) > - 5",
        "TEXT = n:INT WHERE -5 < NUM(n)",
        "TEXT = n:INT WHERE NUM(n) = -0.5 AS n",
        "TEXT = n:INT WHERE NOT NUM(n) > -9223372036854775808",
        "TEXT = n:INT WHERE -1",
    ] {
        assert_minus_rejected(src);
    }
}

#[test]
fn a_stray_minus_between_values_is_a_syntax_error() {
    let src = "TEXT = WORD AS [ 2 - 3 ]";
    assert_minus_rejected(src);
    let e = parse_err(src);
    assert!(e.contains("expected `,` or `]`, found `-`"), "{e}");
}

#[test]
fn signed_text_is_still_matched_in_patterns() {
    // The restriction is on literals in templates and conditions; patterns match `-` as text.
    assert_eq!(
        parse_sexpr("TEXT = n:(0 TO 1 '-' INT) AS NUM(n)"),
        r#"(rule TEXT (label n (seq (repeat 0 1 "-") INT)) (as (NUM n)))"#
    );
}

#[test]
fn ci_literal_error_shows_the_literal_as_written() {
    for (src, shown) in [
        ("TEXT = w:WORD AS i'x'", "found `i'x'`"),
        (r#"TEXT = w:WORD AS i"x""#, r#"found `i"x"`"#),
        ("TEXT = w:WORD AS [w, i'Q']", "found `i'Q'`"),
        ("TEXT = w:WORD WHERE w = i'hello' AS w", "found `i'hello'`"),
        ("TEXT = w:WORD WHERE LOWER(w) = LOWER(i'A')", "found `i'A'`"),
    ] {
        let e = parse_err(src);
        assert!(e.contains("txtql::parse::unexpected_token"), "{src}\n{e}");
        assert!(e.contains(shown), "{src}: message should contain {shown:?}\n{e}");
        assert!(e.contains("only available in patterns"), "{src}: help unchanged\n{e}");
    }
}

#[test]
fn repetition_bounds_beyond_u64_are_bad_numbers() {
    assert_bad_number_at("TEXT = 99999999999999999999 TO n WORD", "99999999999999999999");
    assert_bad_number_at("TEXT = 1 TO 18446744073709551616 WORD", "18446744073709551616");
    // In range of u64, a large bound parses; the static check judges it (bound_too_large).
    parse_sexpr("TEXT = 5000000000 TO n WORD");
    parse_sexpr("TEXT = 1 TO 18446744073709551615 WORD");
}

// ---- printer round trip ----

fn roundtrip(src: &str) {
    let q = txtql::parser::parse(src).unwrap();
    let printed = txtql::printer::print_query(&q);
    let q2 = txtql::parser::parse(&printed).unwrap_or_else(|e| panic!("reparse failed: {e:?}\n{printed}"));
    assert_eq!(sexpr_query(&q), sexpr_query(&q2), "printed:\n{printed}");
    assert_eq!(printed, txtql::printer::print_query(&q2));
}

#[test]
fn printer_round_trips() {
    for src in [
        "TEXT = a (b OR c) d",
        "TEXT = x:(a b) y:(z:WORD)",
        "TEXT = 1 TO n (1 TO 2 WORD SPLITBY ',') SPLITBY ';' SKIPPING ANY",
        "TEXT = 1 TO n 1 TO n LAZY WORD",
        "TEXT = xs:1 TO n x:WORD",
        "TEXT = 1 TO n (xs:1 TO 3 WORD)",
        "TEXT = (ANY UNTILBEFORE 'a b') i'Q\\'x\\n'",
        "STRICT TEXT = (a OR b) OR c WHERE NOT (x = 1 OR y) AND z CONTAINS 'q' AS { 'k': [v FOR v IN a.b, 1.5], w, k: v FOR r }",
        "TEXT = a WHERE a OR (b AND c) OR NOT d",
        "TEXT = a WHERE (a OR b) AND c",
        // Found by fuzzing: tiny and huge floats print with an exponent.
        "O = R WHERE 00000.000000010",
        "TEXT = WORD AS [1e20, 2.5e-7, 0.5]",
        "TEXT = n:INT WHERE NUM(n) > 1e-5 AS [1E+3, 18446744073709551615]",
        "ALIAS eol = NL\nALIAS pair = WORD (',' OR ';') eol\nTEXT = 1 TO n pair",
    ] {
        roundtrip(src);
    }
}
