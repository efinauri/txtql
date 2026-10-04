//! Error and warning types. Every user-facing problem is a `miette::Diagnostic` with a stable code.
//!
//! Errors carry byte spans only; the public API attaches the query or input text as source code.

use miette::{Diagnostic, LabeledSpan, NamedSource, SourceSpan};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    pub fn to(self, other: Span) -> Span {
        Span::new(self.start.min(other.start), self.end.max(other.end))
    }
}

impl From<Span> for SourceSpan {
    fn from(s: Span) -> SourceSpan {
        SourceSpan::new(s.start.into(), s.end - s.start)
    }
}

/// Syntax errors in the query text.
#[derive(Debug, Clone, Error, Diagnostic, PartialEq)]
pub enum ParseError {
    #[error("unexpected character `{ch}`")]
    #[diagnostic(code(txtql::parse::unexpected_char))]
    UnexpectedChar {
        ch: char,
        #[label("not valid here")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("unterminated string")]
    #[diagnostic(code(txtql::parse::unterminated_string), help("close the string with a matching quote"))]
    UnterminatedString {
        #[label("this string never ends")]
        span: Span,
    },

    #[error("unknown escape sequence `\\{ch}`")]
    #[diagnostic(code(txtql::parse::bad_escape), help("valid escapes are \\n, \\t, \\r, \\\\, \\' and \\\""))]
    BadEscape {
        ch: char,
        #[label("unknown escape")]
        span: Span,
    },

    #[error("invalid number")]
    #[diagnostic(code(txtql::parse::bad_number))]
    BadNumber {
        #[label("this number is too large or malformed")]
        span: Span,
    },

    #[error("expected {expected}, found {found}")]
    #[diagnostic(code(txtql::parse::unexpected_token))]
    Unexpected {
        expected: String,
        found: String,
        #[label("unexpected {found}")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("expected {expected}, but the query ends here")]
    #[diagnostic(code(txtql::parse::unexpected_end))]
    UnexpectedEnd {
        expected: String,
        #[label("expected {expected} after this")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("query is nested too deeply")]
    #[diagnostic(code(txtql::parse::too_deep), help("simplify the query or split it into named rules"))]
    TooDeep {
        #[label("nesting limit reached here")]
        span: Span,
    },
}

/// Problems found by static analysis of a parsed query.
#[derive(Debug, Clone, Error, Diagnostic, PartialEq)]
pub enum CheckError {
    #[error("the query has no `TEXT` rule")]
    #[diagnostic(
        code(txtql::check::missing_root),
        help("add a rule named `TEXT`; it is matched against the whole input, e.g. `TEXT = 1 TO n LINE SPLITBY NL`")
    )]
    MissingRoot {
        #[label("the query")]
        span: Span,
    },

    #[error("`{name}` is defined more than once")]
    #[diagnostic(
        code(txtql::check::duplicate_rule),
        help("rules and aliases share one set of names; rename one of them")
    )]
    DuplicateRule {
        name: String,
        #[label("first definition")]
        first: Span,
        #[label("redefined here")]
        second: Span,
    },

    #[error("`TEXT` must be a rule, not an alias")]
    #[diagnostic(code(txtql::check::root_is_alias), help("write `TEXT = pattern` without `ALIAS`"))]
    RootIsAlias {
        #[label("defined as an alias")]
        span: Span,
    },

    #[error("alias `{name}` refers to itself")]
    #[diagnostic(
        code(txtql::check::alias_cycle),
        help("an alias is replaced by its pattern where it is used, so it cannot be recursive; use a rule")
    )]
    AliasCycle {
        name: String,
        #[label("this reference leads back to the alias")]
        span: Span,
    },

    #[error("aliases cannot capture: `{name}` is labelled inside one")]
    #[diagnostic(
        code(txtql::check::label_in_alias),
        help("label the alias where it is used (`x:alias`), or write a rule instead if you need captures inside it")
    )]
    LabelInAlias {
        name: String,
        #[label("label inside an alias")]
        span: Span,
    },

    #[error("`{name}` is a template constant, not a name")]
    #[diagnostic(
        code(txtql::check::reserved_name),
        help(
            "`true`, `false` and `null` (in any case) are constants in templates and conditions, so a rule, alias, \
             label or loop variable called `{name}` could never be referenced; choose another name, e.g. `{name}_value`"
        )
    )]
    ReservedName {
        name: String,
        #[label("{kind} named after a constant")]
        span: Span,
        kind: &'static str,
    },

    #[error("rule `{name}` is not defined")]
    #[diagnostic(code(txtql::check::undefined_rule))]
    UndefinedRule {
        name: String,
        #[label("unknown rule")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("capture `{name}` appears twice in the same match")]
    #[diagnostic(
        code(txtql::check::duplicate_capture),
        help("give one of them a different name with a label, e.g. `other:{name}`")
    )]
    DuplicateCapture {
        name: String,
        #[label("first capture")]
        first: Span,
        #[label("captured again here")]
        second: Span,
    },

    #[error("`{name}` is not captured here")]
    #[diagnostic(code(txtql::check::unknown_capture))]
    UnknownCapture {
        name: String,
        #[label("unknown name")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("`{of}` has no field `{field}`")]
    #[diagnostic(code(txtql::check::unknown_field))]
    UnknownField {
        field: String,
        of: String,
        #[label("unknown field")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("`FOR` needs a repeated value, but `{name}` is {shape}")]
    #[diagnostic(
        code(txtql::check::not_repeated),
        help(
            "lists come from captures inside a repetition of more than one item, e.g. `1 TO n` (`0 TO 1` gives one value or null)"
        )
    )]
    NotRepeated {
        name: String,
        shape: String,
        #[label("not a list")]
        span: Span,
    },

    #[error("only objects can be merged, but `{name}` is {shape}")]
    #[diagnostic(code(txtql::check::not_mergeable))]
    NotMergeable {
        name: String,
        shape: String,
        #[label("not an object")]
        span: Span,
        #[help]
        help: String,
    },

    #[error("this `OR` branch is the same as an earlier one")]
    #[diagnostic(
        code(txtql::check::duplicate_branch),
        help("the earlier branch always wins, so this one can never be chosen; remove it, or change it")
    )]
    DuplicateBranch {
        #[label("first here")]
        first: Span,
        #[label("the same again")]
        second: Span,
    },

    #[error("this repetition can match empty text")]
    #[diagnostic(
        code(txtql::check::empty_loop),
        help("a repeated pattern must consume at least one character, or be split by a separator that does")
    )]
    EmptyLoop {
        #[label("repeated pattern may be empty")]
        span: Span,
    },

    #[error("rule `{name}` can derive itself without consuming any text")]
    #[diagnostic(
        code(txtql::check::empty_cycle),
        help("make sure every recursive reference is preceded or followed by something that consumes text")
    )]
    EmptyCycle {
        name: String,
        #[label("this reference closes the cycle")]
        span: Span,
    },

    #[error("invalid repetition bounds {min} TO {max}")]
    #[diagnostic(code(txtql::check::bad_bounds), help("the lower bound must not exceed the upper bound"))]
    BadBounds {
        min: u32,
        max: u32,
        #[label("here")]
        span: Span,
    },

    #[error("repetition bound {bound} is too large")]
    #[diagnostic(
        code(txtql::check::bound_too_large),
        help("finite bounds are limited to {limit}; use `n` for an unlimited repetition")
    )]
    BoundTooLarge {
        bound: u64,
        limit: u32,
        #[label("here")]
        span: Span,
    },

    #[error("this cannot be the stop of `{keyword}`")]
    #[diagnostic(code(txtql::check::bad_stop))]
    BadStop {
        keyword: &'static str,
        #[label("not allowed in a stop")]
        span: Span,
        #[help]
        help: String,
    },

    #[error("the stop of `{keyword}` can match empty text")]
    #[diagnostic(
        code(txtql::check::empty_stop),
        help("an empty stop would end the repetition at once; a stop must consume text, or be EOF")
    )]
    EmptyStop {
        keyword: &'static str,
        #[label("can be empty")]
        span: Span,
    },

    #[error("empty literal")]
    #[diagnostic(code(txtql::check::empty_literal), help("a literal must contain at least one character"))]
    EmptyLiteral {
        #[label("matches nothing")]
        span: Span,
    },

    #[error("unknown function `{name}`")]
    #[diagnostic(code(txtql::check::unknown_function))]
    UnknownFunction {
        name: String,
        #[label("unknown function")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("`{name}` takes {expected} argument(s), got {got}")]
    #[diagnostic(code(txtql::check::arity))]
    Arity {
        name: String,
        expected: String,
        got: usize,
        #[label("wrong number of arguments")]
        span: Span,
        #[help]
        help: Option<String>,
    },

    #[error("alias `{name}` is never used")]
    #[diagnostic(code(txtql::lint::unused_alias), severity(Warning), help("use it in a rule, or remove it"))]
    UnusedAlias {
        name: String,
        #[label("unused")]
        span: Span,
    },

    #[error("rule `{name}` is never used")]
    #[diagnostic(
        code(txtql::lint::unused_rule),
        severity(Warning),
        help("reference it from `TEXT` (directly or indirectly), or remove it")
    )]
    UnusedRule {
        name: String,
        #[label("unused")]
        span: Span,
    },

    #[error("nested unlimited repetition")]
    #[diagnostic(
        code(txtql::lint::nested_repetition),
        severity(Warning),
        help(
            "`1 TO n (1 TO n x)` can split the text in exponentially many ways; add a separator or a literal to anchor the inner repetition"
        )
    )]
    NestedRepetition {
        #[label("outer repetition")]
        outer: Span,
        #[label("inner repetition")]
        inner: Span,
    },

    #[error("rule `{name}` is right-recursive")]
    #[diagnostic(
        code(txtql::lint::right_recursion),
        severity(Warning),
        help("right recursion makes matching quadratic; a repetition such as `1 TO n x` is linear")
    )]
    RightRecursion {
        name: String,
        #[label("recursive reference at the end of the rule")]
        span: Span,
    },
}

