//! Static checks: one test per diagnostic code (rendered), plus queries that must stay clean.

mod common;
use common::*;
use insta::assert_snapshot;
use txtql::CompileError;
use txtql::error::CheckError;

fn codes(query: &str) -> Vec<String> {
    use miette::Diagnostic;
    match txtql::Query::compile(query) {
        Ok(q) => q.warnings.iter().map(|w| w.code().unwrap().to_string()).collect(),
        Err(CompileError::Check(errs)) => errs.iter().map(|e| e.code().unwrap().to_string()).collect(),
        Err(CompileError::Parse(e)) => panic!("parse error: {e:?}"),
    }
}

#[test]
fn missing_root() {
    assert_snapshot!(compile_err("a = WORD"));
}

#[test]
fn duplicate_rule() {
    assert_snapshot!(compile_err("a = WORD\na = FLOAT\nTEXT = a"));
}

#[test]
fn undefined_rule_with_suggestion() {
    assert_snapshot!(compile_err("color = WORD\nTEXT = 1 TO n colr"));
}

#[test]
fn keywords_and_root_in_any_case() {
    assert!(codes("text = 1 to n w:word splitby ' ' as w").is_empty());
    assert!(codes("Text = Word Nl").is_empty());
    // Every spelling of the root's name is the same name.
    assert_eq!(codes("TEXT = WORD\ntext = WORD"), ["txtql::check::duplicate_rule"]);
    // The root's name is not reserved: captures may be called `text`.
    assert!(codes("TEXT = text:WORD AS text").is_empty());
}

#[test]
fn duplicate_capture_labels() {
    assert_snapshot!(compile_err("TEXT = x:WORD x:FLOAT"));
}

#[test]
fn duplicate_capture_implicit() {
    assert_snapshot!(compile_err("color = WORD\nTEXT = color 'and' color"));
}

#[test]
fn duplicate_capture_across_repetitions() {
    assert_eq!(codes("TEXT = 1 TO n x:WORD 1 TO n x:FLOAT"), ["txtql::check::duplicate_capture"]);
}

#[test]
fn same_capture_in_alternatives_is_fine() {
    assert!(codes("TEXT = x:WORD OR x:FLOAT").is_empty());
    assert!(codes("TEXT = (x:WORD OR x:FLOAT) 'end'").is_empty());
}

#[test]
fn renaming_a_repeated_rule_reference_is_fine() {
    assert!(codes("color = WORD\nTEXT = a:color 'and' b:color").is_empty());
}

#[test]
fn unknown_capture_in_template() {
    assert_snapshot!(compile_err("colors = 1 TO n WORD\nTEXT = noun:WORD colors AS { noun: colours }"));
}

#[test]
fn unknown_capture_as_key_suggests_quoting() {
    assert_snapshot!(compile_err("TEXT = x:WORD AS { name: x }"));
}

#[test]
fn unknown_capture_in_where() {
    assert_snapshot!(compile_err("TEXT = x:WORD WHERE y = 'a'"));
}

#[test]
fn unknown_capture_when_nothing_is_captured() {
    assert_snapshot!(compile_err("TEXT = WORD AS x"));
}

#[test]
fn unknown_field_of_known_object() {
    assert_snapshot!(compile_err("rhyme = noun:WORD 'are' c:WORD\nTEXT = r:rhyme AS r.nown"));
}

#[test]
fn fields_of_repeated_objects_are_in_scope() {
    assert!(codes("rhyme = noun:WORD 'are' c:WORD\nTEXT = 1 TO n rhyme AS [ noun FOR rhyme ]").is_empty());
    assert!(codes("rhyme = noun:WORD 'are' c:WORD\nTEXT = 1 TO n rhyme AS [ rhyme.noun FOR rhyme ]").is_empty());
    assert!(
        codes("rhyme = noun:WORD 'are' 1 TO n cs:WORD\nTEXT = 1 TO n rhyme AS [ [c FOR c IN cs] FOR rhyme ]")
            .is_empty()
    );
}

#[test]
fn unknown_field_inside_for_each() {
    assert_eq!(
        codes("rhyme = noun:WORD 'are' c:WORD\nTEXT = 1 TO n rhyme AS [ nown FOR rhyme ]"),
        ["txtql::check::unknown_capture"]
    );
}

