//! Shared checks for the fuzz targets and the stable `smoke` runner.

use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
enum Pat {
    Lit(u8),
    Word,
    Number,
    Punct,
    Any,
    Digit,
    Letter,
    Newline,
    Tab,
    Int,
    Hex,
    Bin,
    Ipv4,
    Ipv6,
    Line,
    Row,
    Col,
    Eof,
    /// `ANY UNTILBEFORE` / `ANY UNTIL` a line break followed by a literal, or by the end.
    UntilSeq(u8, bool),
    /// `ANY UNTILBEFORE` / `ANY UNTIL` a literal (`after` picks which).
    UntilLit(u8, bool),
    UntilOr(u8, u8, bool),
    Ref(u8),
    AliasRef(u8),
    Label(u8, Box<Pat>),
    Seq(Vec<Pat>),
    Or(Vec<Pat>),
    Repeat { min: u8, max: Option<u8>, lazy: bool, item: Box<Pat>, sep: Option<u8>, skip: bool, until: Option<(u8, bool)> },
}

#[derive(Arbitrary, Debug)]
enum Tmpl {
    Str(u8),
    Num(i8),
    Path(u8, Option<u8>),
    Call(u8, Vec<Tmpl>),
    /// Entries: key, value, `FOR`, `LISTOF`.
    Object(Vec<(Tmpl, Tmpl, Option<u8>, bool)>),
    Merge(u8),
    Array(Vec<(Tmpl, Option<u8>)>),
}

#[derive(Arbitrary, Debug)]
struct Rule {
    pattern: Pat,
    filter: Option<(Tmpl, u8, Tmpl)>,
    template: Option<Tmpl>,
}

#[derive(Arbitrary, Debug)]
pub struct Case {
    /// Alias bodies; labels in them are rejected by the checker, so those run unchecked.
    aliases: Vec<Pat>,
    rules: Vec<Rule>,
    input: Vec<(u8, u8)>,
}

const LITS: &[&str] = &["a", "b", ",", "1", " ", ", ", "\\n", "a b", "x-y", ";", " are ", "A"];
const NAMES: &[&str] = &["p", "q", "s", "t", "noun"];
const FUNCS: &[&str] = &["NUM", "LOWER", "UPPER", "TRIM", "COUNT", "FIRST", "LAST", "JOIN", "ZIP", "num"];
const OPS: &[&str] = &["=", "!=", "<", ">=", "CONTAINS", "STARTSWITH"];
const TOKENS: &[&str] = &["a", "b", "x", "-", "y", ",", ";", "1", "42", "are", "é", "A", "\r", "ff", "10.0.0.1", "\t"];
const GAPS: &[&str] = &[" ", "\n", "", "  ", "\r\n"];

fn pick<'a>(xs: &[&'a str], i: u8) -> &'a str {
    xs[i as usize % xs.len()]
}

fn rule_name(i: usize) -> String {
    if i == 0 { "TEXT".into() } else { format!("r{i}") }
}

/// Names in scope for generated references.
#[derive(Clone, Copy)]
struct Names {
    rules: usize,
    aliases: usize,
}

