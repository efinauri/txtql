//! Grammar IR: snapshots of the NFA built for each construct.

mod common;
use insta::assert_snapshot;

fn dump(q: &str) -> String {
    common::compile(q).grammar.dump()
}

#[test]
fn literals_are_single_terminals() {
    // Every character is significant, spaces included: a literal matches its text exactly.
    assert_snapshot!(dump("TEXT = 'e-mail' ' ' 'black and'"));
}

#[test]
fn case_insensitive_literal() {
    assert_snapshot!(dump("TEXT = i'Are'"));
}

#[test]
fn alternatives_share_start_and_accept() {
    assert_snapshot!(dump("TEXT = 'a' 'b' OR 'c'"));
}

#[test]
fn nested_or_becomes_a_group() {
    assert_snapshot!(dump("TEXT = 'a' ('b' OR 'c')"));
}

#[test]
fn labels() {
    // A label on one symbol annotates it; on several symbols it wraps them in a group.
    assert_snapshot!(dump("r = WORD\nTEXT = x:WORD y:r z:(WORD WORD) w:(v:WORD)"));
}

#[test]
fn unbounded_repetition() {
    assert_snapshot!(dump("TEXT = 1 TO n WORD"));
}

#[test]
fn optional() {
    assert_snapshot!(dump("TEXT = 0 TO 1 WORD"));
}

#[test]
fn bounded_repetition() {
    assert_snapshot!(dump("TEXT = 2 TO 3 WORD"));
}

#[test]
fn min_above_one_unbounded() {
    assert_snapshot!(dump("TEXT = 3 TO n WORD"));
}

#[test]
fn separated_repetition() {
    assert_snapshot!(dump("TEXT = 1 TO n x:WORD SPLITBY ','"));
}

#[test]
fn skipping_repetition() {
    assert_snapshot!(dump("TEXT = 1 TO n FLOAT SKIPPING ANY"));
}

#[test]
fn lazy_repetition() {
    assert_snapshot!(dump("TEXT = 0 TO n LAZY WORD"));
}

#[test]
fn zero_to_zero() {
    assert_snapshot!(dump("TEXT = 0 TO 0 WORD 'x'"));
}

#[test]
fn until_and_line() {
    assert_snapshot!(dump("TEXT = (ANY UNTILBEFORE ':') LINE NL"));
}

#[test]
fn character_primitives() {
    assert_snapshot!(dump("TEXT = DIGIT LETTER (ANY UNTILBEFORE DIGIT) (ANY UNTILBEFORE NL)"));
}

#[test]
fn capture_names_per_alternative() {
    assert_snapshot!(dump("TEXT = a:WORD b:WORD OR c:FLOAT"));
}

#[test]
fn aliases_compile_to_shared_groups_without_captures() {
    assert_snapshot!(dump("w = WORD\nALIAS pair = w ' ' w\nTEXT = pair NL x:pair"));
}

#[test]
fn until_with_alternatives() {
    assert_snapshot!(dump("TEXT = (ANY UNTILBEFORE (' ' OR '\"' OR NL))"));
}
