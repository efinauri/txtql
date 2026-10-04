//! Editor support: what each token of a query is, where names are defined and used, hover
//! text, completions and diagnostics.
//!
//! Queries in an editor are usually unfinished, and the parser stops at the first error. So
//! this works on the fault-tolerant token stream instead of the AST: it finds definitions
//! (`name =`, `ALIAS name =`, `STRICT name =`), tracks which section of a definition each token
//! is in (pattern, `WHERE` or `AS`), and resolves names within the definition. When the query
//! parses, the AST adds the full extent of labelled patterns for hover.

use crate::ast::{self, PatKind, Pattern};
use crate::check::FUNCTIONS;
use crate::error::Span;
use crate::lexer::{Kw, Tok, Token, lex_tolerant};
use crate::{CompileError, Query};
use miette::Diagnostic;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Comment,
    String,
    Number,
    /// Structural keywords: `OR`, `TO`, `AS`, `FOR`, `n` in `1 TO n`, …
    Keyword,
    /// `WORD`, `INT`, `ANY`, `NL`, …
    Primitive,
    Rule,
    Alias,
    /// `name` in `name:pattern`.
    Label,
    /// A capture used in a template or condition.
    Capture,
    /// `x` in `FOR x IN …`.
    Variable,
    Function,
    /// `b` in `a.b`.
    Field,
    /// `true`, `false`, `null`.
    Constant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemToken {
    pub span: Span,
    pub kind: TokenKind,
    /// The token defines its name (a rule or alias name before `=`, a label, a `FOR … IN` variable).
    pub definition: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefKind {
    Rule,
    Alias,
    Label,
    Variable,
}

/// Something a name can refer to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Def {
    pub name: String,
    pub kind: DefKind,
    /// The name itself.
    pub span: Span,
    /// The definition (rule or alias) it belongs to, by index into `Analysis::blocks`.
    pub block: usize,
}