fn pat(p: &Pat, names: Names, depth: u32, out: &mut String) {
    let nrules = names.rules;
    if depth > 6 {
        out.push_str("WORD");
        return;
    }
    match p {
        Pat::Lit(i) => out.push_str(&format!("'{}'", pick(LITS, *i))),
        Pat::Word => out.push_str("WORD"),
        Pat::Number => out.push_str("FLOAT"),
        Pat::Punct => out.push_str("PUNCT"),
        Pat::Any => out.push_str("ANY"),
        Pat::Digit => out.push_str("DIGIT"),
        Pat::Letter => out.push_str("LETTER"),
        Pat::Newline => out.push_str("NL"),
        Pat::Tab => out.push_str("TAB"),
        Pat::Int => out.push_str("INT"),
        Pat::Hex => out.push_str("HEX"),
        Pat::Bin => out.push_str("BIN"),
        Pat::Ipv4 => out.push_str("IPV4"),
        Pat::Ipv6 => out.push_str("IPV6"),
        Pat::Line => out.push_str("LINE"),
        Pat::Row => out.push_str("ROW"),
        Pat::Col => out.push_str("col"),
        Pat::Eof => out.push_str("EOF"),
        Pat::UntilSeq(i, after) => out.push_str(&format!(
            "(ANY {} (NL '{}' OR NL EOF OR 1 TO n ' '))",
            until_kw(*after),
            pick(LITS, *i)
        )),
        Pat::UntilLit(i, after) => out.push_str(&format!("(ANY {} '{}')", until_kw(*after), pick(LITS, *i))),
        Pat::UntilOr(a, b, after) => out.push_str(&format!(
            "(ANY {} ('{}' OR '{}' OR NL))",
            until_kw(*after),
            pick(LITS, *a),
            pick(LITS, *b)
        )),
        // References may be recursive; the checker rejects the bad kinds.
        Pat::Ref(i) => out.push_str(&rule_name(*i as usize % nrules)),
        // Aliases may refer to each other in cycles; the checker rejects those.
        Pat::AliasRef(i) if names.aliases > 0 => out.push_str(&format!("a{}", *i as usize % names.aliases)),
        Pat::AliasRef(_) => out.push_str("WORD"),
        Pat::Label(n, inner) => {
            out.push_str(pick(NAMES, *n));
            out.push_str(":(");
            pat(inner, names, depth + 1, out);
            out.push(')');
        }
        Pat::Seq(items) | Pat::Or(items) => {
            if items.is_empty() {
                out.push_str("ANY");
                return;
            }
            out.push('(');
            for (k, item) in items.iter().take(4).enumerate() {
                if k > 0 {
                    out.push_str(if matches!(p, Pat::Or(_)) { " OR " } else { " " });
                }
                pat(item, names, depth + 1, out);
            }
            out.push(')');
        }
        Pat::Repeat { min, max, lazy, item, sep, skip, until } => {
            let min = min % 4;
            let max = match max {
                Some(m) => (min + m % 4).to_string(),
                None => "n".into(),
            };
            out.push_str(&format!("({min} TO {max}{} (", if *lazy { " LAZY" } else { "" }));
            pat(item, names, depth + 1, out);
            out.push(')');
            if let Some(s) = sep {
                out.push_str(&format!(" SPLITBY '{}'", pick(LITS, *s)));
            }
            if *skip {
                out.push_str(" SKIPPING PUNCT");
            }
            if let Some((s, after)) = until {
                out.push_str(&format!(" {} '{}'", until_kw(*after), pick(LITS, *s)));
            }
            out.push(')');
        }
    }
}

fn tmpl(t: &Tmpl, depth: u32, out: &mut String) {
    if depth > 4 {
        out.push_str("null");
        return;
    }
    let each = |e: &Option<u8>| e.map(|n| format!(" FOR {}", pick(NAMES, n))).unwrap_or_default();
    match t {
        Tmpl::Str(i) => out.push_str(&format!("'{}'", pick(LITS, *i))),
        Tmpl::Num(n) => out.push_str(&n.to_string()),
        Tmpl::Path(a, b) => {
            out.push_str(pick(NAMES, *a));
            if let Some(b) = b {
                out.push('.');
                out.push_str(pick(NAMES, *b));
            }
        }
        Tmpl::Call(f, args) => {
            out.push_str(pick(FUNCS, *f));
            out.push('(');
            for (k, a) in args.iter().take(3).enumerate() {
                if k > 0 {
                    out.push_str(", ");
                }
                tmpl(a, depth + 1, out);
            }
            out.push(')');
        }
        Tmpl::Object(entries) => {
            out.push('{');
            for (k, (key, value, e, listof)) in entries.iter().take(4).enumerate() {
                if k > 0 {
                    out.push_str(", ");
                }
                tmpl(key, depth + 1, out);
                out.push_str(if *listof { ": LISTOF " } else { ": " });
                tmpl(value, depth + 1, out);
                out.push_str(&each(e));
            }
            out.push('}');
        }
        Tmpl::Merge(n) => out.push_str(&format!("{{ {} }}", pick(NAMES, *n))),
        Tmpl::Array(elems) => {
            out.push('[');
            for (k, (v, e)) in elems.iter().take(4).enumerate() {
                if k > 0 {
                    out.push_str(", ");
                }
                tmpl(v, depth + 1, out);
                out.push_str(&each(e));
            }
            out.push(']');
        }
    }
}

