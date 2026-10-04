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
    assert_snapshot!(parse_sexpr("TEXT = WORD AS ['s', 1, -2, 3.5, -0.5, true, false, null, a.b.c, NUM(x), JOIN(a, ', ')]"), @r#"(rule TEXT WORD (as ["s" 1 -2 3.5 -0.5 true false null a.b.c (NUM x) (JOIN a ", ")]))"#);
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
        "STRICT TEXT = (a OR b) OR c WHERE NOT (x = 1 OR y) AND z CONTAINS 'q' AS { 'k': [v FOR v IN a.b, -1.5], w, k: v FOR r }",
        "TEXT = a WHERE a OR (b AND c) OR NOT d",
        "TEXT = a WHERE (a OR b) AND c",
        // Found by fuzzing: tiny and huge floats print with an exponent.
        "O = R WHERE 00000.000000010",
        "TEXT = WORD AS [1e20, -2.5e-7, 0.5]",
        "ALIAS eol = NL\nALIAS pair = WORD (',' OR ';') eol\nTEXT = 1 TO n pair",
    ] {
        roundtrip(src);
    }
}
