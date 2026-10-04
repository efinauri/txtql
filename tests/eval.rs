//! Output construction: capture scoping, default values, templates, runtime errors.

mod common;
use common::*;
use insta::assert_snapshot;
use serde_json::json;

// ---- default values ----

#[test]
fn text_of_a_plain_match() {
    assert_eq!(run("TEXT = WORD 2 TO 2 ' ' 'are' NL WORD", "roses  are\nred"), json!("roses  are\nred"));
}

#[test]
fn object_of_captures() {
    assert_eq!(run("TEXT = noun:WORD ' are ' adj:WORD", "roses are red"), json!({"noun": "roses", "adj": "red"}));
}

#[test]
fn single_repetition_gives_a_list() {
    assert_eq!(run("TEXT = 1 TO n WORD SPLITBY ' '", "a b"), json!(["a", "b"]));
    assert_eq!(run("TEXT = 0 TO n WORD", ""), json!([]));
}

#[test]
fn repetition_of_groups_gives_objects() {
    assert_eq!(
        run("TEXT = 1 TO n (k:WORD ' = ' v:FLOAT) SPLITBY ', '", "a = 1, b = 2"),
        json!([{"k": "a", "v": "1"}, {"k": "b", "v": "2"}])
    );
}

#[test]
fn rule_references_capture_under_their_name() {
    assert_eq!(
        run("color = WORD\nTEXT = color ' and ' other:color", "red and blue"),
        json!({"color": "red", "other": "blue"})
    );
}

#[test]
fn captures_inside_repetitions_are_lists() {
    assert_eq!(
        run("TEXT = 'x' 1 TO n (' ' k:WORD ' = ' v:FLOAT)", "x a = 1 b = 2"),
        json!({"k": ["a", "b"], "v": ["1", "2"]})
    );
}

#[test]
fn nested_repetitions_nest_lists() {
    assert_eq!(
        run("TEXT = 'x' 1 TO n (' [' 1 TO n n:FLOAT SPLITBY ' ' ']')", "x [1 2] [3]"),
        json!({"n": [["1", "2"], ["3"]]})
    );
}

#[test]
fn optional_captures_are_values_or_null() {
    assert_eq!(run("TEXT = a:WORD 0 TO 1 (' ' b:FLOAT)", "x"), json!({"a": "x", "b": null}));
    assert_eq!(run("TEXT = a:WORD 0 TO 1 (' ' b:FLOAT)", "x 1"), json!({"a": "x", "b": "1"}));
    assert_eq!(run("TEXT = a:WORD 1 TO 1 (' ' b:FLOAT)", "x 1"), json!({"a": "x", "b": "1"}));
    // Rule references too, and a repetition's own value.
    assert_eq!(run("TEXT = WORD 0 TO 1 r\nr = ' ' n:INT AS NUM(n)", "x 7"), json!({"r": 7}));
    assert_eq!(run("TEXT = 0 TO 1 WORD", ""), json!(null));
    assert_eq!(run("TEXT = 0 TO 1 WORD", "x"), json!("x"));
    // A label on the repetition still captures its text.
    assert_eq!(run("TEXT = a:WORD b:(0 TO 1 (' ' FLOAT))", "x"), json!({"a": "x", "b": ""}));
}

#[test]
fn captures_are_lists_when_more_than_one_item_is_allowed() {
    assert_eq!(run("TEXT = a:WORD 0 TO 2 (' ' b:FLOAT)", "x 1"), json!({"a": "x", "b": ["1"]}));
    assert_eq!(run("TEXT = 0 TO 2 WORD", "x"), json!(["x"]));
}

#[test]
fn for_over_an_optional_capture_is_an_error() {
    let e = compile_err("TEXT = 0 TO 1 w:WORD AS [ x FOR x IN w ]");
    assert!(e.contains("not_repeated"), "{e}");
}

#[test]
fn untaken_alternative_captures_are_null_in_templates() {
    assert_eq!(run("TEXT = (a:WORD OR b:FLOAT) AS [a, b]", "5"), json!([null, "5"]));
}

#[test]
fn default_value_only_has_the_taken_alternatives_captures() {
    assert_eq!(run("TEXT = a:WORD OR b:FLOAT", "5"), json!({"b": "5"}));
}