impl CheckError {
    pub fn is_warning(&self) -> bool {
        matches!(self.severity(), Some(miette::Severity::Warning))
    }
}

/// Failures while matching input text.
#[derive(Debug, Clone, Error, Diagnostic, PartialEq)]
pub enum MatchError {
    #[error("the text does not match the query")]
    #[diagnostic(code(txtql::input::no_parse))]
    NoParse {
        #[label("{label}")]
        span: Span,
        label: String,
        /// The first line matched by an `UNTIL` / `UNTILBEFORE` repetition that ran across line breaks up to
        /// the failure.
        #[label("{until_label}")]
        until: Option<Span>,
        until_label: String,
        #[help]
        help: Option<String>,
    },

    #[error("the query is too expensive for this input (more than {limit} steps)")]
    #[diagnostic(
        code(txtql::input::too_expensive),
        help(
            "raise the limit with --max-steps, or make the query less ambiguous (fewer nested repetitions, more literals)"
        )
    )]
    TooExpensive { limit: u64 },

    #[error("the matched text is nested too deeply (more than {limit} levels)")]
    #[diagnostic(code(txtql::input::too_deep), help("deeply recursive rules are limited to bound memory"))]
    TooDeep {
        limit: usize,
        #[label("nesting limit reached here")]
        span: Span,
    },
}

