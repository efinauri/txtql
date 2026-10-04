//! Grammar IR: every rule, parenthesised alternative and repetition becomes a nonterminal (NT)
//! whose body is a small NFA over symbols. Repetitions are loops inside an NFA rather than
//! recursive rules, so long lists are recognised in linear time.
//!
//! Transitions out of a state are ordered by preference; the extractor uses that order to
//! pick one parse when there are several.

use crate::ast::{self, PatKind, Pattern, Prim, Query, Stop};
use crate::error::Span;
use crate::input::{Input, is_punct};
use std::collections::HashMap;
use std::fmt::{self, Write};
use std::sync::Arc;

pub type NtId = u32;
pub type StateId = u32;
pub type TermId = u32;

/// Something that consumes text: one character, or a whole run or literal.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Term {
    /// A whole run of letters.
    Word,
    /// A whole run of digits, with an optional decimal part.
    Float,
    /// A whole run of digits.
    Int,
    /// A run of hexadecimal digits, not followed by other letters or digits.
    Hex,
    /// A run of binary digits, not followed by other letters or digits.
    Bin,
    /// A dotted IPv4 address.
    Ipv4,
    /// An IPv6 address.
    Ipv6,
    /// One character that is not a letter, digit or whitespace.
    Punct,
    /// Any one character, line breaks included.
    Any,
    /// One ASCII digit.
    Digit,
    /// One letter (with any combining marks after it).
    Letter,
    /// A line break (`NL`): `\n`, `\r\n` or `\r`.
    Newline,
    /// Exact text; `ci` ignores case.
    Lit { chars: Vec<char>, ci: bool },
    /// One character at which `inner` does not match (`ANY UNTILBEFORE inner`, fused).
    Not(Box<Term>),
    /// Any of several terms (the stop set of `UNTILBEFORE (a OR b)`).
    OneOf(Vec<Term>),
    /// The parts of a stop, one after the other (`UNTILBEFORE (NL DIGIT)`).
    Seq(Vec<Term>),
    /// A repeated part of a stop (`UNTIL 1 TO n ' '`).
    Rep { min: u32, max: Option<u32>, item: Box<Term> },
    /// The end of the input (only inside stops; as a pattern, `EOF` is an assertion).
    Eof,
}

impl Term {
    /// End position if the term matches at `i`.
    pub fn match_at(&self, input: &Input, i: usize) -> Option<usize> {
        if let Term::Seq(_) | Term::Rep { .. } | Term::Eof = self {
            let mut ends = Vec::new();
            self.ends(input, i, &mut ends);
            return ends.into_iter().max();
        }
        let c = input.char(i)?;
        match self {
            Term::Word => input.word_at(i),
            Term::Float => input.number_at(i),
            Term::Int => input.int_at(i),
            Term::Hex => input.hex_at(i),
            Term::Bin => input.bin_at(i),
            Term::Ipv4 => input.ipv4_at(i),
            Term::Ipv6 => input.ipv6_at(i),
            Term::Punct => is_punct(c).then_some(i + 1),
            Term::Any => Some(i + 1),
            Term::Digit => c.is_ascii_digit().then_some(i + 1),
            Term::Letter => input.letter_at(i),
            Term::Newline => input.newline_at(i),
            Term::Lit { chars, ci } => input.literal_at(i, chars, *ci),
            Term::Not(inner) => (!inner.matches(input, i)).then_some(i + 1),
            // The longest match: a stop that `UNTIL` consumes takes all of it.
            Term::OneOf(terms) => terms.iter().filter_map(|t| t.match_at(input, i)).max(),
            Term::Seq(_) | Term::Rep { .. } | Term::Eof => unreachable!("handled above"),
        }
    }

    /// True if the term matches at `i` in any way. Stops use this: a sequence or repetition can
    /// match in several ways, and any one of them is enough.
    pub fn matches(&self, input: &Input, i: usize) -> bool {
        match self {
            Term::OneOf(terms) => terms.iter().any(|t| t.matches(input, i)),
            Term::Seq(_) | Term::Rep { .. } | Term::Eof => {
                let mut ends = Vec::new();
                self.ends(input, i, &mut ends);
                !ends.is_empty()
            }
            _ => self.match_at(input, i).is_some(),
        }
    }