#[test]
fn alternatives_inside_repetitions_keep_only_matched_captures() {
    assert_eq!(
        run("TEXT = 'x' 1 TO n (' ' (w:WORD OR n:FLOAT))", "x a 1 b 2 3"),
        json!({"w": ["a", "b"], "n": ["1", "2", "3"]})
    );
}

#[test]
fn labels_scope_their_captures() {
    assert_eq!(
        run("TEXT = pair:(k:WORD ' ' v:WORD) ' ' extra:WORD", "a b c"),
        json!({"pair": {"k": "a", "v": "b"}, "extra": "c"})
    );
}

#[test]
fn label_on_a_label_wraps() {
    assert_eq!(run("TEXT = outer:(inner:WORD)", "a"), json!({"outer": {"inner": "a"}}));
}

#[test]
fn parenthesised_group_passes_through() {
    assert_eq!(run("TEXT = (WORD OR FLOAT)", "5"), json!("5"));
    assert_eq!(run("TEXT = x:('a' OR 'b c')", "b c"), json!({"x": "b c"}));
}

#[test]
fn separators_and_skipped_text_are_not_captured() {
    assert_eq!(run("TEXT = 1 TO n n:FLOAT SPLITBY s:', '", "1, 2"), json!(["1", "2"]));
    assert_eq!(run("TEXT = 'x' 1 TO n n:FLOAT SKIPPING (w:WORD OR ' ')", "x a 1 b 2 c"), json!({"n": ["1", "2"]}));
}

#[test]
fn builtins_are_text() {
    assert_eq!(run("TEXT = 1 TO n LINE SPLITBY NL", "a b\n\nc"), json!(["a b", "", "c"]));
    assert_eq!(
        run("TEXT = 1 TO n (k:(ANY UNTIL ': ') v:(ANY UNTIL NL))", "a b: c d\ne: f\n"),
        json!([{"k": "a b", "v": "c d"}, {"k": "e", "v": "f"}])
    );
    assert_eq!(run("TEXT = k:(ANY UNTILBEFORE ':') ':'", ":"), json!({"k": ""}));
}

#[test]
fn digits_and_letters() {
    assert_eq!(run("TEXT = 1 TO n DIGIT", "987"), json!(["9", "8", "7"]));
    assert_eq!(run("TEXT = 1 TO n LETTER", "XMAS"), json!(["X", "M", "A", "S"]));
    assert_eq!(
        run("bank = 1 TO n ds:DIGIT AS [ NUM(d) FOR d IN ds ]\nTEXT = 1 TO n bank SPLITBY NL", "12\n34"),
        json!([[1, 2], [3, 4]])
    );
}

#[test]
fn helper_values_are_text() {
    assert_eq!(
        run("TEXT = ip:IPV4 ' ' h:('0x' HEX) ' ' b:BIN TAB i:INT", "10.0.0.1 0xff 101\t7"),
        json!({"ip": "10.0.0.1", "h": "0xff", "b": "101", "i": "7"})
    );
}

#[test]
fn whitespace_can_be_captured_and_counted() {
    assert_eq!(
        run("pair = ch:LETTER 1 TO n spaces:' ' AS { ch: COUNT(spaces) }\nTEXT = 1 TO n pair AS { pair }", "a   b  c "),
        json!({"a": 3, "b": 2, "c": 1})
    );
}

#[test]
fn aliases_do_not_capture() {
    assert_eq!(
        run("ALIAS eol = NL\nrhyme = noun:WORD ' are ' color:WORD eol\nTEXT = 1 TO n rhyme", "roses are red\n"),
        json!([{"noun": "roses", "color": "red"}])
    );
    // Rules inside an alias are not captured: the alias only contributes text.
    assert_eq!(run("w = WORD\nALIAS two = w ' ' w\nTEXT = x:WORD ' ' two", "a b c"), json!({"x": "a"}));
}

#[test]
fn until_through_aliases() {
    assert_eq!(
        run("ALIAS end = (' ' OR '\"')\nTEXT = p:(ANY UNTILBEFORE (end OR '?')) 1 TO n ANY", "/a?b c"),
        json!({"p": "/a"})
    );
}

