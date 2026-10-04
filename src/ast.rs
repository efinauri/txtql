//! Syntax tree of a query.

use crate::error::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub aliases: Vec<Alias>,
    pub rules: Vec<Rule>,
    pub span: Span,
}

/// `ALIAS name = pattern`: a named pattern fragment. It is substituted where it is used and
/// never captures anything; label it at the use site to capture its text.
#[derive(Debug, Clone, PartialEq)]
pub struct Alias {
    pub name: Ident,
    pub pattern: Pattern,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub name: Ident,
    pub strict: bool,
    pub pattern: Pattern,
    pub filter: Option<Cond>,
    pub template: Option<Template>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Prim {
    Word,
    /// A whole run of digits, with an optional decimal part (`42`, `3.5`).
    Float,
    /// A whole run of digits.
    Int,
    /// A run of hexadecimal digits.
    Hex,
    /// A run of binary digits.
    Bin,
    /// A dotted IPv4 address.
    Ipv4,
    /// An IPv6 address.
    Ipv6,
    Punct,
    Any,
    /// One digit, even inside a number.
    Digit,
    /// One letter, even inside a word.
    Letter,
    /// A whole line, without its line break.
    Line,
    /// A line break: `\n`, `\r\n` or `\r`.
    Newline,
    /// A tab character.
    Tab,
    /// Empty text; its value is the line number (from 1).
    Row,
    /// Empty text; its value is the column (from 1, in characters).
    Col,
    /// Empty text at the end of the input.
    Eof,
}

impl Prim {
    pub fn as_str(self) -> &'static str {
        match self {
            Prim::Word => "WORD",
            Prim::Float => "FLOAT",
            Prim::Int => "INT",
            Prim::Hex => "HEX",
            Prim::Bin => "BIN",
            Prim::Ipv4 => "IPV4",
            Prim::Ipv6 => "IPV6",
            Prim::Punct => "PUNCT",
            Prim::Any => "ANY",
            Prim::Digit => "DIGIT",
            Prim::Letter => "LETTER",
            Prim::Line => "LINE",
            Prim::Newline => "NL",
            Prim::Tab => "TAB",
            Prim::Row => "ROW",
            Prim::Col => "COL",
            Prim::Eof => "EOF",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pattern {
    pub kind: PatKind,
    pub span: Span,
}

/// Where an `UNTIL` / `UNTILBEFORE` repetition ends relative to its stop. The stop is never
/// part of a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    /// `UNTILBEFORE x`: end right before `x` and leave it for what follows.
    Before,
    /// `UNTIL x`: consume `x` as well.
    After,
}

impl Stop {
    pub fn keyword(self) -> &'static str {
        match self {
            Stop::Before => "UNTILBEFORE",
            Stop::After => "UNTIL",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PatKind {
    Lit { text: String, ci: bool },
    Prim(Prim),
    Ref(Ident),
    Label(Ident, Box<Pattern>),
    Seq(Vec<Pattern>),
    Or(Vec<Pattern>),
    Repeat(Box<Repeat>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Repeat {
    pub min: u32,
    /// `None` means unbounded (`n`).
    pub max: Option<u32>,
    pub lazy: bool,
    pub item: Pattern,
    pub sep: Option<Pattern>,
    pub skip: Option<Pattern>,
    /// `UNTILBEFORE x` / `UNTIL x`: the repetition ends at the first `x` found where an
    /// iteration would begin, and `x` must be there.
    pub until: Option<Until>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Until {
    pub stop: Pattern,
    pub kind: Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub kind: TmplKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TmplKind {
    Str(String),
    Num(serde_json::Number),
    Bool(bool),
    Null,
    /// `a.b.c`
    Path(Vec<Ident>),
    Call(Ident, Vec<Template>),
    Object(Vec<ObjEntry>),
    Array(Vec<ArrElem>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjEntry {
    Pair {
        key: Template,
        value: Template,
        each: Option<Box<ForEach>>,
        /// `key: LISTOF value`: every value for this key is collected into a list.
        listof: bool,
    },
    /// A value without a key: merge the object(s) in it into this object.
    Merge(Template),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ArrElem {
    pub value: Template,
    pub each: Option<Box<ForEach>>,
}

/// `FOR var` or `FOR var IN source`.
#[derive(Debug, Clone, PartialEq)]
pub struct ForEach {
    pub var: Ident,
    /// Defaults to the capture named `var`.
    pub source: Option<Template>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cond {
    pub kind: CondKind,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    StartsWith,
    EndsWith,
}

impl CmpOp {
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
            CmpOp::Contains => "CONTAINS",
            CmpOp::StartsWith => "STARTSWITH",
            CmpOp::EndsWith => "ENDSWITH",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CondKind {
    Or(Box<Cond>, Box<Cond>),
    And(Box<Cond>, Box<Cond>),
    Not(Box<Cond>),
    Cmp(Template, CmpOp, Template),
    /// A bare value: true when it is not null, false, "" or empty.
    Truthy(Template),
}

/// The root rule is named `TEXT`, in any case. The name is not reserved otherwise.
pub fn is_root_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("TEXT")
}

impl Query {
    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|r| r.name.name == name)
    }

    pub fn root(&self) -> Option<&Rule> {
        self.rules.iter().find(|r| is_root_name(&r.name.name))
    }

    pub fn alias(&self, name: &str) -> Option<&Alias> {
        self.aliases.iter().find(|a| a.name.name == name)
    }
}