#[test]
fn names_in_templates_of_unknown_shape_are_allowed() {
    // `r` has a template with computed keys, so fields cannot be checked statically.
    assert!(codes("r = k:WORD v:WORD AS { k: v }\nTEXT = 1 TO n r AS [ anything FOR r ]").is_empty());
}

#[test]
fn not_repeated() {
    assert_snapshot!(compile_err("TEXT = x:WORD AS [ y FOR x ]"));
}

#[test]
fn not_repeated_with_in() {
    assert_eq!(codes("TEXT = x:WORD AS [ y FOR y IN x ]"), ["txtql::check::not_repeated"]);
}

#[test]
fn duplicate_branch() {
    assert_snapshot!(compile_err("TEXT = ('a' OR WORD OR 'a')"));
    // Aliases are compared by what they stand for.
    assert_eq!(codes("ALIAS sp = ' '\nTEXT = (' ' OR sp)"), ["txtql::check::duplicate_branch"]);
    assert!(codes("TEXT = ('a' OR i'a')").is_empty());
    // A self-referencing alias is compared without expanding it (it used to grow exponentially).
    assert!(codes("ALIAS a0 = (DIGIT OR a0 OR a0)\nTEXT = a0").contains(&"txtql::check::alias_cycle".to_string()));
}

#[test]
fn not_mergeable() {
    assert_snapshot!(compile_err("TEXT = user:WORD AS { user }"));
}

#[test]
fn not_mergeable_list() {
    assert_eq!(codes("TEXT = 1 TO n w:WORD SPLITBY ' ' AS { w }"), ["txtql::check::not_mergeable"]);
    // Objects, lists of objects and unknown shapes are fine.
    assert!(codes("TEXT = 1 TO n p SPLITBY ' ' AS { p }\np = k:WORD AS { k: 1 }").is_empty());
}

#[test]
fn empty_loop() {
    assert_snapshot!(compile_err("TEXT = 1 TO n (0 TO 1 WORD)"));
}

#[test]
fn empty_loop_until_and_skip() {
    assert_eq!(codes("TEXT = 1 TO n (ANY UNTILBEFORE ',')"), ["txtql::check::empty_loop"]);
    assert_eq!(codes("TEXT = 1 TO n WORD SKIPPING (0 TO n PUNCT)"), ["txtql::check::empty_loop"]);
    assert_eq!(codes("e = 0 TO 1 WORD\nTEXT = 1 TO n e"), ["txtql::check::empty_loop"]);
    assert!(codes("TEXT = 0 TO 0 (0 TO 1 WORD)").is_empty(), "a loop that never runs is fine");
}

#[test]
fn empty_items_are_fine_with_a_consuming_separator() {
    // Lines can be empty, but NL between them always consumes text.
    assert!(codes("TEXT = 1 TO n LINE SPLITBY NL").is_empty());
    assert_eq!(codes("TEXT = 1 TO n LINE"), ["txtql::check::empty_loop"]);
    assert_eq!(codes("TEXT = 1 TO n LINE SPLITBY (0 TO 1 ',')"), ["txtql::check::empty_loop"]);
}

#[test]
fn empty_cycle() {
    assert_snapshot!(compile_err("a = b\nb = a OR WORD\nTEXT = a"));
}

#[test]
fn empty_cycle_through_nullable_context() {
    assert_eq!(codes("a = 0 TO 1 'x' a 0 TO 1 'y' OR WORD\nTEXT = a"), ["txtql::check::empty_cycle"]);
    assert_eq!(codes("a = 1 TO n a OR WORD\nTEXT = a"), ["txtql::check::empty_cycle"]);
}

#[test]
fn consuming_recursion_is_fine() {
    assert!(codes("list = '(' 0 TO n (WORD OR list) ')'\nTEXT = list").is_empty());
    assert!(codes("expr = expr '+' FLOAT OR FLOAT\nTEXT = expr").is_empty(), "left recursion is fine");
}

#[test]
fn bad_bounds() {
    assert_snapshot!(compile_err("TEXT = 3 TO 2 WORD"));
}