/// A match that could have been read in two ways that produce different output.
#[derive(Debug, Clone, Error, PartialEq)]
#[error("ambiguous match in rule `{rule}`: the text can be read in two ways with different results")]
pub struct Ambiguity {
    pub rule: String,
    pub node_span: Span,
    pub chosen_span: Span,
    pub chosen_desc: String,
    pub alt_span: Span,
    pub alt_desc: String,
    pub chosen_value: String,
    /// The rule's value in the alternative reading; `None` when that reading fails to evaluate.
    pub alt_value: Option<String>,
    /// Why the alternative reading fails to evaluate (an evaluation error, or nesting deeper
    /// than `max_depth`); set exactly when `alt_value` is not.
    pub alt_failure: Option<String>,
    /// Reported as an error rather than a warning (`--strict` or a `STRICT` rule).
    pub strict: bool,
    /// Advice for the kind of ambiguity, replacing the generic advice.
    pub hint: Option<String>,
    /// Label the whole match too. Only for matches over several lines: on one line, the
    /// renderer cannot draw a label around the two others.
    pub label_node: bool,
}

impl Diagnostic for Ambiguity {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new("txtql::input::ambiguous"))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(if self.strict { miette::Severity::Error } else { miette::Severity::Warning })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        let advice = self
            .hint
            .as_deref()
            .unwrap_or("Make the pattern more specific (literals, SPLITBY, LAZY) if the other reading was intended.");
        let chosen = format!("txtql picked the first reading, which gives {}", self.chosen_value);
        // The reason goes on a line of its own: help text is wrapped, and a long reason would
        // otherwise split the sentence before it.
        let other = match (&self.alt_value, &self.alt_failure) {
            (Some(value), _) => format!("{chosen}; the other gives {value}."),
            (None, Some(why)) => {
                format!("{chosen}.\nHowever, another reading of this text exists but fails to evaluate:\n{why}.")
            }
            (None, None) => format!("{chosen}; the other gives no value."),
        };
        Some(Box::new(format!("{other}\n{advice}")))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let mut labels = vec![
            LabeledSpan::new_with_span(Some(format!("chosen: {}", self.chosen_desc)), self.chosen_span),
            LabeledSpan::new_with_span(Some(format!("alternative: {}", self.alt_desc)), self.alt_span),
        ];
        if self.label_node && self.node_span != self.chosen_span.to(self.alt_span) {
            labels.push(LabeledSpan::new_with_span(Some(format!("matched by `{}`", self.rule)), self.node_span));
        }
        Some(Box::new(labels.into_iter()))
    }
}