#[test]
fn labelled_aliases() {
    // An alias captures nothing, so its value is the text it matched.
    assert_eq!(
        run("ALIAS rest = (ANY UNTILBEFORE NL)\nTEXT = k:WORD ': ' v:rest NL", "key: some value\n"),
        json!({"k": "key", "v": "some value"})
    );
    assert_eq!(run("ALIAS digits = 1 TO n DIGIT\nTEXT = d:digits", "123"), json!({"d": "123"}));
    assert_eq!(run("w = WORD\nALIAS two = w ' ' w\nTEXT = t:two", "a b"), json!({"t": "a b"}));
}

// ---- labels on repetitions, UNTILBEFORE / UNTIL ----

#[test]
fn a_label_on_a_repetition_captures_its_text() {
    assert_eq!(run("TEXT = ds:(1 TO n DIGIT)", "123"), json!({"ds": "123"}));
    // Separators and skipped text are part of the text.
    assert_eq!(run("TEXT = ws:(1 TO n WORD SPLITBY ', ')", "a, b"), json!({"ws": "a, b"}));
    assert_eq!(run("TEXT = o:(0 TO 1 WORD) '.'", "."), json!({"o": ""}));
}

#[test]
fn a_label_on_the_item_captures_a_list() {
    assert_eq!(run("TEXT = 1 TO n ds:DIGIT '.'", "123."), json!({"ds": ["1", "2", "3"]}));
    // Labels bind to the nearest pattern: here the item of the short form.
    assert_eq!(run("TEXT = k:ANY UNTILBEFORE '=' '=1'", "key=1"), json!({"k": ["k", "e", "y"]}));
    assert_eq!(run("TEXT = k:(ANY UNTILBEFORE '=') '=1'", "key=1"), json!({"k": "key"}));
    assert_eq!(run("TEXT = k:ANY ANY UNTIL '='", "key="), json!({"k": "k"}));
}

#[test]
fn the_stop_of_untilafter_is_never_in_a_value() {
    assert_eq!(run("TEXT = '\"' t:(ANY UNTIL '\"') '!'", "\"hi\"!"), json!({"t": "hi"}));
    assert_eq!(run("TEXT = 1 TO n cs:ANY UNTIL ';' '.'", "ab;."), json!({"cs": ["a", "b"]}));
    assert_eq!(run("TEXT = ws:(1 TO n WORD SPLITBY ', ' UNTIL '.')", "a, b."), json!({"ws": "a, b"}));
    // The default value of a rule that is a single repetition: its items.
    assert_eq!(run("TEXT = 1 TO n WORD SPLITBY ' ' UNTIL '.'", "a b."), json!(["a", "b"]));
}

#[test]
fn until_consumes_the_longest_stop() {
    // Whatever the order of the stops.
    assert_eq!(run("TEXT = l:(ANY UNTIL ('-' OR '--')) r:(ANY UNTIL NL)", "ab--x\n"), json!({"l": "ab", "r": "x"}));
    // Always the longest, even if the rest would need a shorter one: how much of the stop is
    // taken never makes a second reading.
    run_err("TEXT = l:(ANY UNTIL ('--' OR '-')) r:('-' WORD)", "ab--x");
    // A repeated stop takes the whole run.
    assert_eq!(
        run("TEXT = a:(ANY UNTIL sp) b:(ANY UNTIL NL)\nALIAS sp = 1 TO n ' '", "x   y\n"),
        json!({"a": "x", "b": "y"})
    );
}

#[test]
fn recursive_values() {
    assert_eq!(
        run("list = '(' 0 TO n items:(WORD OR list) SPLITBY ' ' ')' AS items\nTEXT = list", "(a (b) ())"),
        json!(["a", ["b"], []])
    );
}

// ---- templates ----

#[test]
fn literal_templates() {
    assert_eq!(
        run("TEXT = WORD AS { 's': 'x', 'i': 1, 'f': -2.5, 't': true, 'f2': false, 'n': null, 'a': [], 'o': {} }", "w"),
        json!({"s": "x", "i": 1, "f": -2.5, "t": true, "f2": false, "n": null, "a": [], "o": {}})
    );
}

#[test]
fn keys_from_captures() {
    assert_eq!(run("TEXT = k:WORD ' ' v:WORD AS { k: v }", "name ada"), json!({"name": "ada"}));
    assert_eq!(run("TEXT = k:FLOAT ' ' v:WORD AS { NUM(k): v }", "7 x"), json!({"7": "x"}));
}