#[test]
fn bound_too_large() {
    assert_snapshot!(compile_err("TEXT = 1 TO 20000 WORD"));
    assert_eq!(codes("TEXT = 99999 TO n WORD"), ["txtql::check::bound_too_large"]);
}

#[test]
fn bad_stop() {
    // Rules and labels are not allowed in a stop.
    assert_snapshot!(compile_err("TEXT = (ANY UNTILBEFORE w)\nw = WORD"));
    assert_eq!(codes("TEXT = (ANY UNTILBEFORE ANY)"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = (ANY UNTILBEFORE ROW)"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = (ANY UNTIL (1 TO n ' ' SPLITBY ','))"), ["txtql::check::bad_stop"]);
    // Sequences, repetitions and EOF are fine.
    assert!(codes("TEXT = (ANY UNTILBEFORE (NL DIGIT OR NL EOF)) (ANY UNTIL 1 TO n ' ') (ANY UNTIL EOF)").is_empty());
    // A stop that can be empty would end the repetition at once.
    assert_eq!(codes("TEXT = (ANY UNTIL (0 TO 1 'x'))"), ["txtql::check::empty_stop"]);
    assert!(
        codes("TEXT = (ANY UNTILBEFORE WORD) (ANY UNTILBEFORE FLOAT) (ANY UNTILBEFORE PUNCT) (ANY UNTILBEFORE 'a b') (ANY UNTILBEFORE DIGIT) (ANY UNTILBEFORE LETTER) (ANY UNTILBEFORE NL)").is_empty()
    );
    assert!(codes("TEXT = (ANY UNTILBEFORE (' ' OR '\"' OR NL))").is_empty());
    assert!(codes("TEXT = (ANY UNTILBEFORE (' ' OR WORD WORD))").is_empty());
    assert!(codes("TEXT = (ANY UNTILBEFORE (INT OR HEX OR BIN OR IPV4 OR TAB OR NL))").is_empty());
    assert_eq!(codes("TEXT = (ANY UNTILBEFORE LINE)"), ["txtql::check::bad_stop"]);
    // Aliases are looked through, as if their pattern were written in place.
    assert!(codes("ALIAS quote = (' ' OR '\"')\nTEXT = (ANY UNTILBEFORE (quote OR '?'))").is_empty());
    assert_eq!(codes("w = WORD\nALIAS two = w w\nTEXT = (ANY UNTILBEFORE (two OR '?'))"), ["txtql::check::bad_stop"]);
}

#[test]
fn bad_stop_cases_that_remain() {
    // bad_stop is for defined rules, labels, ANY alone, LINE/ROW/COL and nested clauses.
    assert_eq!(codes("w = WORD\nTEXT = w ANY UNTIL w"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = ANY UNTIL x:'a'"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = ANY UNTIL ANY"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = ANY UNTILBEFORE COL"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = ANY UNTIL (1 TO n 'a' SKIPPING ' ')"), ["txtql::check::bad_stop"]);
    assert_eq!(codes("TEXT = ANY UNTIL (1 TO n 'a' UNTIL 'b')"), ["txtql::check::bad_stop"]);
}

// ---- first-pass checks run inside stops (CheckStops, lang/StopsArePlainPatterns) ----

#[test]
fn bounds_are_checked_inside_stops() {
    assert_eq!(codes("TEXT = ANY UNTIL 3 TO 1 'a'"), ["txtql::check::bad_bounds"]);
    assert_eq!(codes("TEXT = ANY UNTILBEFORE (3 TO 1 'a' OR 'b')"), ["txtql::check::bad_bounds"]);
    assert_eq!(codes("TEXT = ANY UNTIL 1 TO 99999 'a'"), ["txtql::check::bound_too_large"]);
    // A stop that can be empty may also be reported as empty_stop; the bound is still checked.
    let c = codes("TEXT = ANY UNTIL 0 TO 99999 'a'");
    assert!(c.contains(&"txtql::check::bound_too_large".to_string()), "{c:?}");
    assert!(!c.contains(&"txtql::check::bad_stop".to_string()), "{c:?}");
}