/// A template or WHERE clause failed while building the output.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct EvalError {
    pub message: String,
    /// Location in the query.
    pub query_span: Span,
    /// The matched input text being evaluated.
    pub input_span: Span,
    pub help: Option<String>,
    /// Filled in by the public API so the related diagnostic can show the input.
    pub input: Option<Box<InputContext>>,
}

impl EvalError {
    pub fn new(message: impl Into<String>, query_span: Span, input_span: Span) -> EvalError {
        EvalError { message: message.into(), query_span, input_span, help: None, input: None }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> EvalError {
        self.help = Some(help.into());
        self
    }

    pub fn attach_input(&mut self, source: NamedSource<Arc<str>>) {
        self.input = Some(Box::new(InputContext { src: source, span: self.input_span }));
    }
}

impl Diagnostic for EvalError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new("txtql::eval::error"))
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.help.as_ref().map(|h| Box::new(h) as Box<dyn fmt::Display>)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let label = LabeledSpan::new_with_span(Some("while evaluating this".into()), self.query_span);
        Some(Box::new(std::iter::once(label)))
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        let ctx = self.input.as_deref()?;
        Some(Box::new(std::iter::once(ctx as &dyn Diagnostic)))
    }
}

/// An object entry set a key that was already set, so an earlier value was lost. A warning, or
/// an error in strict runs.
#[derive(Debug, Clone, Error)]
#[error("key `{key}` is set more than once; the later value wins")]
pub struct RepeatedKey {
    pub key: String,
    /// The entry that set the key again.
    pub query_span: Span,
    /// The match whose value was being built.
    pub input_span: Span,
    pub strict: bool,
    /// Filled in by the public API so the related diagnostic can show the input.
    pub input: Option<Box<InputContext>>,
}

impl RepeatedKey {
    pub fn new(key: String, query_span: Span, input_span: Span) -> RepeatedKey {
        RepeatedKey { key, query_span, input_span, strict: false, input: None }
    }

    pub fn attach_input(&mut self, source: NamedSource<Arc<str>>) {
        self.input = Some(Box::new(InputContext { src: source, span: self.input_span }));
    }
}

impl Diagnostic for RepeatedKey {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new("txtql::eval::repeated_key"))
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(if self.strict { miette::Severity::Error } else { miette::Severity::Warning })
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new("to keep every value, collect them with `LISTOF`, e.g. `{ k: LISTOF v FOR x IN xs }`"))
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        let label = LabeledSpan::new_with_span(Some("this sets the key again".into()), self.query_span);
        Some(Box::new(std::iter::once(label)))
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn Diagnostic> + 'a>> {
        let ctx = self.input.as_deref()?;
        Some(Box::new(std::iter::once(ctx as &dyn Diagnostic)))
    }
}

/// Related diagnostic pointing into the input text.
#[derive(Debug, Clone, Error, Diagnostic)]
#[error("in this part of the input")]
#[diagnostic(severity(Advice))]
pub struct InputContext {
    #[source_code]
    pub src: NamedSource<Arc<str>>,
    #[label("matched text")]
    pub span: Span,
}