/// One rule or alias definition, from its first token to the last token before the next one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub kind: DefKind,
    pub strict: bool,
    pub span: Span,
    /// Index into `Analysis::defs` of the name.
    pub def: usize,
    /// `--` comment lines right above the definition, without the dashes.
    pub doc: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diag {
    pub span: Span,
    pub severity: Severity,
    pub code: Option<String>,
    pub message: String,
    /// Other labelled places, with their label.
    pub related: Vec<(Span, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Keyword,
    Primitive,
    Rule,
    Alias,
    Capture,
    Function,
    Constant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    /// Before the first definition.
    Outside,
    Pattern,
    Where,
    Template,
}

#[derive(Debug, Clone)]
pub struct Analysis {
    pub src: String,
    /// Sorted by position.
    pub tokens: Vec<SemToken>,
    pub defs: Vec<Def>,
    pub blocks: Vec<Block>,
    /// For names that refer to something: the token's span and the index of its `Def`.
    pub uses: Vec<(Span, usize)>,
    pub diagnostics: Vec<Diag>,
    /// Full span of each labelled pattern, by the start of its label (only when the query parses).
    label_extent: HashMap<usize, Span>,
    /// Section of each lexical token, and the block it belongs to.
    sections: Vec<(Span, Section, Option<usize>)>,
}

const PATTERN_KEYWORDS: &[Kw] =
    &[Kw::OR, Kw::TO, Kw::LAZY, Kw::SPLITBY, Kw::SKIPPING, Kw::UNTILBEFORE, Kw::UNTIL, Kw::WHERE, Kw::AS];
const CONDITION_KEYWORDS: &[Kw] = &[Kw::AND, Kw::OR, Kw::NOT, Kw::CONTAINS, Kw::STARTSWITH, Kw::ENDSWITH, Kw::AS];
const TEMPLATE_KEYWORDS: &[Kw] = &[Kw::FOR, Kw::IN, Kw::LISTOF];
const PRIMITIVES: &[Kw] = &[
    Kw::WORD,
    Kw::INT,
    Kw::FLOAT,
    Kw::HEX,
    Kw::BIN,
    Kw::IPV4,
    Kw::IPV6,
    Kw::LETTER,
    Kw::DIGIT,
    Kw::PUNCT,
    Kw::ANY,
    Kw::LINE,
    Kw::NL,
    Kw::TAB,
    Kw::ROW,
    Kw::COL,
    Kw::EOF,
];
const CONSTANTS: &[&str] = &["true", "false", "null"];

fn is_primitive(kw: Kw) -> bool {
    PRIMITIVES.contains(&kw)
}

impl Analysis {
    pub fn new(src: &str) -> Analysis {
        let lexed = lex_tolerant(src);
        let toks: Vec<&Token> = lexed.tokens.iter().filter(|t| t.tok != Tok::Eof).collect();
        let mut a = Analysis {
            src: src.to_string(),
            tokens: Vec::new(),
            defs: Vec::new(),
            blocks: Vec::new(),
            uses: Vec::new(),
            diagnostics: diagnostics(src),
            label_extent: HashMap::new(),
            sections: Vec::new(),
        };
        a.scan(&toks, &lexed.comments);
        if let Ok(q) = crate::parser::parse(src) {
            a.label_extent = label_extents(&q);
        }
        for &span in &lexed.comments {
            a.tokens.push(SemToken { span, kind: TokenKind::Comment, definition: false });
        }
        a.tokens.sort_by_key(|t| t.span.start);
        a
    }

    /// Finds the definitions, classifies every token and resolves names.
    fn scan(&mut self, toks: &[&Token], comments: &[Span]) {
        let ident = |i: usize| match toks.get(i).map(|t| &t.tok) {
            Some(Tok::Ident(s)) => Some(s.as_str()),
            _ => None,
        };
        let is = |i: usize, tok: &Tok| toks.get(i).is_some_and(|t| &t.tok == tok);

        // Pass 1: definition heads, so that names can be used before they are defined.
        // `starts[k]` is the token index where block `k` begins and `names[k]` its name token.
        let mut heads: Vec<(usize, usize, DefKind, bool)> = Vec::new();
        let mut section = Section::Outside;
        for i in 0..toks.len() {
            match &toks[i].tok {
                Tok::Kw(Kw::WHERE) if section != Section::Outside => section = Section::Where,
                Tok::Kw(Kw::AS) if section != Section::Outside => section = Section::Template,
                Tok::Ident(_) if is(i + 1, &Tok::Eq) => {
                    // In a condition, `name = …` right after WHERE / AND / OR / NOT / `(` compares.
                    let prev = i.checked_sub(1).map(|p| &toks[p].tok);
                    let compares = section == Section::Where
                        && matches!(prev, Some(Tok::Kw(Kw::WHERE | Kw::AND | Kw::OR | Kw::NOT)) | Some(Tok::LParen));
                    if compares {
                        continue;
                    }
                    let (start, kind, strict) = match prev {
                        Some(Tok::Kw(Kw::ALIAS)) => (i - 1, DefKind::Alias, false),
                        Some(Tok::Kw(Kw::STRICT)) => (i - 1, DefKind::Rule, true),
                        _ => (i, DefKind::Rule, false),
                    };
                    heads.push((start, i, kind, strict));
                    section = Section::Pattern;
                }
                _ => {}
            }
        }

        // Blocks and their names.
        for (k, &(start, name_at, kind, strict)) in heads.iter().enumerate() {
            let end_tok = heads.get(k + 1).map_or(toks.len(), |h| h.0);
            let end = toks[end_tok - 1].span.end;
            let name = ident(name_at).expect("heads are identifiers").to_string();
            let def = self.defs.len();
            self.defs.push(Def { name: name.clone(), kind, span: toks[name_at].span, block: k });
            let doc = doc_comments(&self.src, comments, toks[start].span.start);
            self.blocks.push(Block { name, kind, strict, span: Span::new(toks[start].span.start, end), def, doc });
        }
        let top: HashMap<String, usize> = {
            let mut m = HashMap::new();
            for b in &self.blocks {
                m.entry(b.name.clone()).or_insert(b.def);
            }
            m
        };

        // Pass 2: classify tokens, block by block.
        let mut block: Option<usize> = None;
        let mut section = Section::Outside;
        let mut next_head = 0;
        // Uses of captures, resolved at the end of each block when all its labels are known.
        let mut pending: Vec<(Span, String)> = Vec::new();
        let mut for_vars: Vec<usize> = Vec::new();
        let mut labels: Vec<usize> = Vec::new();
        let mut rule_refs: Vec<(String, usize)> = Vec::new();
        let finish = |a: &mut Analysis,
                      pending: &mut Vec<(Span, String)>,
                      labels: &mut Vec<usize>,
                      for_vars: &mut Vec<usize>,
                      rule_refs: &mut Vec<(String, usize)>| {
            for (span, name) in pending.drain(..) {
                let target = for_vars
                    .iter()
                    .chain(labels.iter())
                    .copied()
                    .find(|&d| a.defs[d].name == name)
                    .or_else(|| rule_refs.iter().find(|(n, _)| *n == name).map(|&(_, d)| d));
                if let Some(d) = target {
                    a.uses.push((span, d));
                }
            }
            labels.clear();
            for_vars.clear();
            rule_refs.clear();
        };

        for i in 0..toks.len() {
            let t = toks[i];
            if heads.get(next_head).is_some_and(|h| h.0 == i) {
                finish(self, &mut pending, &mut labels, &mut for_vars, &mut rule_refs);
                block = Some(next_head);
                section = Section::Pattern;
                next_head += 1;
            }
            self.sections.push((t.span, section, block));
            let push = |a: &mut Analysis, kind, definition| {
                a.tokens.push(SemToken { span: t.span, kind, definition });
            };
            match &t.tok {
                Tok::Str { .. } => push(self, TokenKind::String, false),
                Tok::Int(_) | Tok::Float(_) => push(self, TokenKind::Number, false),
                Tok::Kw(kw) => {
                    push(self, if is_primitive(*kw) { TokenKind::Primitive } else { TokenKind::Keyword }, false);
                    match kw {
                        Kw::WHERE if section != Section::Outside => section = Section::Where,
                        Kw::AS if section != Section::Outside => section = Section::Template,
                        _ => {}
                    }
                }
                Tok::Ident(name) => {
                    if let Some(b) = block
                        && heads[b].1 == i
                    {
                        let kind = if heads[b].2 == DefKind::Alias { TokenKind::Alias } else { TokenKind::Rule };
                        push(self, kind, true);
                        self.uses.push((t.span, self.blocks[b].def));
                        continue;
                    }
                    let prev = i.checked_sub(1).map(|p| &toks[p].tok);
                    match section {
                        Section::Outside => {}
                        Section::Pattern => {
                            if name.eq_ignore_ascii_case("n") && matches!(prev, Some(Tok::Kw(Kw::TO))) {
                                push(self, TokenKind::Keyword, false);
                            } else if is(i + 1, &Tok::Colon) {
                                push(self, TokenKind::Label, true);
                                let d = self.defs.len();
                                self.defs.push(Def {
                                    name: name.clone(),
                                    kind: DefKind::Label,
                                    span: t.span,
                                    block: block.expect("patterns are inside a block"),
                                });
                                labels.push(d);
                                self.uses.push((t.span, d));
                            } else {
                                match top.get(name) {
                                    Some(&d) if self.defs[d].kind == DefKind::Alias => {
                                        push(self, TokenKind::Alias, false);
                                        self.uses.push((t.span, d));
                                    }
                                    Some(&d) => {
                                        push(self, TokenKind::Rule, false);
                                        self.uses.push((t.span, d));
                                        rule_refs.push((name.clone(), d));
                                    }
                                    None => push(self, TokenKind::Rule, false),
                                }
                            }
                        }
                        Section::Where | Section::Template => {
                            if is(i + 1, &Tok::LParen) {
                                push(self, TokenKind::Function, false);
                            } else if matches!(prev, Some(Tok::Dot)) {
                                push(self, TokenKind::Field, false);
                            } else if section == Section::Template && CONSTANTS.contains(&name.as_str()) {
                                push(self, TokenKind::Constant, false);
                            } else if matches!(prev, Some(Tok::Kw(Kw::FOR))) && is(i + 1, &Tok::Kw(Kw::IN)) {
                                push(self, TokenKind::Variable, true);
                                let d = self.defs.len();
                                self.defs.push(Def {
                                    name: name.clone(),
                                    kind: DefKind::Variable,
                                    span: t.span,
                                    block: block.expect("templates are inside a block"),
                                });
                                for_vars.push(d);
                                self.uses.push((t.span, d));
                            } else {
                                push(self, TokenKind::Capture, false);
                                pending.push((t.span, name.clone()));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        finish(self, &mut pending, &mut labels, &mut for_vars, &mut rule_refs);
        // A `FOR x` without `IN` iterates the capture `x`, and inside the loop `x` is the item:
        // a template variable that shadows nothing new, so it resolves like a capture above.
        self.uses.sort_by_key(|(s, _)| s.start);
    }

    /// The `Def` of the name at `offset`, if any.
    pub fn def_at(&self, offset: usize) -> Option<usize> {
        self.uses.iter().find(|(s, _)| s.start <= offset && offset <= s.end).map(|&(_, d)| d)
    }

    pub fn definition_at(&self, offset: usize) -> Option<Span> {
        self.def_at(offset).map(|d| self.defs[d].span)
    }

    /// Every place that names the same thing as the name at `offset`, the definition included.
    /// A rule's references include the captures that use its implicit capture.
    pub fn references_at(&self, offset: usize) -> Vec<Span> {
        let Some(d) = self.def_at(offset) else { return Vec::new() };
        self.uses.iter().filter(|&&(_, e)| e == d).map(|&(s, _)| s).collect()
    }

    /// The span and current text of a renamable name at `offset`.
    pub fn rename_target(&self, offset: usize) -> Option<(Span, String)> {
        let d = self.def_at(offset)?;
        let (span, _) = *self.uses.iter().find(|(s, _)| s.start <= offset && offset <= s.end)?;
        Some((span, self.defs[d].name.clone()))
    }

    pub fn token_at(&self, offset: usize) -> Option<&SemToken> {
        self.tokens.iter().find(|t| t.span.start <= offset && offset <= t.span.end && t.span.end > t.span.start)
    }

    /// Markdown hover text for the token at `offset`.
    pub fn hover_at(&self, offset: usize) -> Option<(Span, String)> {
        let tok = self.token_at(offset)?;
        let text = &self.src[tok.span.start..tok.span.end];
        let body = match tok.kind {
            TokenKind::Keyword | TokenKind::Primitive => keyword_doc(text)?.to_string(),
            TokenKind::Function => function_doc(text)?.to_string(),
            TokenKind::Rule | TokenKind::Alias | TokenKind::Label | TokenKind::Capture | TokenKind::Variable => {
                let d = self.def_at(offset)?;
                self.def_doc(d)
            }
            _ => return None,
        };
        Some((tok.span, body))
    }

    fn def_doc(&self, d: usize) -> String {
        let def = &self.defs[d];
        let block = &self.blocks[def.block];
        match def.kind {
            DefKind::Rule | DefKind::Alias => {
                let mut out = String::new();
                if !block.doc.is_empty() {
                    out.push_str(&block.doc.join("\n"));
                    out.push_str("\n\n");
                }
                out.push_str(&format!("```txtql\n{}\n```", &self.src[block.span.start..block.span.end]));
                out
            }
            DefKind::Label => {
                let extent = self.label_extent.get(&def.span.start).copied().unwrap_or(def.span);
                format!(
                    "capture `{}` of rule `{}`\n\n```txtql\n{}\n```",
                    def.name,
                    block.name,
                    &self.src[extent.start..extent.end]
                )
            }
            DefKind::Variable => format!("loop variable `{}`: one item at a time", def.name),
        }
    }

    fn section_at(&self, offset: usize) -> (Section, Option<usize>) {
        self.sections
            .iter()
            .rev()
            .find(|(s, _, _)| s.start < offset)
            .map_or((Section::Outside, None), |&(_, sec, b)| (sec, b))
    }

    /// Completions that fit at `offset`; the editor filters them by the word being typed.
    pub fn completions_at(&self, offset: usize) -> Vec<Completion> {
        let (section, block) = self.section_at(offset);
        let mut out = Vec::new();
        let kw = |out: &mut Vec<Completion>, kws: &[Kw], kind| {
            for k in kws {
                let label = k.as_str();
                let detail = keyword_doc(label).unwrap_or_default().lines().next().unwrap_or_default().to_string();
                out.push(Completion { label: label.to_string(), kind, detail });
            }
        };
        let names = |out: &mut Vec<Completion>| {
            for b in &self.blocks {
                if out.iter().any(|c| c.label == b.name) {
                    continue;
                }
                let kind = if b.kind == DefKind::Alias { CompletionKind::Alias } else { CompletionKind::Rule };
                let text = &self.src[b.span.start..b.span.end];
                out.push(Completion { label: b.name.clone(), kind, detail: first_line(text) });
            }
        };
        let captures = |out: &mut Vec<Completion>| {
            let Some(b) = block else { return };
            for (span, d) in &self.uses {
                let def = &self.defs[*d];
                let in_block = self.blocks[b].span.start <= span.start && span.end <= self.blocks[b].span.end;
                if !in_block || out.iter().any(|c| c.label == def.name) {
                    continue;
                }
                let detail = match def.kind {
                    DefKind::Label => "capture".to_string(),
                    DefKind::Rule if def.block != b => format!("capture of rule `{}`", def.name),
                    DefKind::Variable => "loop variable".to_string(),
                    _ => continue,
                };
                out.push(Completion { label: def.name.clone(), kind: CompletionKind::Capture, detail });
            }
        };
        let functions = |out: &mut Vec<Completion>| {
            for (name, _, _) in FUNCTIONS {
                let detail = function_doc(name).unwrap_or_default().lines().next().unwrap_or_default().to_string();
                out.push(Completion { label: name.to_string(), kind: CompletionKind::Function, detail });
            }
        };
        match section {
            Section::Outside => kw(&mut out, &[Kw::ALIAS, Kw::STRICT], CompletionKind::Keyword),
            Section::Pattern => {
                kw(&mut out, PRIMITIVES, CompletionKind::Primitive);
                kw(&mut out, PATTERN_KEYWORDS, CompletionKind::Keyword);
                kw(&mut out, &[Kw::ALIAS, Kw::STRICT], CompletionKind::Keyword);
                names(&mut out);
            }
            Section::Where => {
                captures(&mut out);
                functions(&mut out);
                kw(&mut out, CONDITION_KEYWORDS, CompletionKind::Keyword);
            }
            Section::Template => {
                captures(&mut out);
                functions(&mut out);
                kw(&mut out, TEMPLATE_KEYWORDS, CompletionKind::Keyword);
                for c in CONSTANTS {
                    out.push(Completion {
                        label: c.to_string(),
                        kind: CompletionKind::Constant,
                        detail: String::new(),
                    });
                }
                kw(&mut out, &[Kw::ALIAS, Kw::STRICT], CompletionKind::Keyword);
            }
        }
        out
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or_default().trim().to_string()
}

/// The `--` comment lines directly above `start` (no blank line in between).
fn doc_comments(src: &str, comments: &[Span], start: usize) -> Vec<String> {
    let line_start = src[..start].rfind('\n').map_or(0, |i| i + 1);
    let mut lines = Vec::new();
    let mut expect_end = line_start;
    for c in comments.iter().rev().filter(|c| c.end <= line_start) {
        // The comment must fill its own line and end right before the expected line.
        let c_line_start = src[..c.start].rfind('\n').map_or(0, |i| i + 1);
        let rest = &src[c.end..expect_end];
        if !src[c_line_start..c.start].trim().is_empty() || rest.trim_start_matches('\r') != "\n" {
            break;
        }
        lines.push(src[c.start + 2..c.end].trim().to_string());
        expect_end = c_line_start;
    }
    lines.reverse();
    lines
}

fn label_extents(q: &ast::Query) -> HashMap<usize, Span> {
    fn walk(p: &Pattern, out: &mut HashMap<usize, Span>) {
        match &p.kind {
            PatKind::Label(name, inner) => {
                out.insert(name.span.start, p.span);
                walk(inner, out);
            }
            PatKind::Seq(items) | PatKind::Or(items) => items.iter().for_each(|i| walk(i, out)),
            PatKind::Repeat(r) => {
                walk(&r.item, out);
                for x in r.sep.iter().chain(&r.skip).chain(r.until.as_ref().map(|u| &u.stop)) {
                    walk(x, out);
                }
            }
            _ => {}
        }
    }
    let mut out = HashMap::new();
    for r in &q.rules {
        walk(&r.pattern, &mut out);
    }
    out
}

/// The query's errors and warnings, as reported by `txtql check`.
pub fn diagnostics(src: &str) -> Vec<Diag> {
    match Query::compile(src) {
        Ok(q) => q.warnings.iter().map(|w| diag(w, Severity::Warning)).collect(),
        Err(CompileError::Parse(e)) => vec![diag(&e, Severity::Error)],
        Err(CompileError::Check(errs)) => {
            errs.iter().map(|e| diag(e, if e.is_warning() { Severity::Warning } else { Severity::Error })).collect()
        }
    }
}

fn diag(d: &dyn Diagnostic, severity: Severity) -> Diag {
    let labels: Vec<miette::LabeledSpan> = d.labels().into_iter().flatten().collect();
    let primary = labels.iter().position(|l| l.primary()).unwrap_or(0);
    let to_span = |l: &miette::LabeledSpan| Span::new(l.offset(), l.offset() + l.len());
    let mut message = d.to_string();
    if let Some(label) = labels.get(primary).and_then(|l| l.label()) {
        message.push_str(&format!("\n{label}"));
    }
    if let Some(help) = d.help() {
        message.push_str(&format!("\nhelp: {help}"));
    }
    let related = labels
        .iter()
        .enumerate()
        .filter(|&(k, _)| k != primary)
        .map(|(_, l)| (to_span(l), l.label().unwrap_or_default().to_string()))
        .collect();
    Diag {
        span: labels.get(primary).map(to_span).unwrap_or_default(),
        severity,
        code: d.code().map(|c| c.to_string()),
        message,
        related,
    }
}

/// Documentation for keywords and primitives: a one-line summary, then details.
pub fn keyword_doc(word: &str) -> Option<&'static str> {
    Some(match word.to_ascii_uppercase().as_str() {
        "WORD" => "a whole run of letters\n\nNever part of a run: in `abc1`, `ab` is not a WORD.",
        "INT" => "a whole run of digits\n\n`3.14` holds two INTs.",
        "FLOAT" => "a number with an optional decimal part\n\n`42`, `3.5`",
        "HEX" => "a run of hexadecimal digits, not followed by other letters or digits",
        "BIN" => "a run of binary digits, not followed by other letters or digits",
        "IPV4" => "an IPv4 address such as `199.72.81.55`\n\nEach part is at most 255.",
        "IPV6" => {
            "an IPv6 address such as `2001:db8::1` or `::ffff:192.0.2.1`\n\nThe longest address at that point; a port is written `'[' IPV6 ']:' INT`."
        }
        "LETTER" => "one letter, also inside a run",
        "DIGIT" => "one digit, also inside a run\n\n`1 TO n DIGIT` splits `987` into digits.",
        "PUNCT" => "one character that is not a letter, digit or whitespace",
        "ANY" => "any one character, line breaks included",
        "LINE" => "a whole line, without its line break (may be empty)",
        "NL" => "a line break: `\\n`, `\\r\\n` or `\\r`",
        "TAB" => "a tab character",
        "ROW" => {
            "empty text, worth the current line number (from 1)\n\n`line = n:ROW 1 TO n words:WORD SPLITBY ' ' NL AS { n: words }`"
        }
        "COL" => "empty text, worth the current column (from 1, in characters)",
        "EOF" => "empty text at the end of the input\n\nUseful in stops: `ANY UNTILBEFORE (NL '[' OR NL EOF)`.",
        "OR" => "alternatives, tried in order: `a OR b`",
        "TO" => {
            "repetition: `min TO max pattern`, with `n` for no upper limit\n\n\
             Captures inside are lists, except when the maximum is 1 (`0 TO 1`): then they are the value, or null."
        }
        "N" => "no upper limit, in `min TO n pattern`",
        "LAZY" => "prefer fewer repetitions: `1 TO n LAZY ANY`",
        "SPLITBY" => "items separated by a pattern: `1 TO n WORD SPLITBY ', '`",
        "SKIPPING" => "allow text matching a pattern before, between and after the items",
        "UNTIL" => {
            "end the repetition at the first stop, and consume the stop too\n\n\
             `ANY UNTIL NL`. The stop must be there and is never part of a value. Without bounds, `p UNTIL s` \
             means `0 TO n p UNTIL s`. The stop is checked wherever an iteration would begin."
        }
        "UNTILBEFORE" => {
            "end the repetition right before the first stop, which must follow and is left for what comes next\n\n\
             `ANY UNTILBEFORE ' '`. Without bounds, `p UNTILBEFORE s` means `0 TO n p UNTILBEFORE s`."
        }
        "WHERE" => "a condition the match must satisfy; if it is false, another reading is tried",
        "AS" => "the JSON template that builds the rule's value",
        "ALIAS" => "a named pattern fragment that never captures: `ALIAS sp = 1 TO n ' '`",
        "STRICT" => "make ambiguity in this rule an error instead of a warning",
        "FOR" => "one element per item of a list: `[ e FOR x ]`, `[ e FOR x IN list ]`",
        "LISTOF" => {
            "collect every value of a repeated key into a list: `{ r.team: LISTOF r.name FOR r IN rows }`\n\nThe value is always a list, even with one item."
        }
        "IN" => "the list to iterate: `FOR x IN list`",
        "AND" => "both conditions hold",
        "NOT" => "the condition does not hold",
        "CONTAINS" => "text contains text, or a list contains a value",
        "STARTSWITH" => "text starts with text",
        "ENDSWITH" => "text ends with text",
        _ => return None,
    })
}

pub fn function_doc(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_uppercase().as_str() {
        "NUM" => "`NUM(text)`: the text as a number (`null` stays `null`)",
        "LOWER" => "`LOWER(text)`: lower case",
        "UPPER" => "`UPPER(text)`: upper case",
        "TRIM" => "`TRIM(text)`: without surrounding whitespace",
        "COUNT" => "`COUNT(list)`: the number of items",
        "FIRST" => "`FIRST(list)`: the first item, or `null`",
        "ZIP" => {
            "`ZIP(keys, values)`: an object pairing each key with the value in the same position\n\nMissing values are `null`; extra values are an error. `[ ZIP(header, r) FOR r IN rows ]` reads a CSV with a header line."
        }
        "LAST" => "`LAST(list)`: the last item, or `null`",
        "JOIN" => {
            "`JOIN(list, separator)`: the items joined into one text with the separator between them (the separator is required)"
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(a: &Analysis) -> Vec<(&str, TokenKind, bool)> {
        a.tokens.iter().map(|t| (&a.src[t.span.start..t.span.end], t.kind, t.definition)).collect()
    }

    fn at(src: &str, needle: &str) -> usize {
        src.find(needle).unwrap_or_else(|| panic!("{needle} not in {src}"))
    }

    #[test]
    fn classifies_tokens() {
        let src = "-- doc\nTEXT = 1 TO n r:rec WHERE COUNT(r) > 0 AS { 'rows': r, 'n': null }\nrec = WORD sp NL\nALIAS sp = ' '";
        let a = Analysis::new(src);
        use TokenKind::*;
        assert_eq!(
            kinds(&a),
            vec![
                ("-- doc", Comment, false),
                ("TEXT", Rule, true),
                ("1", Number, false),
                ("TO", Keyword, false),
                ("n", Keyword, false),
                ("r", Label, true),
                ("rec", Rule, false),
                ("WHERE", Keyword, false),
                ("COUNT", Function, false),
                ("r", Capture, false),
                ("0", Number, false),
                ("AS", Keyword, false),
                ("'rows'", String, false),
                ("r", Capture, false),
                ("'n'", String, false),
                ("null", Constant, false),
                ("rec", Rule, true),
                ("WORD", Primitive, false),
                ("sp", Alias, false),
                ("NL", Primitive, false),
                ("ALIAS", Keyword, false),
                ("sp", Alias, true),
                ("' '", String, false),
            ]
        );
    }

    #[test]
    fn conditions_are_not_definitions() {
        let src = "r = w:WORD WHERE w = 'x' AND (w = 'y' OR NOT w = 'z')\nTEXT = r";
        let a = Analysis::new(src);
        assert_eq!(a.blocks.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["r", "TEXT"]);
        assert_eq!(a.references_at(at(src, "w:")).len(), 4);
    }

    #[test]
    fn go_to_definition() {
        let src = "TEXT = 1 TO n e:entry AS [ e.level FOR e ]\nentry = level:WORD NL";
        let a = Analysis::new(src);
        // A rule reference goes to the rule.
        let def = a.definition_at(at(src, "entry AS")).unwrap();
        assert_eq!(&src[def.start..def.end], "entry");
        assert_eq!(def.start, at(src, "entry ="));
        // A capture goes to its label.
        let def = a.definition_at(at(src, "e.level")).unwrap();
        assert_eq!(def.start, at(src, "e:entry"));
        // A field is not resolved.
        assert_eq!(a.definition_at(at(src, "level FOR")), None);
    }

    #[test]
    fn implicit_captures_refer_to_the_rule() {
        let src = "TEXT = 1 TO n rhyme AS { rhyme }\nrhyme = WORD NL";
        let a = Analysis::new(src);
        let refs = a.references_at(at(src, "rhyme ="));
        assert_eq!(refs.len(), 3, "the reference, the capture in the template and the definition");
    }

    #[test]
    fn for_in_variables() {
        let src = "TEXT = 1 TO n ds:DIGIT AS [ NUM(d) FOR d IN ds ]";
        let a = Analysis::new(src);
        let def = a.definition_at(at(src, "d) FOR")).unwrap();
        assert_eq!(def.start, at(src, "d IN"));
        let def = a.definition_at(at(src, "ds ]")).unwrap();
        assert_eq!(def.start, at(src, "ds:"));
    }

    #[test]
    fn labels_are_scoped_to_their_rule() {
        let src = "a = x:WORD AS x\nb = x:INT AS x\nTEXT = a b";
        let a = Analysis::new(src);
        let second = src.rfind("AS x").unwrap() + 3;
        assert_eq!(a.definition_at(second).unwrap().start, at(src, "x:INT"));
        assert_eq!(a.references_at(at(src, "x:WORD")).len(), 2);
    }

    #[test]
    fn works_on_broken_queries() {
        // An unterminated string and a parse error do not stop highlighting or navigation.
        let src = "TEXT = 1 TO n entry OR OR\nentry = k:WORD '= AS k";
        let a = Analysis::new(src);
        assert!(a.diagnostics.iter().any(|d| d.severity == Severity::Error));
        assert_eq!(a.definition_at(at(src, "entry OR")).unwrap().start, at(src, "entry ="));
        assert!(a.tokens.iter().any(|t| t.kind == TokenKind::String));
    }

    #[test]
    fn hover() {
        let src = "-- One log line.\n-- Second line.\nentry = level:(ANY UNTIL ' ') NL\nTEXT = 1 TO n entry";
        let a = Analysis::new(src);
        let (_, text) = a.hover_at(src.rfind("entry").unwrap()).unwrap();
        assert!(text.starts_with("One log line.\nSecond line.\n\n```txtql\nentry = "), "{text}");
        let (_, text) = a.hover_at(at(src, "level")).unwrap();
        assert!(text.contains("level:(ANY UNTIL ' ')"), "{text}");
        let (_, text) = a.hover_at(at(src, "UNTIL")).unwrap();
        assert!(text.starts_with("end the repetition at the first stop, and consume"), "{text}");
    }

    #[test]
    fn completions_depend_on_the_section() {
        let src = "TEXT = w:WORD n:INT AS { w: NUM() }\nrec = LINE";
        let a = Analysis::new(src);
        let labels = |offset| a.completions_at(offset).into_iter().map(|c| c.label).collect::<Vec<_>>();
        let in_pattern = labels(at(src, "n:INT"));
        assert!(in_pattern.contains(&"WORD".to_string()) && in_pattern.contains(&"rec".to_string()));
        assert!(!in_pattern.contains(&"NUM".to_string()));
        let in_template = labels(at(src, ") }"));
        assert!(in_template.contains(&"w".to_string()) && in_template.contains(&"n".to_string()));
        assert!(in_template.contains(&"NUM".to_string()) && !in_template.contains(&"WORD".to_string()));
    }

    #[test]
    fn diagnostics_have_spans_codes_and_help() {
        let src = "TEXT = colr\ncolor = WORD";
        let d = diagnostics(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].code.as_deref(), Some("txtql::check::undefined_rule"));
        assert_eq!(&src[d[0].span.start..d[0].span.end], "colr");
        assert!(d[0].message.contains("help:"), "{}", d[0].message);
        let warn = diagnostics("TEXT = WORD\nunused = INT");
        assert_eq!(warn[0].severity, Severity::Warning);
    }

    #[test]
    fn doc_comments_need_to_touch_the_definition() {
        let src = "-- far\n\n-- near\nr = WORD\nTEXT = r";
        let a = Analysis::new(src);
        assert_eq!(a.blocks[0].doc, ["near"]);
    }
}
