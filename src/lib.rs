//! txtql: extract structured JSON from free text with readable grammar rules.
//!
//! ```
//! let query = txtql::Query::compile(
//!     "color  = WORD
//!      colors = 1 TO n color SPLITBY (' and ' OR ', ')
//!      rhyme  = noun:WORD ' are ' colors NL AS { noun: colors }
//!      TEXT   = 1 TO n rhyme               AS { rhyme }",
//! ).unwrap();
//! let out = query.run("roses are red\nbees are black and yellow\n", &Default::default()).unwrap();
//! assert_eq!(out.value, serde_json::json!({"roses": ["red"], "bees": ["black", "yellow"]}));
//! ```

pub mod analysis;
pub mod ast;
pub mod check;
pub mod earley;
pub mod error;
pub mod eval;
pub mod extract;
pub mod input;
pub mod ir;
pub mod lexer;
pub mod lsp;
pub mod parser;
pub mod printer;
pub mod util;

use error::{Ambiguity, CheckError, EvalError, MatchError, ParseError, RepeatedKey, Span};
pub use eval::drop_deep;
use miette::{NamedSource, Report};
use serde_json::Value;
use std::sync::Arc;

/// Limits and switches for running a query.
#[derive(Debug, Clone)]
pub struct Options {
    /// Maximum work (parser items plus search steps) before giving up.
    pub max_steps: u64,
    /// Maximum nesting of rule matches inside each other. Deep nesting is safe for the stack
    /// (recursion moves to heap-allocated stack segments when needed); this only bounds memory.
    pub max_depth: usize,
    /// Treat every output-changing ambiguity and every repeated key as an error.
    pub strict: bool,
    /// Look for output-changing ambiguity at all.
    pub check_ambiguity: bool,
    /// Work allowed for the ambiguity analysis; it stops quietly (and says so) when exceeded.
    pub ambiguity_steps: u64,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            max_steps: 200_000_000,
            max_depth: 10_000,
            strict: false,
            check_ambiguity: true,
            ambiguity_steps: 5_000_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Query {
    pub ast: ast::Query,
    pub grammar: ir::Grammar,
    /// Lints that did not prevent compilation.
    pub warnings: Vec<CheckError>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CompileError {
    Parse(ParseError),
    /// All diagnostics (errors and warnings) when at least one is an error.
    Check(Vec<CheckError>),
}

#[derive(Debug)]
pub struct Output {
    /// The extracted JSON. Values from deeply recursive grammars can be nested very deeply;
    /// serde_json drops and serializes them recursively, so use [`drop_deep`] to free such a
    /// value on a small stack.
    pub value: Value,
    /// Output-changing ambiguities that were resolved by the default preferences.
    pub ambiguities: Vec<Ambiguity>,
    /// Object entries that set a key a second time, losing the earlier value.
    pub repeated_keys: Vec<RepeatedKey>,
    /// False if the ambiguity analysis stopped early (budget exhausted).
    pub ambiguity_check_complete: bool,
    /// Work done (parser items plus search steps). Deterministic, so useful for tests.
    pub steps: u64,
}

#[derive(Debug)]
pub enum RunError {
    Match(MatchError),
    Eval(EvalError),
    /// Ambiguities reported as errors (`strict`, or rules marked `STRICT`).
    Ambiguous(Vec<Ambiguity>),
    /// Repeated object keys, reported as errors in strict runs.
    RepeatedKeys(Vec<RepeatedKey>),
}

impl Query {
    pub fn compile(src: &str) -> Result<Query, CompileError> {
        let ast = parser::parse(src).map_err(CompileError::Parse)?;
        let diags = check::check(&ast, src);
        if diags.iter().any(|d| !d.is_warning()) {
            return Err(CompileError::Check(diags));
        }
        let grammar = ir::compile(&ast);
        Ok(Query { ast, grammar, warnings: diags })
    }

    /// Compiles without the static checks, for testing that the runtime stays safe (bounded
    /// time and memory, no panics) even on grammars the checker would reject. Returns `None`
    /// for queries the compiler cannot handle at all (unresolved names, oversized bounds).
    #[doc(hidden)]
    pub fn compile_unchecked(src: &str) -> Option<Query> {
        use error::CheckError as E;
        let ast = parser::parse(src).ok()?;
        let diags = check::check(&ast, src);
        let blocking = diags.iter().any(|d| {
            matches!(
                d,
                E::MissingRoot { .. }
                    | E::RootIsAlias { .. }
                    | E::AliasCycle { .. }
                    | E::DuplicateRule { .. }
                    | E::UndefinedRule { .. }
                    | E::BadStop { .. }
                    | E::BoundTooLarge { .. }
            )
        });
        if blocking {
            return None;
        }
        let grammar = ir::compile(&ast);
        Some(Query { ast, grammar, warnings: Vec::new() })
    }

    pub fn run(&self, text: &str, opts: &Options) -> Result<Output, RunError> {
        let g = &self.grammar;
        let input = input::Input::new(text);
        let n = input.len() as u32;
        let mut budget = util::Budget::new(opts.max_steps);
        let chart = earley::recognize(g, &input, &mut budget).map_err(RunError::Match)?;
        if !chart.accepted {
            return Err(RunError::Match(no_parse(g, &input, &chart)));
        }
        let mut ex = extract::Extractor::new(g, &input, &chart, &mut budget, opts.max_depth);
        let lift = |e: extract::Error| match e {
            extract::Error::Match(m) => RunError::Match(m),
            extract::Error::Eval(e) => RunError::Eval(e),
        };
        let Some(root) = ex.best(g.root, 0, n).map_err(lift)? else {
            return Err(RunError::Match(MatchError::NoParse {
                until: None,
                until_label: String::new(),
                span: Span::new(0, text.len()),
                label: "the text matches the patterns, but no reading satisfies the WHERE conditions".into(),
                help: Some("check the WHERE clauses; a rule whose WHERE is false does not match".into()),
            }));
        };
        let (value, mut repeated_keys) = {
            let ctx = eval::Ctx::new(g, &input, &ex.nodes);
            let value = eval::rule_value(&ctx, &eval::Overlay::default(), root).map_err(RunError::Eval)?;
            (value, ctx.repeated_keys.take())
        };
        if opts.strict && !repeated_keys.is_empty() {
            for r in &mut repeated_keys {
                r.strict = true;
            }
            return Err(RunError::RepeatedKeys(repeated_keys));
        }
        let (mut ambiguities, complete) = if opts.check_ambiguity {
            let report = ex.analyze(root, opts.ambiguity_steps).map_err(lift)?;
            (report.found, report.complete)
        } else {
            (Vec::new(), false)
        };
        if opts.strict {
            for a in &mut ambiguities {
                a.strict = true;
            }
        }
        if ambiguities.iter().any(|a| a.strict) {
            return Err(RunError::Ambiguous(ambiguities.into_iter().filter(|a| a.strict).collect()));
        }
        let steps = ex.steps();
        Ok(Output { value, ambiguities, repeated_keys, ambiguity_check_complete: complete, steps })
    }
}

fn no_parse(g: &ir::Grammar, input: &input::Input, chart: &earley::Chart) -> MatchError {
    let at = chart.furthest;
    // "anything but x" (from UNTILBEFORE / UNTIL) says little next to concrete alternatives; keep it only if
    // there is nothing else.
    let is_negation = |sym: &ir::Sym| match sym {
        ir::Sym::Term(t) => matches!(g.terms[*t as usize], ir::Term::Not(_)),
        ir::Sym::Assert(a) => matches!(a, ir::Assert::NotAt(_)),
        ir::Sym::Nt(_) => false,
    };
    let concrete: Vec<_> = chart.expected.iter().copied().filter(|(s, _)| !is_negation(s)).collect();
    let syms = if concrete.is_empty() { chart.expected.clone() } else { concrete };
    // Each description with the rules it would begin; `None` once it is also expected elsewhere,
    // e.g. to continue a rule already under way.
    let mut grouped: Vec<(String, Option<Vec<&str>>)> = Vec::new();
    for (sym, state) in syms {
        let d = g.describe_sym(sym);
        let rule = state.and_then(|s| g.rule_started_at(s)).map(|r| g.rules[r].name.as_str());
        let entry = match grouped.iter().position(|(e, _)| *e == d) {
            Some(k) => &mut grouped[k],
            None => {
                grouped.push((d, Some(Vec::new())));
                grouped.last_mut().unwrap()
            }
        };
        match (rule, &mut entry.1) {
            (Some(r), Some(rules)) if !rules.contains(&r) => rules.push(r),
            (Some(_), _) => {}
            (None, rules) => *rules = None,
        }
    }
    let mut expected: Vec<String> = grouped
        .into_iter()
        .map(|(d, rules)| match rules {
            Some(rules) if !rules.is_empty() => {
                let names: Vec<String> = rules.iter().map(|r| format!("`{r}`")).collect();
                format!("{d} (the start of {})", names.join(" or "))
            }
            _ => d,
        })
        .collect();
    if at < input.len() && chart.has(g.root, 0, at) {
        expected.push("the end of the text".into());
    }
    // LINE only matches at the start of a line; elsewhere, the rest of the line is what was meant.
    let line_hint = chart.expected.iter().any(|(s, _)| *s == ir::Sym::Assert(ir::Assert::LineStart)).then(|| {
        "LINE matches whole lines only. For the rest of this line, write ANY UNTILBEFORE NL \
         (e.g. ALIAS rest = ANY UNTILBEFORE NL)"
            .to_string()
    });
    if line_hint.is_some() {
        for e in &mut expected {
            if e == "the start of a line" {
                e.push_str(" (for `LINE`)");
            }
        }
    }
    const MAX_LISTED: usize = 8;
    let list = match expected.len() {
        0 => "more text".to_string(),
        1 => expected[0].clone(),
        k if k > MAX_LISTED => {
            format!("{}, or one of {} other things", expected[..MAX_LISTED].join(", "), k - MAX_LISTED)
        }
        _ => format!("{} or {}", expected[..expected.len() - 1].join(", "), expected[expected.len() - 1]),
    };

    // An UNTILBEFORE / UNTIL that started on an earlier line and ran up to here: the text is probably
    // malformed where it started, not where the match finally failed.
    // Lines are counted as NL, LINE and ROW do: `\r\n` once, `\n` and a lone `\r` each.
    let line_of = |pos: usize| input.row_col(pos).0;
    // Label only the first line of its match: a span over many lines would print every
    // one of them, while two separate labels let the lines between them be skipped.
    // Line breaks right before the failure do not count: the repetition only crossed lines if
    // it went on after one.
    let crossed = |start: usize| {
        let mut last = at;
        while last > start && matches!(input.char(last - 1), Some('\n' | '\r')) {
            last -= 1;
        }
        line_of(start) < line_of(last)
    };
    let until = chart.open_until.filter(|&(start, _)| crossed(start)).map(|(start, stop)| {
        let mut end = start;
        while end < at && !input.is_line_end(end) {
            end += 1;
        }
        let (a, b) = input.byte_span(start, end.max(start + 1).min(input.len()));
        (Span::new(a, b), line_of(start), stop.keyword())
    });
    let until_label = until
        .map(|(_, _, kw)| format!("{kw} started here and matched across line breaks up to the error below"))
        .unwrap_or_default();
    let until_help = |line: usize, kw: &str| {
        format!("{kw} started on line {line} and matched across line breaks, so the problem is probably on line {line}")
    };

    let found = input.describe_at(at);
    if at < input.len() {
        let end = input.word_at(at).or_else(|| input.number_at(at)).or_else(|| input.newline_at(at)).unwrap_or(at + 1);
        let (a, b) = input.byte_span(at, end);
        let help = match until {
            Some((_, line, kw)) => until_help(line, kw),
            None if line_hint.is_some() => line_hint.clone().unwrap(),
            None if at == 0 => format!("the text does not match from the start; found {found}"),
            None => format!("the text matched up to here; found {found}"),
        };
        MatchError::NoParse {
            span: Span::new(a, b),
            label: format!("expected {list}"),
            until: until.map(|u| u.0),
            until_label: until_label.clone(),
            help: Some(help),
        }
    } else if at > 0 {
        // The text ended too early. Point at the last character: an empty span at the end
        // renders badly.
        let (a, b) = input.byte_span(at - 1, at);
        let help = match until {
            Some((_, line, kw)) => until_help(line, kw),
            None => "the text ended before the query was complete".into(),
        };
        MatchError::NoParse {
            span: Span::new(a, b),
            label: format!("expected {list} after this"),
            until: until.map(|u| u.0),
            until_label: until_label.clone(),
            help: Some(help),
        }
    } else {
        MatchError::NoParse {
            span: Span::new(0, 0),
            label: format!("expected {list}"),
            until: None,
            until_label,
            help: Some("the text is empty".into()),
        }
    }
}

// ---- rendering ----

fn named(name: &str, src: &str) -> NamedSource<Arc<str>> {
    NamedSource::new(name, Arc::from(src)).with_language("txtql")
}

impl CompileError {
    pub fn reports(&self, query_name: &str, query: &str) -> Vec<Report> {
        let src = named(query_name, query);
        match self {
            CompileError::Parse(e) => vec![Report::new(e.clone()).with_source_code(src)],
            CompileError::Check(errs) => {
                errs.iter().map(|e| Report::new(e.clone()).with_source_code(src.clone())).collect()
            }
        }
    }
}

pub fn warning_reports(warnings: &[CheckError], query_name: &str, query: &str) -> Vec<Report> {
    let src = named(query_name, query);
    warnings.iter().map(|e| Report::new(e.clone()).with_source_code(src.clone())).collect()
}

pub fn ambiguity_reports(ambs: &[Ambiguity], input_name: &str, input: &str) -> Vec<Report> {
    let src = named(input_name, input);
    ambs.iter().map(|a| Report::new(a.clone()).with_source_code(src.clone())).collect()
}

pub fn repeated_key_reports(
    keys: &[RepeatedKey],
    query_name: &str,
    query: &str,
    input_name: &str,
    input: &str,
) -> Vec<Report> {
    keys.iter()
        .map(|k| {
            let mut k = k.clone();
            k.attach_input(named(input_name, input));
            Report::new(k).with_source_code(named(query_name, query))
        })
        .collect()
}

impl RunError {
    pub fn reports(&self, query_name: &str, query: &str, input_name: &str, input: &str) -> Vec<Report> {
        match self {
            RunError::Match(e) => vec![Report::new(e.clone()).with_source_code(named(input_name, input))],
            RunError::Eval(e) => {
                let mut e = e.clone();
                e.attach_input(named(input_name, input));
                vec![Report::new(e).with_source_code(named(query_name, query))]
            }
            RunError::Ambiguous(ambs) => ambiguity_reports(ambs, input_name, input),
            RunError::RepeatedKeys(keys) => repeated_key_reports(keys, query_name, query, input_name, input),
        }
    }
}

/// Renders a report without colours, for tests and plain-text output.
pub fn render_plain(report: &Report) -> String {
    let mut out = String::new();
    miette::GraphicalReportHandler::new_themed(miette::GraphicalTheme::unicode_nocolor())
        .with_width(100)
        .render_report(&mut out, report.as_ref())
        .expect("rendering to a String cannot fail");
    out
}