#[test]
fn key_order_follows_the_template() {
    let v = run("TEXT = a:WORD ' ' b:WORD AS { 'z': a, 'a': b, 'm': a }", "x y");
    assert_eq!(serde_json::to_string(&v).unwrap(), r#"{"z":"x","a":"y","m":"x"}"#);
}

#[test]
fn duplicate_keys_last_wins() {
    assert_eq!(run("TEXT = a:WORD ' ' b:WORD AS { 'k': a, 'k': b }", "x y"), json!({"k": "y"}));
    assert_eq!(
        run("p = k:WORD ' ' v:FLOAT AS { k: v }\nTEXT = 1 TO n p SPLITBY ' ' AS { p }", "a 1 b 2 a 3"),
        json!({"a": "3", "b": "2"})
    );
}

const PAIRS: &str = "p = k:WORD ' ' v:FLOAT\n";

#[test]
fn for_each_in_arrays() {
    assert_eq!(run(&format!("{PAIRS}TEXT = 1 TO n p SPLITBY ' ' AS [ k FOR p ]"), "a 1 b 2"), json!(["a", "b"]));
    assert_eq!(run(&format!("{PAIRS}TEXT = 1 TO n p SPLITBY ' ' AS [ p.v FOR p ]"), "a 1 b 2"), json!(["1", "2"]));
}

#[test]
fn for_each_in_objects() {
    assert_eq!(
        run(&format!("{PAIRS}TEXT = 1 TO n p SPLITBY ' ' AS {{ k: NUM(v) FOR p }}"), "a 1 b 2"),
        json!({"a": 1, "b": 2})
    );
}

#[test]
fn for_each_in_explicit_source() {
    assert_eq!(
        run(
            "p = k:WORD ' ' 1 TO n vs:FLOAT SPLITBY ' '\nTEXT = 1 TO n p SPLITBY '; ' AS { k: [NUM(v) FOR v IN vs] FOR p }",
            "a 1 2; b 3"
        ),
        json!({"a": [1, 2], "b": [3]})
    );
}

#[test]
fn for_each_mixed_with_plain_elements() {
    assert_eq!(
        run("TEXT = 1 TO n w:WORD SPLITBY ' ' AS ['start', w FOR w, 'end']", "a b"),
        json!(["start", "a", "b", "end"])
    );
}

#[test]
fn for_each_over_non_objects_binds_only_the_variable() {
    assert_eq!(run("TEXT = 'x' 1 TO n (' ' w:WORD) AS [ UPPER(w) FOR w ]", "x a b"), json!(["A", "B"]));
}

#[test]
fn for_each_over_null_is_empty() {
    assert_eq!(run("TEXT = (a:WORD OR 'x' 1 TO n (' ' b:FLOAT)) AS [ b FOR b ]", "w"), json!([]));
}

#[test]
fn nested_for_each() {
    assert_eq!(
        run(
            "rec = name:WORD ': ' 1 TO n vals:FLOAT SPLITBY ', '\nTEXT = 1 TO n rec SPLITBY NL AS [ [ NUM(v) FOR v IN vals ] FOR rec ]",
            "a: 1, 2\nb: 3"
        ),
        json!([[1, 2], [3]])
    );
}

#[test]
fn inner_bindings_shadow_outer_ones() {
    assert_eq!(
        run("p = x:WORD\nTEXT = x:FLOAT 1 TO n (' ' p) AS { 'outer': x, 'inner': [x FOR p] }", "1 a b"),
        json!({"outer": "1", "inner": ["a", "b"]})
    );
}

#[test]
fn each_merges_objects() {
    let p = "p = k:WORD ' ' v:FLOAT AS { k: v }\n";
    assert_eq!(
        run(&format!("{p}TEXT = 1 TO n p SPLITBY ' ' AS {{ 'first': 'x', p }}"), "a 1 b 2"),
        json!({"first": "x", "a": "1", "b": "2"})
    );
    assert_eq!(run(&format!("{p}TEXT = one:p AS {{ one }}"), "a 1"), json!({"a": "1"}));
}

#[test]
fn functions() {
    assert_eq!(
        run(
            "TEXT = w:WORD ' ' 1 TO n ns:FLOAT SPLITBY ' ' AS { 'l': LOWER(w), 'u': UPPER(w), 'c': COUNT(ns), 'f': FIRST(ns), 'z': LAST(ns), 'j': JOIN(ns, '+'), 'n': NUM(FIRST(ns)), 't': TRIM(w) }",
            "Hello 1 2.5 3"
        ),
        json!({"l": "hello", "u": "HELLO", "c": 3, "f": "1", "z": "3", "j": "1+2.5+3", "n": 1, "t": "Hello"})
    );
}

#[test]
fn field_paths() {
    assert_eq!(run("inner = a:WORD ' ' b:(c:WORD)\nTEXT = x:inner AS [x.a, x.b.c]", "p q"), json!(["p", "q"]));
    // Fields of values whose shape is only known at runtime read as null when missing.
    assert_eq!(
        run("inner = k:WORD ' ' v:WORD AS { k: v }\nTEXT = x:inner AS [x.name, x.missing]", "name ada"),
        json!(["ada", null])
    );
}

#[test]
fn listof_collects_and_repeated_keys_are_reported() {
    let q = "TEXT = 1 TO n ps:p AS { x.k: LISTOF x.v FOR x IN ps, 'last': { x.k: x.v FOR x IN ps } }\np = k:WORD '=' v:INT NL";
    let o = run_opts(q, "a=1\nb=2\na=3\n", &Default::default());
    assert_eq!(o.value, json!({"a": ["1", "3"], "b": ["2"], "last": {"a": "3", "b": "2"}}));
    // One report for the entry that repeated a key.
    assert_eq!(o.repeated_keys.len(), 1);
    assert_eq!(o.repeated_keys[0].key, "a");
    // Strict runs make it an error.
    let strict = txtql::Options { strict: true, ..Default::default() };
    assert!(matches!(common::compile(q).run("a=1\na=3\n", &strict), Err(txtql::RunError::RepeatedKeys(_))));
}

#[test]
fn zip_pairs_keys_and_values() {
    assert_eq!(run("TEXT = WORD AS ZIP(['a', 'b', 'c'], [1, 2])", "x"), json!({"a": 1, "b": 2, "c": null}));
    assert!(run_err("TEXT = WORD AS ZIP(['a'], [1, 2])", "x").contains("`ZIP` got 2 values for 1 keys"));
}

#[test]
fn where_conditions() {
    let q = "big = k:WORD ' ' n:FLOAT WHERE NUM(n) >= 10 AND NOT k STARTSWITH 'x'\nother = WORD ' ' FLOAT\nTEXT = 1 TO n (big OR other) SPLITBY ' ' AS [ k FOR big ]";
    let o = run_opts(q, "a 5 b 50 xy 99 c 10", &Default::default());
    assert_eq!(o.value, json!(["b", "c"]));
}

// ---- runtime errors ----

#[test]
fn error_number_conversion() {
    assert_snapshot!(run_err("TEXT = w:WORD AS NUM(w)", "hello"));
}

#[test]
fn error_key_not_text() {
    assert_snapshot!(run_err("TEXT = 'x ' 1 TO n ws:WORD SPLITBY ' ' AS { ws: 1 }", "x a b"));
}

#[test]
fn error_merge_non_objects() {
    // `p` is text or an object, so this is only found when the query runs.
    assert_snapshot!(run_err("TEXT = 1 TO n p SPLITBY ' ' AS { p }\np = WORD OR n:INT", "a b"));
}

#[test]
fn for_each_over_text_is_caught_statically_when_the_shape_is_known() {
    let e = compile_err("p = w:WORD AS { 'w': w }\nTEXT = x:p AS [ c FOR c IN x.w ]");
    assert!(e.contains("txtql::check::not_repeated"), "{e}");
}

#[test]
fn error_for_each_over_text_at_runtime() {
    // A computed key makes the shape unknown statically, so it fails at runtime.
    let e = run_err("p = w:WORD AS { w: w }\nTEXT = x:p AS [ c FOR c IN x.a ]", "a");
    assert!(e.contains("`FOR` needs a list, but this is text"), "{e}");
}

#[test]
fn error_field_of_text() {
    let e = run_err("p = w:WORD AS w\nTEXT = x:p AS x.field", "a");
    assert!(e.contains("cannot read field `field` of text"), "{e}");
}

#[test]
fn error_in_where() {
    assert_snapshot!(run_err("TEXT = w:WORD WHERE w > 5", "abc"));
}

#[test]
fn error_upper_of_list() {
    let e = run_err("TEXT = 'x ' 1 TO n ws:WORD SPLITBY ' ' AS UPPER(ws)", "x a");
    assert!(e.contains("`UPPER` needs text, but got a list"), "{e}");
}

// ---- equality of texts ----

#[test]
fn equal_texts_compare_as_text_even_when_numeric() {
    assert_eq!(run("TEXT = a:FLOAT ' ' b:FLOAT WHERE a != b AS 'different'", "10 10.0"), json!("different"));
    let e = run_err("TEXT = a:FLOAT ' ' b:FLOAT WHERE a = b", "10 10.0");
    assert!(e.contains("no reading satisfies the WHERE conditions"), "{e}");
    assert_eq!(run("TEXT = WORD WHERE '10' != '10.0' AS 'ok'", "x"), json!("ok"));
    // A number on either side compares numerically.
    assert_eq!(run("TEXT = a:FLOAT ' ' b:FLOAT WHERE NUM(a) = b AS 'same'", "10 10.0"), json!("same"));
}

// ---- ZIP repeated keys ----

#[test]
fn zip_repeated_key_keeps_the_later_value_and_is_reported() {
    let o = run_opts("TEXT = WORD AS ZIP(['a', 'b', 'a'], [1, 2, 3])", "x", &Default::default());
    assert_eq!(o.value, json!({"a": 3, "b": 2}));
    assert_eq!(o.repeated_keys.len(), 1);
    assert_eq!(o.repeated_keys[0].key, "a");
    let q = "TEXT = WORD AS ZIP(['a', 'b', 'a'], [1, 2, 3])";
    let rendered = render(&txtql::repeated_key_reports(&o.repeated_keys, "query", q, "input", "x"));
    assert!(rendered.contains("txtql::eval::repeated_key"), "{rendered}");
    // Strict runs make it an error.
    let strict = txtql::Options { strict: true, ..Default::default() };
    assert!(matches!(compile(q).run("x", &strict), Err(txtql::RunError::RepeatedKeys(_))));
}

#[test]
fn zip_repeated_key_is_reported_once_per_call_per_run() {
    let o = run_opts("p = WORD AS ZIP(['a', 'a'], [1, 2])\nTEXT = 1 TO n p SPLITBY ' '", "x y z", &Default::default());
    assert_eq!(o.value, json!([{"a": 2}, {"a": 2}, {"a": 2}]));
    assert_eq!(o.repeated_keys.len(), 1);
}

// ---- no-parse hints ----

const ACCESS_LOG_QUERY: &str = "TEXT  = 1 TO n entry
entry = host:(ANY UNTIL ' ') '- - [' time:(ANY UNTIL ']') ' \"'
        method:WORD ' ' path:(ANY UNTIL ' ') protocol:(ANY UNTIL '\"') ' '
        status:FLOAT ' ' bytes:FLOAT NL";

const ACCESS_LOG_LINES: [&str; 3] = [
    "dnet018.sat.texas.net - - [01/Jul/1995:00:22:42 -0400] \"GET /history/apollo/apollo-13/ HTTP/1.0\" 200 1732",
    "pipe6.nyc.pipeline.com - - [01/Jul/1995:00:22:43 -0400] \"GET /shuttle/missions/sts-71/movies/sts-71-mir-dock.mpg\" 200 946425",
    "ix-sd9-18.ix.netcom.com - - [01/Jul/1995:00:22:43 -0400] \"GET /shuttle/missions/sts-71/mission-sts-71.html HTTP/1.0\" 200 12040",
];

#[test]
fn stop_across_lines_hint_counts_a_lone_cr_as_a_line_break() {
    let input = ACCESS_LOG_LINES.join("\r") + "\r";
    let e = run_err(ACCESS_LOG_QUERY, &input);
    assert!(e.contains("UNTIL started on line 2"), "{e}");
}

#[test]
fn stop_across_lines_hint_counts_crlf_once() {
    let input = ACCESS_LOG_LINES.join("\r\n") + "\r\n";
    let e = run_err(ACCESS_LOG_QUERY, &input);
    assert!(e.contains("UNTIL started on line 2"), "{e}");
}