    /// Every end position of a match at `i`, added to `out`.
    fn ends(&self, input: &Input, i: usize, out: &mut Vec<usize>) {
        let add = |out: &mut Vec<usize>, e: usize| {
            if !out.contains(&e) {
                out.push(e);
            }
        };
        match self {
            Term::Eof => {
                if i == input.len() {
                    add(out, i);
                }
            }
            Term::OneOf(terms) => terms.iter().for_each(|t| t.ends(input, i, out)),
            Term::Seq(parts) => {
                let mut cur = vec![i];
                for part in parts {
                    let mut next = Vec::new();
                    for &p in &cur {
                        part.ends(input, p, &mut next);
                    }
                    cur = next;
                }
                cur.into_iter().for_each(|e| add(out, e));
            }
            Term::Rep { min, max, item } => {
                let mut seen = vec![i];
                let mut cur = vec![i];
                if *min == 0 {
                    add(out, i);
                }
                let mut count = 0;
                while !cur.is_empty() && max.is_none_or(|m| count < m) {
                    let mut next = Vec::new();
                    for &p in &cur {
                        item.ends(input, p, &mut next);
                    }
                    count += 1;
                    // Past the minimum, a position reached before adds nothing new.
                    if count >= *min {
                        next.retain(|e| !seen.contains(e));
                        seen.extend(&next);
                        next.iter().for_each(|&e| add(out, e));
                    }
                    cur = next;
                }
            }
            other => {
                if let Some(e) = other.match_at(input, i) {
                    add(out, e);
                }
            }
        }
    }
}

/// Zero-width checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Assert {
    /// Start of the text or right after a line break.
    LineStart,
    /// End of the text or right before a line break.
    LineEnd,
    /// `ROW`: always holds; its value is the line number.
    Row,
    /// `COL`: always holds; its value is the column.
    Col,
    /// `EOF`: the end of the input.
    End,
    /// The stop of an `UNTILBEFORE` matches here (it ends the repetition).
    At(TermId),
    /// The stop of an `UNTIL` / `UNTILBEFORE` does not match here (another iteration may begin).
    NotAt(TermId),
}