#[test]
fn duplicate_branches_are_checked_inside_stops() {
    assert_eq!(codes("TEXT = ANY UNTIL ('a' OR 'a')"), ["txtql::check::duplicate_branch"]);
    assert_eq!(codes("TEXT = ANY UNTILBEFORE ('a' OR WORD OR 'a')"), ["txtql::check::duplicate_branch"]);
}

#[test]
fn undefined_name_in_a_stop_is_undefined_rule_with_hint() {
    let q = "fooo2 = WORD\nTEXT = fooo2 ANY UNTIL fooo";
    assert_eq!(codes(q), ["txtql::check::undefined_rule"]);
    let e = compile_err(q);
    assert!(e.contains("did you mean `fooo2`?"), "{e}");
    assert!(!e.contains("bad_stop"), "{e}");
    assert_eq!(codes("TEXT = ANY UNTILBEFORE (';' OR nothere)"), ["txtql::check::undefined_rule"]);
}

#[test]
fn empty_literal_in_a_stop_is_empty_literal() {
    for q in ["TEXT = ANY UNTIL ''", "TEXT = ANY UNTILBEFORE (';' OR '')"] {
        let c = codes(q);
        assert!(c.contains(&"txtql::check::empty_literal".to_string()), "{q}: {c:?}");
        assert!(!c.contains(&"txtql::check::bad_stop".to_string()), "{q}: {c:?}");
    }
}

#[test]
fn pattern_checks_run_inside_splitby_and_skipping() {
    assert_eq!(codes("TEXT = 1 TO n WORD SPLITBY (',' OR ',')"), ["txtql::check::duplicate_branch"]);
    assert_eq!(codes("TEXT = 1 TO n WORD SKIPPING (' ' OR ' ')"), ["txtql::check::duplicate_branch"]);
    assert_eq!(codes("TEXT = 1 TO n WORD SPLITBY 3 TO 1 ','"), ["txtql::check::bad_bounds"]);
    assert_eq!(codes("TEXT = 1 TO n WORD SPLITBY nothere"), ["txtql::check::undefined_rule"]);
    assert!(codes("TEXT = 1 TO n WORD SPLITBY ''").contains(&"txtql::check::empty_literal".to_string()));
}

#[test]
fn bound_too_large_reports_the_bound_as_written() {
    for (q, written) in [
        ("TEXT = 5000000000 TO n WORD", "5000000000"),
        ("TEXT = 1 TO 5000000000 WORD", "5000000000"),
        ("TEXT = 1 TO 18446744073709551615 WORD", "18446744073709551615"),
    ] {
        assert_eq!(codes(q), ["txtql::check::bound_too_large"], "{q}");
        let e = compile_err(q);
        assert!(e.contains(&format!("repetition bound {written} is too large")), "{q}\n{e}");
        assert!(!e.contains("4294967295"), "{q}: bound must not be clamped\n{e}");
    }
    // Beyond u64 the bound is not a number at all.
    let e = compile_err("TEXT = 99999999999999999999 TO n WORD");
    assert!(e.contains("txtql::parse::bad_number"), "{e}");
}

#[test]
fn empty_literal() {
    assert_snapshot!(compile_err("TEXT = WORD ''"));
    assert!(codes("TEXT = WORD '  '").is_empty(), "spaces are ordinary characters");
}

#[test]
fn unknown_function() {
    assert_snapshot!(compile_err("TEXT = x:WORD AS LOWR(x)"));
}

#[test]
fn function_names_in_any_case() {
    assert!(codes("TEXT = x:WORD AS lower(x)").is_empty());
    assert!(codes("TEXT = x:WORD AS Join([x], '')").is_empty());
}

#[test]
fn arity() {
    assert_snapshot!(compile_err("TEXT = x:WORD AS JOIN(x, ',', ';')"));
    assert_eq!(codes("TEXT = x:WORD AS NUM()"), ["txtql::check::arity"]);
}

#[test]
fn lint_unused_rule() {
    assert_snapshot!(warnings("unused = WORD\nTEXT = FLOAT"));
}

#[test]
fn lint_nested_repetition() {
    assert_snapshot!(warnings("TEXT = 1 TO n (1 TO n WORD)"));
    assert!(codes("TEXT = 1 TO n (1 TO n WORD SPLITBY ',')").is_empty());
    assert!(codes("TEXT = 1 TO n ('-' 1 TO n WORD)").is_empty(), "anchored by a literal");
}