pub fn query(case: &Case) -> String {
    let names = Names { rules: case.rules.len().clamp(1, 5), aliases: case.aliases.len().min(3) };
    let mut out = String::new();
    for (i, alias) in case.aliases.iter().take(names.aliases).enumerate() {
        out.push_str(&format!("ALIAS a{i} = "));
        pat(alias, names, 0, &mut out);
        out.push('\n');
    }
    for (i, rule) in case.rules.iter().take(names.rules).enumerate() {
        out.push_str(&rule_name(i));
        out.push_str(" = ");
        pat(&rule.pattern, names, 0, &mut out);
        if let Some((a, op, b)) = &rule.filter {
            out.push_str(" WHERE ");
            tmpl(a, 3, &mut out);
            out.push_str(&format!(" {} ", pick(OPS, *op)));
            tmpl(b, 3, &mut out);
        }
        if let Some(t) = &rule.template {
            out.push_str(" AS ");
            tmpl(t, 0, &mut out);
        }
        out.push('\n');
    }
    if case.rules.is_empty() {
        out.push_str("TEXT = 0 TO n ANY\n");
    }
    out
}

/// The query text and input a case stands for.
pub fn render(case: &Case) -> (String, String) {
    let input = case.input.iter().take(40).map(|&(t, g)| format!("{}{}", pick(TOKENS, t), pick(GAPS, g))).collect();
    (query(case), input)
}

/// Arbitrary bytes as a query: compiling must never panic, and anything that parses must
/// print to text that parses back to the same tree.
pub fn check_parse(data: &[u8]) {
    let Ok(src) = std::str::from_utf8(data) else { return };
    if let Ok(ast) = txtql::parser::parse(src) {
        let printed = txtql::printer::print_query(&ast);
        let again = txtql::parser::parse(&printed).expect("printed query must parse");
        assert_eq!(printed, txtql::printer::print_query(&again), "printing is not a fixed point");
    }
    let _ = txtql::Query::compile(src);
}

/// Arbitrary bytes as input text: positions map back to the text, and word and number runs
/// are disjoint, maximal and start where they should.
pub fn check_input(data: &[u8]) {
    let text = String::from_utf8_lossy(data);
    let input = txtql::input::Input::new(&text);
    assert_eq!(input.slice(0, input.len()), text);
    let mut covered_until = 0;
    for i in 0..input.len() {
        let c = input.char(i).unwrap();
        assert_eq!(input.slice(i, i + 1).chars().next(), Some(c));
        for end in [input.word_at(i), input.number_at(i)].into_iter().flatten() {
            assert!(i >= covered_until, "runs overlap at {i}");
            assert!(end > i && end <= input.len());
            covered_until = end;
        }
        if let Some(end) = input.word_at(i) {
            assert!(c.is_alphabetic());
            assert!(
                input.char(end).is_none_or(|n| !n.is_alphabetic() && !txtql::input::is_mark(n)),
                "word run not maximal"
            );
        }
        if let Some(end) = input.number_at(i) {
            assert!(c.is_ascii_digit());
            assert!(input.char(end).is_none_or(|n| !n.is_ascii_digit()), "number run not maximal");
        }
        if let Some(end) = input.newline_at(i) {
            assert!(input.is_line_end(i) && input.is_line_start(end));
        }
    }
}

/// A structured grammar and input: compiling and running must never panic, must stay within
/// the step limit, and errors must render. Returns true if the query compiled.
pub fn check_run(case: &Case) -> bool {
    let (src, input) = render(case);
    let (q, checked) = match txtql::Query::compile(&src) {
        Ok(q) => (q, true),
        // Rejected grammars must still be safe to run: the runtime cannot rely on the checker.
        Err(_) => match txtql::Query::compile_unchecked(&src) {
            Some(q) => (q, false),
            None => return false,
        },
    };
    let opts = txtql::Options { max_steps: 200_000, ambiguity_steps: 50_000, ..Default::default() };
    match q.run(&input, &opts) {
        Ok(out) => {
            let json = serde_json::to_string(&out.value).unwrap();
            let back: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(back, out.value);
        }
        Err(e) => {
            for r in e.reports("q", &src, "i", &input) {
                let _ = txtql::render_plain(&r);
            }
        }
    }
    checked
}

fn until_kw(after: bool) -> &'static str {
    if after { "UNTIL" } else { "UNTILBEFORE" }
}