impl Assert {
    pub fn holds(self, terms: &[Term], input: &Input, i: usize) -> bool {
        match self {
            Assert::LineStart => input.is_line_start(i),
            Assert::LineEnd => input.is_line_end(i),
            Assert::Row | Assert::Col => true,
            Assert::End => i == input.len(),
            Assert::At(t) => terms[t as usize].matches(input, i),
            Assert::NotAt(t) => !terms[t as usize].matches(input, i),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sym {
    Term(TermId),
    Assert(Assert),
    Nt(NtId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Plain,
    /// One iteration of a repetition.
    Item,
    /// A `SPLITBY` separator; its captures are dropped.
    Sep,
    /// Text skipped by `SKIPPING`; its captures are dropped, and it never counts as ambiguity.
    Skip,
    /// The stop consumed by `UNTIL`; never part of a value.
    Stop,
}

#[derive(Debug, Clone)]
pub struct Trans {
    pub to: StateId,
    pub sym: Sym,
    /// Capture name. Explicit labels and implicit rule-reference captures both end up here.
    pub label: Option<Arc<str>>,
    /// The label is the implicit capture of a rule reference (not written by the user).
    pub implicit: bool,
    pub role: Role,
    /// Alternative of the enclosing rule or group this transition belongs to.
    pub alt: u32,
    /// Query location, for messages.
    pub span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub nt: NtId,
    pub trans: Vec<Trans>,
    pub accept: bool,
    /// For lazy repetitions: prefer stopping over taking another transition.
    pub accept_first: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NtKind {
    /// A user rule, by index into `Grammar::rules`.
    Rule(usize),
    /// Parenthesised or labelled sub-pattern. Its captures belong to the enclosing scope.
    Group,
    /// A repetition. Lazy repetitions prefer fewer iterations and shorter spans. A `single`
    /// one (at most one item, like `0 TO 1`) gives its item's value or null instead of a list.
    Rep { lazy: bool, single: bool },
    /// LINE: matched text only, no captures.
    Builtin,
}

#[derive(Debug, Clone)]
pub struct NtDef {
    pub kind: NtKind,
    pub start: StateId,
    pub span: Span,
    /// Short description for messages, e.g. "rule `rhyme`" or "repetition".
    pub desc: String,
    /// Capture names visible through this NT, per alternative (rules and groups) or for the
    /// repeated item (repetitions: a single entry).
    pub alt_names: Vec<Vec<Arc<str>>>,
    /// This NT is an `UNTIL` / `UNTILBEFORE` repetition (used to explain errors after it ran
    /// across lines).
    pub until: Option<ast::Stop>,
    /// This NT is an alias. It captures nothing, so its value is the text it matched.
    pub alias: bool,
    /// For a stop repetition whose stop has no line break: the repetition as written, and with
    /// `NL` added to its stop (for the hint when it ran across lines).
    pub until_fix: Option<(String, String)>,
}

impl NtDef {
    pub fn names(&self) -> impl Iterator<Item = &Arc<str>> {
        let mut seen: Vec<&Arc<str>> = Vec::new();
        for names in &self.alt_names {
            for n in names {
                if !seen.contains(&n) {
                    seen.push(n);
                }
            }
        }
        seen.into_iter()
    }

    pub fn is_lazy(&self) -> bool {
        matches!(self.kind, NtKind::Rep { lazy: true, .. })
    }
}

#[derive(Debug, Clone)]
pub struct RuleInfo {
    pub nt: NtId,
    pub name: String,
    pub strict: bool,
    pub filter: Option<ast::Cond>,
    pub template: Option<ast::Template>,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub struct Grammar {
    pub nts: Vec<NtDef>,
    pub states: Vec<State>,
    pub terms: Vec<Term>,
    pub rules: Vec<RuleInfo>,
    pub root: NtId,
}

impl Grammar {
    /// End position if terminal `t` matches at `i`.
    pub fn term_match(&self, t: TermId, input: &Input, i: usize) -> Option<usize> {
        self.terms[t as usize].match_at(input, i)
    }

    pub fn nt_of_state(&self, s: StateId) -> NtId {
        self.states[s as usize].nt
    }

    /// The rule (other than `TEXT`) that begins at state `s`: `s` is the start of that rule, or
    /// of a group or repetition at its very start. Used to explain what an expectation belongs to.
    pub fn rule_started_at(&self, mut s: StateId) -> Option<usize> {
        loop {
            let nt = self.states[s as usize].nt;
            let def = &self.nts[nt as usize];
            // A start state that is entered again (`SKIPPING`) may be far into the match.
            if def.start != s || self.states.iter().any(|st| st.trans.iter().any(|t| t.to == s)) {
                return None;
            }
            match def.kind {
                NtKind::Rule(idx) => return (nt != self.root).then_some(idx),
                NtKind::Group | NtKind::Rep { .. } => {
                    let mut callers =
                        self.states.iter().enumerate().filter(|(_, st)| st.trans.iter().any(|t| t.sym == Sym::Nt(nt)));
                    let (caller, _) = callers.next()?;
                    if callers.next().is_some() {
                        return None;
                    }
                    s = caller as StateId;
                }
                NtKind::Builtin => return None,
            }
        }
    }

    pub fn describe_sym(&self, sym: Sym) -> String {
        match sym {
            Sym::Term(t) => self.terms[t as usize].to_string(),
            Sym::Assert(a) => match a {
                Assert::LineStart => "the start of a line".into(),
                Assert::Row | Assert::Col => "a position".into(),
                Assert::End => "the end of the text".into(),
                Assert::LineEnd => "the end of a line".into(),
                Assert::At(t) => self.terms[t as usize].to_string(),
                Assert::NotAt(t) => format!("anything but {}", self.terms[t as usize]),
            },
            Sym::Nt(n) => self.nts[n as usize].desc.clone(),
        }
    }

    /// Debug dump of the whole grammar, used by snapshot tests.
    pub fn dump(&self) -> String {
        let mut out = String::new();
        for (id, nt) in self.nts.iter().enumerate() {
            writeln!(out, "nt{id} {} ({:?}) start=s{}", nt.desc, nt.kind, nt.start).unwrap();
            if !nt.alt_names.iter().all(|a| a.is_empty()) {
                writeln!(out, "  names: {:?}", nt.alt_names).unwrap();
            }
            for (sid, st) in self.states.iter().enumerate().filter(|(_, s)| s.nt == id as NtId) {
                let accept = match (st.accept, st.accept_first) {
                    (true, true) => " accept(first)",
                    (true, false) => " accept",
                    _ => "",
                };
                writeln!(out, "  s{sid}{accept}").unwrap();
                for t in &st.trans {
                    let sym = match t.sym {
                        Sym::Term(tid) => self.terms[tid as usize].to_string(),
                        Sym::Assert(Assert::At(t)) => format!("at {}", self.terms[t as usize]),
                        Sym::Assert(Assert::NotAt(t)) => format!("not at {}", self.terms[t as usize]),
                        Sym::Assert(a) => format!("{a:?}"),
                        Sym::Nt(n) => format!("nt{n}"),
                    };
                    let label = t.label.as_ref().map(|l| format!(" as {l}")).unwrap_or_default();
                    let role = if t.role == Role::Plain { String::new() } else { format!(" [{:?}]", t.role) };
                    writeln!(out, "    -> s{} {sym}{label}{role}", t.to).unwrap();
                }
            }
        }
        out
    }
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Word => f.write_str("WORD"),
            Term::Float => f.write_str("FLOAT"),
            Term::Int => f.write_str("INT"),
            Term::Hex => f.write_str("HEX"),
            Term::Bin => f.write_str("BIN"),
            Term::Ipv4 => f.write_str("IPV4"),
            Term::Ipv6 => f.write_str("IPV6"),
            Term::Punct => f.write_str("PUNCT"),
            Term::Any => f.write_str("any character"),
            Term::Digit => f.write_str("DIGIT"),
            Term::Letter => f.write_str("LETTER"),
            Term::Newline => f.write_str("a line break"),
            Term::Lit { chars, ci } => {
                let text: String = chars.iter().collect();
                write!(f, "{}{}", if *ci { "i" } else { "" }, crate::printer::quote(&text))
            }
            Term::Not(inner) => write!(f, "anything but {inner}"),
            Term::OneOf(terms) => {
                let parts: Vec<String> = terms.iter().map(|t| t.to_string()).collect();
                f.write_str(&parts.join(" or "))
            }
            Term::Seq(parts) => {
                let parts: Vec<String> = parts.iter().map(|t| t.to_string()).collect();
                f.write_str(&parts.join(" then "))
            }
            Term::Rep { min, max: Some(max), item } => write!(f, "{min} to {max} times {item}"),
            Term::Rep { min, max: None, item } => write!(f, "{min} or more times {item}"),
            Term::Eof => f.write_str("the end of the text"),
        }
    }
}

/// The terminal of a single-token primitive (everything but LINE).
fn prim_term(prim: Prim) -> Term {
    match prim {
        Prim::Word => Term::Word,
        Prim::Float => Term::Float,
        Prim::Int => Term::Int,
        Prim::Hex => Term::Hex,
        Prim::Bin => Term::Bin,
        Prim::Ipv4 => Term::Ipv4,
        Prim::Ipv6 => Term::Ipv6,
        Prim::Punct => Term::Punct,
        Prim::Any => Term::Any,
        Prim::Digit => Term::Digit,
        Prim::Letter => Term::Letter,
        Prim::Newline => Term::Newline,
        Prim::Tab => Term::Lit { chars: vec!['\t'], ci: false },
        Prim::Line => unreachable!("LINE is a nonterminal"),
        Prim::Row | Prim::Col | Prim::Eof => unreachable!("zero-width primitives are assertions"),
    }
}

/// The stop condition of `UNTILBEFORE x` / `UNTIL x`: a literal, a primitive, or alternatives of those, looking
/// through aliases.
fn stop_term(p: &Pattern, aliases: &HashMap<String, Pattern>) -> Term {
    match &p.kind {
        PatKind::Lit { text, ci } => Term::Lit { chars: text.chars().collect(), ci: *ci },
        PatKind::Prim(Prim::Eof) => Term::Eof,
        PatKind::Prim(prim) => prim_term(*prim),
        PatKind::Or(alts) => Term::OneOf(alts.iter().map(|a| stop_term(a, aliases)).collect()),
        PatKind::Seq(parts) => Term::Seq(parts.iter().map(|a| stop_term(a, aliases)).collect()),
        PatKind::Repeat(r) => Term::Rep { min: r.min, max: r.max, item: Box::new(stop_term(&r.item, aliases)) },
        PatKind::Ref(name) if aliases.contains_key(&name.name) => stop_term(&aliases[&name.name], aliases),
        _ => unreachable!("check rejects other stops"),
    }
}

/// A stop repetition as written, and with `NL` added to its stop, unless the stop already has
/// a line break.
fn until_fix(p: &Pattern, aliases: &HashMap<String, Pattern>) -> Option<(String, String)> {
    let PatKind::Repeat(r) = &p.kind else { return None };
    let u = r.until.as_ref()?;
    fn has_nl(p: &Pattern, aliases: &HashMap<String, Pattern>, depth: usize) -> bool {
        match &p.kind {
            PatKind::Prim(Prim::Newline) => true,
            PatKind::Lit { text, .. } => text.contains(['\n', '\r']),
            PatKind::Or(items) | PatKind::Seq(items) => items.iter().any(|i| has_nl(i, aliases, depth + 1)),
            PatKind::Repeat(r) => has_nl(&r.item, aliases, depth + 1),
            PatKind::Ref(name) if depth < 100 => aliases.get(&name.name).is_some_and(|a| has_nl(a, aliases, depth + 1)),
            _ => false,
        }
    }
    if has_nl(&u.stop, aliases, 0) {
        return None;
    }
    let nl = Pattern { kind: PatKind::Prim(Prim::Newline), span: u.stop.span };
    let alts = match &u.stop.kind {
        PatKind::Or(alts) => alts.iter().cloned().chain([nl]).collect(),
        _ => vec![u.stop.clone(), nl],
    };
    let mut fixed = (**r).clone();
    fixed.until = Some(ast::Until { stop: Pattern { kind: PatKind::Or(alts), span: u.stop.span }, kind: u.kind });
    let fixed = Pattern { kind: PatKind::Repeat(Box::new(fixed)), span: p.span };
    Some((crate::printer::print_pattern(p), crate::printer::print_pattern(&fixed)))
}

/// The compiled parts of an `UNTIL` / `UNTILBEFORE` clause.
struct UntilSteps {
    kind: Stop,
    /// The stop as a single term, for the zero-width checks.
    term: TermId,
    /// The stop as a step, for `UNTIL` to consume.
    consume: Option<Step>,
}

/// A symbol together with the capture metadata of its occurrence.
#[derive(Debug, Clone)]
struct Step {
    sym: Sym,
    label: Option<Arc<str>>,
    /// Explicit labels (`x:...`) make the step a capture boundary; implicit rule labels can be renamed.
    explicit: bool,
    span: Span,
}

struct Compiler {
    g: Grammar,
    term_ids: HashMap<Term, TermId>,
    rule_ids: HashMap<String, usize>,
    aliases: HashMap<String, Pattern>,
    alias_nts: HashMap<String, NtId>,
    /// Set while compiling an alias body: rule references there are not captured.
    no_capture: bool,
    line_nt: Option<NtId>,
}

/// Compiles a checked query. Panics on undefined rules; run `check` first.
pub fn compile(q: &Query) -> Grammar {
    let mut c = Compiler {
        g: Grammar { nts: Vec::new(), states: Vec::new(), terms: Vec::new(), rules: Vec::new(), root: 0 },
        term_ids: HashMap::new(),
        rule_ids: HashMap::new(),
        aliases: q.aliases.iter().map(|a| (a.name.name.clone(), a.pattern.clone())).collect(),
        alias_nts: HashMap::new(),
        no_capture: false,
        line_nt: None,
    };
    // Reserve one NT per rule first so references resolve regardless of definition order.
    for (idx, rule) in q.rules.iter().enumerate() {
        let nt = c.new_nt(NtKind::Rule(idx), rule.pattern.span, format!("rule `{}`", rule.name.name));
        c.rule_ids.insert(rule.name.name.clone(), idx);
        c.g.rules.push(RuleInfo {
            nt,
            name: rule.name.name.clone(),
            strict: rule.strict,
            filter: rule.filter.clone(),
            template: rule.template.clone(),
            span: rule.name.span,
        });
    }
    for (idx, rule) in q.rules.iter().enumerate() {
        let nt = c.g.rules[idx].nt;
        let alts: Vec<&Pattern> = match &rule.pattern.kind {
            PatKind::Or(alts) => alts.iter().collect(),
            _ => vec![&rule.pattern],
        };
        let alts: Vec<Vec<Step>> = alts.into_iter().map(|a| c.seq(a)).collect();
        c.fill_alts(nt, alts);
    }
    let root = q.root().expect("check ensures a TEXT rule");
    c.g.root = c.g.rules[c.rule_ids[&root.name.name]].nt;
    c.g
}

impl Compiler {
    fn new_nt(&mut self, kind: NtKind, span: Span, desc: String) -> NtId {
        let id = self.g.nts.len() as NtId;
        let start = self.new_state(id);
        self.g.nts.push(NtDef {
            kind,
            start,
            span,
            desc,
            alt_names: Vec::new(),
            until: None,
            alias: false,
            until_fix: None,
        });
        id
    }

    fn new_state(&mut self, nt: NtId) -> StateId {
        self.g.states.push(State { nt, ..State::default() });
        (self.g.states.len() - 1) as StateId
    }

    fn term(&mut self, t: Term) -> Sym {
        let next = self.g.terms.len() as TermId;
        let id = *self.term_ids.entry(t.clone()).or_insert(next);
        if id == next {
            self.g.terms.push(t);
        }
        Sym::Term(id)
    }

    fn add(&mut self, from: StateId, to: StateId, step: &Step, role: Role) {
        self.add_alt(from, to, step, role, 0);
    }

    fn add_alt(&mut self, from: StateId, to: StateId, step: &Step, role: Role, alt: u32) {
        let label = if matches!(role, Role::Sep | Role::Skip | Role::Stop) { None } else { step.label.clone() };
        let implicit = label.is_some() && !step.explicit;
        self.g.states[from as usize].trans.push(Trans {
            to,
            sym: step.sym,
            label,
            implicit,
            role,
            alt,
            span: step.span,
        });
    }

    /// Adds `steps` as a chain of transitions from `from` to `to`.
    fn chain(&mut self, nt: NtId, from: StateId, to: StateId, steps: &[Step], role: Role, alt: u32) {
        let mut cur = from;
        for (k, step) in steps.iter().enumerate() {
            let next = if k + 1 == steps.len() { to } else { self.new_state(nt) };
            self.add_alt(cur, next, step, role, alt);
            cur = next;
        }
    }

    /// Capture names a step exposes to the enclosing scope.
    fn step_names(&self, step: &Step) -> Vec<Arc<str>> {
        if let Some(l) = &step.label {
            return vec![l.clone()];
        }
        match step.sym {
            Sym::Nt(n) if matches!(self.g.nts[n as usize].kind, NtKind::Group | NtKind::Rep { .. }) => {
                self.g.nts[n as usize].names().cloned().collect()
            }
            _ => Vec::new(),
        }
    }

    fn fill_alts(&mut self, nt: NtId, alts: Vec<Vec<Step>>) {
        let start = self.g.nts[nt as usize].start;
        let accept = self.new_state(nt);
        self.g.states[accept as usize].accept = true;
        let mut alt_names = Vec::new();
        for (k, steps) in alts.iter().enumerate() {
            let mut names = Vec::new();
            for s in steps {
                for n in self.step_names(s) {
                    if !names.contains(&n) {
                        names.push(n);
                    }
                }
            }
            alt_names.push(names);
            self.chain(nt, start, accept, steps, Role::Plain, k as u32);
        }
        self.g.nts[nt as usize].alt_names = alt_names;
    }

    fn group(&mut self, alts: Vec<Vec<Step>>, span: Span) -> Step {
        let nt = self.new_nt(NtKind::Group, span, "group".into());
        self.fill_alts(nt, alts);
        Step { sym: Sym::Nt(nt), label: None, explicit: false, span }
    }

    /// Compiles a pattern into exactly one step, wrapping it in a group if needed.
    fn single(&mut self, p: &Pattern) -> Step {
        let mut steps = self.seq(p);
        if steps.len() == 1 { steps.pop().unwrap() } else { self.group(vec![steps], p.span) }
    }

    fn seq(&mut self, p: &Pattern) -> Vec<Step> {
        let plain = |sym| Step { sym, label: None, explicit: false, span: p.span };
        match &p.kind {
            PatKind::Seq(items) => items.iter().flat_map(|i| self.seq(i)).collect(),
            PatKind::Or(alts) => {
                let alts = alts.iter().map(|a| self.seq(a)).collect();
                vec![self.group(alts, p.span)]
            }
            PatKind::Lit { text, ci } => vec![plain(self.term(Term::Lit { chars: text.chars().collect(), ci: *ci }))],
            PatKind::Prim(prim) => vec![match prim {
                Prim::Line => plain(Sym::Nt(self.line_nt())),
                Prim::Row => plain(Sym::Assert(Assert::Row)),
                Prim::Col => plain(Sym::Assert(Assert::Col)),
                Prim::Eof => plain(Sym::Assert(Assert::End)),
                other => plain(self.term(prim_term(*other))),
            }],
            PatKind::Ref(name) if self.aliases.contains_key(&name.name) => {
                vec![Step { sym: Sym::Nt(self.alias_nt(&name.name)), label: None, explicit: false, span: name.span }]
            }
            PatKind::Ref(name) => {
                let idx = self.rule_ids[&name.name];
                let label = (!self.no_capture).then(|| name.name.as_str().into());
                vec![Step { sym: Sym::Nt(self.g.rules[idx].nt), label, explicit: false, span: name.span }]
            }
            PatKind::Label(name, inner) => {
                let mut steps = self.seq(inner);
                let mut step = if steps.len() == 1 && !steps[0].explicit {
                    steps.pop().unwrap()
                } else {
                    self.group(vec![steps], inner.span)
                };
                step.label = Some(name.name.as_str().into());
                step.explicit = true;
                step.span = p.span;
                vec![step]
            }
            PatKind::Repeat(r) => {
                let item = self.single(&r.item);
                let sep = r.sep.as_ref().map(|s| self.single(s));
                let skip = r.skip.as_ref().map(|s| self.single(s));
                let until = r.until.as_ref().map(|u| {
                    let stop = stop_term(&u.stop, &self.aliases);
                    let Sym::Term(term) = self.term(stop) else { unreachable!() };
                    // `UNTIL` consumes the longest match of its stop, as one term: how much of the
                    // stop is taken never makes a second reading.
                    let consume = (u.kind == Stop::After).then_some(Step {
                        sym: Sym::Term(term),
                        label: None,
                        explicit: false,
                        span: u.stop.span,
                    });
                    UntilSteps { kind: u.kind, term, consume }
                });
                let nt = self.new_nt(
                    NtKind::Rep { lazy: r.lazy, single: r.max.is_some_and(|m| m <= 1) },
                    p.span,
                    "repetition".into(),
                );
                self.g.nts[nt as usize].until = until.as_ref().map(|u| u.kind);
                self.g.nts[nt as usize].until_fix = until_fix(p, &self.aliases);
                self.repeat_body(nt, r.min, r.max, r.lazy, item, sep, skip, until);
                vec![plain(Sym::Nt(nt))]
            }
        }
    }

    /// Builds the NFA of a counted repetition:
    ///
    /// ```text
    /// before0 -item-> after[1] -sep-> mid -item-> after[2] ...
    /// ```
    ///
    /// Counts above `cap` share states (for unbounded repetitions `cap = max(min, 1)`).
    /// Accepting: `after[c]` for `c >= min`, and `before0` if `min == 0`.
    ///
    /// `SKIPPING p` adds a self-loop on `p` to `before0`, every `after[c]` and every `mid`
    /// state. A loop keeps the parser at one item per position; a separate "skipped text"
    /// rule would be restarted after every item and make long inputs quadratic. Transitions
    /// are ordered continue, then skip, so skipping only happens where nothing else fits.
    ///
    /// `UNTILBEFORE s` / `UNTIL s` guard the start of every iteration (the separator, if
    /// there is one) with "`s` does not match here", and replace accepting with a transition to
    /// a final state: "`s` matches here", or `s` itself. `ANY` fuses with its guard into the
    /// one-character term "anything but `s`".
    #[allow(clippy::too_many_arguments)]
    fn repeat_body(
        &mut self,
        nt: NtId,
        min: u32,
        max: Option<u32>,
        lazy: bool,
        mut item: Step,
        sep: Option<Step>,
        skip: Option<Step>,
        until: Option<UntilSteps>,
    ) {
        self.g.nts[nt as usize].alt_names = vec![self.step_names(&item)];
        let before0 = self.g.nts[nt as usize].start;
        let mut guard = None;
        let mut end = None;
        if let Some(u) = until {
            let fused = sep.is_none() && matches!(item.sym, Sym::Term(t) if self.g.terms[t as usize] == Term::Any);
            if fused {
                item.sym = self.term(Term::Not(Box::new(self.g.terms[u.term as usize].clone())));
            } else {
                guard = Some(Step {
                    sym: Sym::Assert(Assert::NotAt(u.term)),
                    label: None,
                    explicit: false,
                    span: item.span,
                });
            }
            let fin = self.new_state(nt);
            self.g.states[fin as usize].accept = true;
            let step = match u.consume {
                Some(stop) => (stop, Role::Stop),
                None => {
                    let at =
                        Step { sym: Sym::Assert(Assert::At(u.term)), label: None, explicit: false, span: item.span };
                    (at, Role::Plain)
                }
            };
            end = Some((fin, step));
        }
        let cap = match max {
            Some(m) => m,
            None => min.max(1),
        };
        // after[0] is before0.
        let after: Vec<StateId> = (0..=cap).map(|c| if c == 0 { before0 } else { self.new_state(nt) }).collect();
        for c in 0..=cap {
            let from = after[c as usize];
            let can_continue = max.is_none_or(|m| c < m);
            if can_continue {
                let next = after[(c + 1).min(cap) as usize];
                let begin = match &guard {
                    Some(g) => {
                        let s = self.new_state(nt);
                        self.add(from, s, g, Role::Plain);
                        s
                    }
                    None => from,
                };
                match &sep {
                    Some(sep) if c > 0 => {
                        let mid = self.new_state(nt);
                        self.add(begin, mid, sep, Role::Sep);
                        self.add(mid, next, &item, Role::Item);
                        if let Some(k) = &skip {
                            self.add(mid, mid, k, Role::Skip);
                        }
                    }
                    _ => self.add(begin, next, &item, Role::Item),
                }
            }
            if let Some(k) = &skip {
                self.add(from, from, k, Role::Skip);
            }
            if c >= min.max(1) || (c == 0 && min == 0) {
                match &end {
                    Some((fin, (step, role))) => self.add(from, *fin, step, *role),
                    None => {
                        let st = &mut self.g.states[from as usize];
                        st.accept = true;
                        st.accept_first = lazy;
                    }
                }
            }
        }
    }

    /// The group an alias compiles to: its pattern with no captures, shared by all its uses.
    fn alias_nt(&mut self, name: &str) -> NtId {
        if let Some(&nt) = self.alias_nts.get(name) {
            return nt;
        }
        let pattern = self.aliases[name].clone();
        let nt = self.new_nt(NtKind::Group, pattern.span, format!("alias `{name}`"));
        self.g.nts[nt as usize].alias = true;
        // Registered before compiling the body, so even a (rejected) recursive alias terminates.
        self.alias_nts.insert(name.to_string(), nt);
        let saved = std::mem::replace(&mut self.no_capture, true);
        let alts: Vec<&Pattern> = match &pattern.kind {
            PatKind::Or(alts) => alts.iter().collect(),
            _ => vec![&pattern],
        };
        let alts: Vec<Vec<Step>> = alts.into_iter().map(|a| self.seq(a)).collect();
        self.fill_alts(nt, alts);
        self.no_capture = saved;
        nt
    }

    fn line_nt(&mut self) -> NtId {
        if let Some(n) = self.line_nt {
            return n;
        }
        // LineStart (not a line break)* LineEnd: a whole line, without its line break.
        let nt = self.new_nt(NtKind::Builtin, Span::default(), "LINE".into());
        let s0 = self.g.nts[nt as usize].start;
        let [s1, s2] = [(); 2].map(|_| self.new_state(nt));
        let st = |sym| Step { sym, label: None, explicit: false, span: Span::default() };
        let not_nl = self.term(Term::Not(Box::new(Term::Newline)));
        self.add(s0, s1, &st(Sym::Assert(Assert::LineStart)), Role::Plain);
        self.add(s1, s1, &st(not_nl), Role::Plain);
        self.add(s1, s2, &st(Sym::Assert(Assert::LineEnd)), Role::Plain);
        self.g.states[s2 as usize].accept = true;
        self.line_nt = Some(nt);
        nt
    }
}