#[test]
fn lint_right_recursion() {
    assert_snapshot!(warnings("list = WORD ',' list OR WORD\nTEXT = list"));
}

#[test]
fn several_errors_are_reported_together_in_source_order() {
    let q = "TEXT = 3 TO 1 WORD '' (ANY UNTILBEFORE ANY)";
    let CompileError::Check(errs) = compile_error(q) else { panic!() };
    let kinds: Vec<&str> = errs
        .iter()
        .map(|e| match e {
            CheckError::BadBounds { .. } => "bounds",
            CheckError::EmptyLiteral { .. } => "literal",
            CheckError::BadStop { .. } => "stop",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["bounds", "literal", "stop"]);
}

#[test]
fn realistic_queries_are_clean() {
    for q in [
        "eol = NL\ncolor = WORD\ncolors = 1 TO n color SPLITBY (' and ' OR ', ')\nrhyme = noun:WORD ' are ' colors AS { noun: colors }\nTEXT = 1 TO n rhymes:rhyme SPLITBY eol eol AS { rhymes }",
        "eol = NL\ncolor = WORD\ncolors = 1 TO n color SPLITBY (' and ' OR ', ')\nrhyme = noun:WORD ' are ' colors\nTEXT = 1 TO n rhymes:rhyme SPLITBY eol eol AS { 'nouns': [noun FOR rhymes], 'adjectives': [colors FOR rhymes] }",
        "field = key:(ANY UNTIL ':') ' ' value:(ANY UNTILBEFORE NL)\nTEXT = 1 TO n fields:field SPLITBY NL NL AS { key: value FOR fields }",
        "entry = '[' ts:(ANY UNTIL ']') ' ' level:WORD ' ' msg:(ANY UNTILBEFORE NL) WHERE level != 'DEBUG' AS { 'at': ts, 'level': LOWER(level), 'msg': msg }\nTEXT = 1 TO n (entry NL) SKIPPING (LINE NL) AS [ entry FOR entry ]",
    ] {
        assert!(codes(q).is_empty(), "{q}\n{:?}", codes(q));
    }
}

#[test]
fn lint_warnings_do_not_hide_later_errors() {
    // Found by fuzzing: a first-pass lint used to stop all later checks.
    assert_eq!(codes("TEXT = 0 TO n (0 TO n WORD)"), ["txtql::lint::nested_repetition", "txtql::check::empty_loop"]);
    assert_eq!(
        codes("TEXT = 1 TO n (1 TO n WORD) AS nope"),
        ["txtql::lint::nested_repetition", "txtql::check::unknown_capture"]
    );
}

// ---- aliases ----

#[test]
fn label_in_alias() {
    assert_snapshot!(compile_err("ALIAS date = y:FLOAT '-' m:FLOAT\nTEXT = date"));
}

#[test]
fn alias_cycle() {
    assert_snapshot!(compile_err("ALIAS a = 'x' b\nALIAS b = 'y' OR a\nTEXT = a"));
    assert_eq!(codes("ALIAS a = 'x' a\nTEXT = a"), ["txtql::check::alias_cycle"]);
}

#[test]
fn root_is_alias() {
    assert_snapshot!(compile_err("ALIAS TEXT = WORD"));
}

#[test]
fn unused_alias() {
    assert_snapshot!(warnings("ALIAS sp = ' '\nTEXT = WORD"));
}

#[test]
fn rules_and_aliases_share_names() {
    assert_eq!(codes("ALIAS w = WORD\nw = FLOAT\nTEXT = w"), ["txtql::check::duplicate_rule"]);
}

#[test]
fn aliases_are_never_captured() {
    // Used twice in one sequence without a duplicate capture.
    assert!(
        codes("ALIAS eol = NL\nTEXT = WORD eol WORD eol AS eol").len() == 1,
        "{:?}",
        codes("ALIAS eol = NL\nTEXT = WORD eol WORD eol AS eol")
    );
    assert_eq!(codes("ALIAS eol = NL\nTEXT = WORD eol WORD eol AS eol"), ["txtql::check::unknown_capture"]);
    // A rule referenced inside an alias is not captured either.
    assert_eq!(codes("w = WORD\nALIAS two = w ' ' w\nTEXT = two AS w"), ["txtql::check::unknown_capture"]);
    // Labelling the alias where it is used captures it.
    assert!(codes("ALIAS rest = (ANY UNTILBEFORE NL)\nTEXT = r:rest NL AS r").is_empty());
}

#[test]
fn checks_see_through_aliases() {
    assert_eq!(codes("ALIAS opt = 0 TO 1 WORD\nTEXT = 1 TO n opt"), ["txtql::check::empty_loop"]);
    assert_eq!(codes("ALIAS self = TEXT\nTEXT = self OR WORD"), ["txtql::check::empty_cycle"]);
    assert_eq!(codes("ALIAS a = nope\nTEXT = a"), ["txtql::check::undefined_rule"]);
}

// ---- JOIN needs its separator ----

#[test]
fn join_without_separator_is_an_arity_error() {
    assert_eq!(codes("TEXT = 1 TO n x:WORD SPLITBY ' ' AS JOIN(x)"), ["txtql::check::arity"]);
    let e = compile_err("TEXT = 1 TO n x:WORD SPLITBY ' ' AS JOIN(x)");
    // The hint shows the call written out with a separator.
    assert!(e.contains(", ' ')"), "{e}");
    assert!(codes("TEXT = 1 TO n x:WORD SPLITBY ' ' AS JOIN(x, ' ')").is_empty());
    assert_eq!(run("TEXT = 1 TO n x:WORD SPLITBY ' ' AS JOIN(x, ' ')", "a b c"), serde_json::json!("a b c"));
}

// ---- true, false and null are not names ----

#[test]
fn reserved_constant_names() {
    for q in [
        "null = WORD\nTEXT = null",
        "True = WORD\nTEXT = True",
        "FALSE = WORD\nTEXT = FALSE",
        "ALIAS null = WORD\nTEXT = null",
        "ALIAS True = WORD\nTEXT = True",
        "ALIAS false = WORD\nTEXT = false",
        "TEXT = null:WORD",
        "TEXT = TRUE:WORD",
        "TEXT = False:WORD",
    ] {
        assert!(codes(q).contains(&"txtql::check::reserved_name".to_string()), "{q}\n{:?}", codes(q));
    }
}

#[test]
fn reserved_constant_loop_variables() {
    for q in [
        "TEXT = 1 TO n w:WORD SPLITBY ' ' AS [ true FOR true IN w ]",
        "TEXT = 1 TO n w:WORD SPLITBY ' ' AS [ x FOR Null IN w ]",
        "TEXT = 1 TO n w:WORD SPLITBY ' ' AS [ 1 FOR FALSE IN w ]",
    ] {
        assert!(codes(q).contains(&"txtql::check::reserved_name".to_string()), "{q}\n{:?}", codes(q));
    }
    // Ordinary loop variables and `.null` fields are fine.
    assert!(codes("TEXT = 1 TO n w:WORD SPLITBY ' ' AS [ x FOR x IN w ]").is_empty());
    assert!(codes("p = w:WORD AS { 'null': w }\nTEXT = 1 TO n x:p SPLITBY ' ' AS [ y.null FOR y IN x ]").is_empty());
}

#[test]
fn reserved_names_do_not_affect_fields_or_similar_names() {
    // Field names after `.` are free.
    assert!(codes("p = w:WORD AS { 'null': w, 'true': w }\nTEXT = x:p AS [x.null, x.true]").is_empty());
    assert_eq!(
        run("p = w:WORD AS { 'null': w, 'True': w }\nTEXT = x:p AS [x.null, x.True]", "a"),
        serde_json::json!(["a", "a"])
    );
    // Names that merely contain them are ordinary.
    assert!(codes("nullable = WORD\nTEXT = nullable").is_empty());
    assert!(codes("TEXT = truth:WORD ' ' falsey:WORD AS [truth, falsey]").is_empty());
    assert!(codes("ALIAS nothing = WORD\nTEXT = n:nothing AS n").is_empty());
}
