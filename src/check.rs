//! Static analysis of a parsed query: name resolution, capture scoping, shape inference for
//! templates, and lints. All problems are collected, not just the first.

use crate::ast::*;
use crate::error::{CheckError, Span};
use std::collections::{HashMap, HashSet};

/// Largest finite repetition bound.
pub const MAX_BOUND: u32 = 10_000;

pub const FUNCTIONS: &[(&str, usize, usize)] = &[
    ("NUM", 1, 1),
    ("LOWER", 1, 1),
    ("UPPER", 1, 1),
    ("TRIM", 1, 1),
    ("COUNT", 1, 1),
    ("FIRST", 1, 1),
    ("LAST", 1, 1),
    ("JOIN", 2, 2),
    ("ZIP", 2, 2),
];

/// Statically known shape of a value.
#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Text,
    Num,
    Bool,
    Null,
    Array(Box<Shape>),
    /// `None` when the keys are not known statically.
    Object(Option<Vec<(String, Shape)>>),
    Unknown,
}

impl Shape {
    pub fn describe(&self) -> &'static str {
        match self {
            Shape::Text => "text",
            Shape::Num => "a number",
            Shape::Bool => "a boolean",
            Shape::Null => "null",
            Shape::Array(_) => "a list",
            Shape::Object(_) => "an object",
            Shape::Unknown => "unknown",
        }
    }

    fn unify(self, other: Shape) -> Shape {
        if self == other { self } else { Shape::Unknown }
    }
}

pub fn check(q: &Query, src: &str) -> Vec<CheckError> {
    let mut c = Checker::new(q, src);
    c.run();
    c.errors.sort_by_key(|e| first_span(e).map(|s| (s.start, s.end)));
    c.errors
}

fn first_span(e: &CheckError) -> Option<Span> {
    use miette::Diagnostic;
    let label = e.labels()?.next()?;
    Some(Span::new(label.offset(), label.offset() + label.len()))
}

fn is_constant_name(name: &str) -> bool {
    ["true", "false", "null"].iter().any(|c| name.eq_ignore_ascii_case(c))
}

/// Every label in `p`, in all parts of nested patterns.
fn labels_in<'p>(p: &'p Pattern, out: &mut Vec<(&'p Ident, &'static str)>) {
    match &p.kind {
        PatKind::Label(name, inner) => {
            out.push((name, "label"));
            labels_in(inner, out);
        }
        PatKind::Seq(items) | PatKind::Or(items) => items.iter().for_each(|i| labels_in(i, out)),
        PatKind::Repeat(r) => {
            labels_in(&r.item, out);
            for p in r.sep.iter().chain(&r.skip).chain(r.until.as_ref().map(|u| &u.stop)) {
                labels_in(p, out);
            }
        }
        PatKind::Lit { .. } | PatKind::Prim(_) | PatKind::Ref(_) => {}
    }
}

/// Levenshtein distance, for "did you mean" suggestions.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, &cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

pub fn suggest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let best = candidates
        .into_iter()
        .filter(|c| *c != name)
        .map(|c| (distance(&name.to_lowercase(), &c.to_lowercase()), c))
        .filter(|(d, c)| *d <= (c.chars().count().max(name.chars().count()) / 3).max(1))
        .min_by_key(|(d, _)| *d)?;
    Some(best.1.to_string())
}

#[derive(Debug, Clone)]
struct Capture {
    name: String,
    shape: Shape,
    span: Span,
}

#[derive(Debug, Clone)]
struct Env {
    vars: Vec<(String, Shape)>,
    /// Inside `FOR` over values of unknown shape, any name may be a field.
    open: bool,
}

impl Env {
    fn get(&self, name: &str) -> Option<&Shape> {
        self.vars.iter().rev().find(|(n, _)| n == name).map(|(_, s)| s)
    }
}

struct Checker<'q> {
    q: &'q Query,
    src: &'q str,
    rules: HashMap<&'q str, &'q Rule>,
    aliases: HashMap<&'q str, &'q Alias>,
    errors: Vec<CheckError>,
    nullable: HashMap<&'q str, bool>,
    shapes: HashMap<&'q str, Shape>,
    shape_in_progress: HashSet<&'q str>,
}

impl<'q> Checker<'q> {
    fn new(q: &'q Query, src: &'q str) -> Self {
        Checker {
            q,
            src,
            rules: HashMap::new(),
            aliases: HashMap::new(),
            errors: Vec::new(),
            nullable: HashMap::new(),
            shapes: HashMap::new(),
            shape_in_progress: HashSet::new(),
        }
    }

    fn run(&mut self) {
        // Rules and aliases share one namespace.
        let mut defined: HashMap<&str, Span> = HashMap::new();
        // Every spelling of the root's name is the same name.
        let key = |name: &'q str| if is_root_name(name) { "TEXT" } else { name };
        let mut define = |this: &mut Self, name: &'q Ident| match defined.get(key(&name.name)) {
            Some(&first) => {
                this.errors.push(CheckError::DuplicateRule { name: name.name.clone(), first, second: name.span });
                false
            }
            None => {
                defined.insert(key(&name.name), name.span);
                true
            }
        };
        for alias in &self.q.aliases {
            if define(self, &alias.name) {
                self.aliases.insert(&alias.name.name, alias);
            }
        }
        for rule in &self.q.rules {
            if define(self, &rule.name) {
                self.rules.insert(&rule.name.name, rule);
            }
        }
        if let Some(alias) = self.aliases.iter().find(|(n, _)| is_root_name(n)).map(|(_, a)| *a) {
            self.errors.push(CheckError::RootIsAlias { span: alias.name.span });
        } else if self.q.root().is_none() {
            self.errors.push(CheckError::MissingRoot { span: self.q.span });
        }

        self.reserved_names();

        for alias in &self.q.aliases {
            self.pattern(&alias.pattern, None);
            self.no_labels_in_alias(&alias.pattern);
        }
        for rule in &self.q.rules {
            self.pattern(&rule.pattern, None);
        }
        self.alias_cycles();
        // Later passes assume every reference resolves and patterns are well formed. Lints
        // (warnings) from the first pass must not stop them.
        if self.errors.iter().any(|e| !e.is_warning()) {
            return;
        }

        self.compute_nullable();
        for alias in &self.q.aliases {
            self.loops(&alias.pattern);
        }
        for rule in &self.q.rules {
            self.loops(&rule.pattern);
        }
        self.empty_cycles();
        self.lints();

        for rule in &self.q.rules {
            let captures = self.captures(&rule.pattern, true);
            let env = Env { vars: captures.into_iter().map(|c| (c.name, c.shape)).collect(), open: false };
            if let Some(cond) = &rule.filter {
                self.cond(cond, &env);
            }
            if let Some(t) = &rule.template {
                self.tmpl(t, &env);
            }
        }
    }

    // ---- pass 1: names, bounds, literals ----

    fn pattern(&mut self, p: &'q Pattern, outer_rep: Option<Span>) {
        match &p.kind {
            PatKind::Lit { text, .. } => {
                if text.is_empty() {
                    self.errors.push(CheckError::EmptyLiteral { span: p.span });
                }
            }
            PatKind::Prim(_) => {}
            PatKind::Ref(name) => {
                if !self.rules.contains_key(name.name.as_str()) && !self.aliases.contains_key(name.name.as_str()) {
                    let help = suggest(&name.name, self.rules.keys().chain(self.aliases.keys()).copied())
                        .map(|s| format!("did you mean `{s}`?"));
                    self.errors.push(CheckError::UndefinedRule { name: name.name.clone(), span: name.span, help });
                }
            }
            PatKind::Label(_, inner) => self.pattern(inner, outer_rep),
            PatKind::Or(alts) => {
                // A branch equal to an earlier one can never be chosen: the earlier one wins.
                for k in 1..alts.len() {
                    if let Some(first) = (0..k).find(|&j| self.same_pattern(&alts[j], &alts[k], 0)) {
                        self.errors.push(CheckError::DuplicateBranch { first: alts[first].span, second: alts[k].span });
                    }
                }
                for i in alts {
                    self.pattern(i, None);
                }
            }
            PatKind::Seq(items) => {
                // Anything around the inner repetition anchors it, so only direct nesting warns.
                for i in items {
                    self.pattern(i, None);
                }
            }
            PatKind::Repeat(r) => {
                let bound_span = Span::new(p.span.start, p.span.start + 1);
                if r.min > MAX_BOUND || r.max.is_some_and(|m| m > MAX_BOUND) {
                    let bound = r.min.max(r.max.unwrap_or(0));
                    self.errors.push(CheckError::BoundTooLarge { bound: bound.into(), limit: MAX_BOUND, span: p.span });
                } else if let Some(max) = r.max
                    && r.min > max
                {
                    self.errors.push(CheckError::BadBounds { min: r.min, max, span: bound_span.to(p.span) });
                }
                // Label only the `min TO max` head of each repetition: nested full spans are
                // hard to tell apart when rendered.
                let head = self.repeat_head(p, r);
                if r.max.is_none()
                    && let Some(outer) = outer_rep
                    && r.sep.is_none()
                    && r.until.is_none()
                {
                    self.errors.push(CheckError::NestedRepetition { inner: head, outer });
                }
                // A stop anchors the inner repetition just as a separator does.
                let this = (r.max.is_none() && r.sep.is_none() && r.until.is_none()).then_some(head);
                self.pattern(&r.item, this);
                if let Some(u) = &r.until {
                    let keyword = u.kind.keyword();
                    if let Some(bad) = self.bad_stop(&u.stop, &mut Vec::new()) {
                        let help = format!(
                            "a stop is built from literals and primitives (except LINE, ROW and COL), with sequences, \
                             OR and repetitions, e.g. `{keyword} (NL DIGIT OR NL EOF)`; rules and labels are not allowed"
                        );
                        self.errors.push(CheckError::BadStop { keyword, span: bad.span, help });
                    } else if self.stop_is_any(&u.stop, 0) {
                        let help = "ANY matches at every character, so the repetition would end at once; \
                                    stop at something specific, e.g. `ANY UNTILBEFORE (',' OR NL)`"
                            .to_string();
                        self.errors.push(CheckError::BadStop { keyword, span: u.stop.span, help });
                    } else if self.stop_can_be_empty(&u.stop, 0) {
                        self.errors.push(CheckError::EmptyStop { keyword, span: u.stop.span });
                    }
                }
                if let Some(s) = &r.sep {
                    self.pattern(s, None);
                }
                if let Some(s) = &r.skip {
                    self.pattern(s, None);
                }
            }
        }
    }

    /// The `min TO max [LAZY]` part of a repetition, without surrounding parentheses.
    fn repeat_head(&self, p: &Pattern, r: &Repeat) -> Span {
        let paren_or_space = |c: char| c.is_whitespace() || c == '(';
        let start = p.span.start
            + (self.src[p.span.start..].len() - self.src[p.span.start..].trim_start_matches(paren_or_space).len());
        let end = self.src[..r.item.span.start].trim_end_matches(paren_or_space).len();
        if start < end { Span::new(start, end) } else { p.span }
    }

    // ---- pass 2: nullability, empty loops, empty cycles ----

    /// The first part of an `UNTIL` / `UNTILBEFORE` stop that is not built from literals and
    /// primitives (other than LINE, ROW and COL) with sequences, `OR` and plain repetitions.
    /// Aliases are looked through, as if their pattern were written in place; `seen` stops alias
    /// cycles (reported separately).
    fn bad_stop<'p>(&self, p: &'p Pattern, seen: &mut Vec<&'p str>) -> Option<&'p Pattern>
    where
        'q: 'p,
    {
        match &p.kind {
            PatKind::Lit { text, .. } if !text.is_empty() => None,
            PatKind::Prim(prim) if !matches!(prim, Prim::Line | Prim::Row | Prim::Col) => None,
            PatKind::Or(items) | PatKind::Seq(items) => items.iter().find_map(|a| self.bad_stop(a, seen)),
            PatKind::Repeat(r) if r.sep.is_none() && r.skip.is_none() && r.until.is_none() => {
                self.bad_stop(&r.item, seen)
            }
            PatKind::Ref(name) => match self.aliases.get(name.name.as_str()) {
                Some(_) if seen.contains(&name.name.as_str()) => None,
                Some(alias) => {
                    seen.push(&name.name);
                    // Report the use site: the alias itself may be fine elsewhere.
                    self.bad_stop(&alias.pattern, seen).map(|_| p)
                }
                None => Some(p),
            },
            _ => Some(p),
        }
    }

    /// True if `a` and `b` are written the same, reading aliases as their patterns. Identical
    /// references are equal without being expanded, so self-referencing aliases cannot blow up.
    fn same_pattern(&self, a: &Pattern, b: &Pattern, depth: usize) -> bool {
        if depth > 50 {
            return false;
        }
        let alias = |p: &Pattern| match &p.kind {
            PatKind::Ref(n) => self.aliases.get(n.name.as_str()).map(|a| &a.pattern),
            _ => None,
        };
        if let (PatKind::Ref(x), PatKind::Ref(y)) = (&a.kind, &b.kind)
            && x.name == y.name
        {
            return true;
        }
        if let Some(pa) = alias(a) {
            return self.same_pattern(pa, b, depth + 1);
        }
        if let Some(pb) = alias(b) {
            return self.same_pattern(a, pb, depth + 1);
        }
        let all = |xs: &[Pattern], ys: &[Pattern]| {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| self.same_pattern(x, y, depth + 1))
        };
        let opt = |x: &Option<Pattern>, y: &Option<Pattern>| match (x, y) {
            (Some(x), Some(y)) => self.same_pattern(x, y, depth + 1),
            (None, None) => true,
            _ => false,
        };
        match (&a.kind, &b.kind) {
            (PatKind::Lit { text: t1, ci: c1 }, PatKind::Lit { text: t2, ci: c2 }) => t1 == t2 && c1 == c2,
            (PatKind::Prim(p1), PatKind::Prim(p2)) => p1 == p2,
            (PatKind::Label(n1, i1), PatKind::Label(n2, i2)) => {
                n1.name == n2.name && self.same_pattern(i1, i2, depth + 1)
            }
            (PatKind::Seq(x), PatKind::Seq(y)) | (PatKind::Or(x), PatKind::Or(y)) => all(x, y),
            (PatKind::Repeat(r1), PatKind::Repeat(r2)) => {
                (r1.min, r1.max, r1.lazy) == (r2.min, r2.max, r2.lazy)
                    && self.same_pattern(&r1.item, &r2.item, depth + 1)
                    && opt(&r1.sep, &r2.sep)
                    && opt(&r1.skip, &r2.skip)
                    && match (&r1.until, &r2.until) {
                        (Some(u1), Some(u2)) => u1.kind == u2.kind && self.same_pattern(&u1.stop, &u2.stop, depth + 1),
                        (None, None) => true,
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    /// A stop that matches empty text somewhere other than the end would end every repetition
    /// at once. `EOF` is the exception: it is empty, but only matches at the end.
    fn stop_can_be_empty(&self, p: &Pattern, depth: usize) -> bool {
        match &p.kind {
            PatKind::Prim(Prim::Eof) => false,
            PatKind::Ref(name) if depth < 100 => match self.aliases.get(name.name.as_str()) {
                Some(alias) => self.stop_can_be_empty(&alias.pattern, depth + 1),
                None => false,
            },
            PatKind::Seq(items) => items.iter().all(|i| self.stop_can_be_empty(i, depth + 1)),
            PatKind::Or(items) => items.iter().any(|i| self.stop_can_be_empty(i, depth + 1)),
            PatKind::Repeat(r) => r.min == 0 || self.stop_can_be_empty(&r.item, depth + 1),
            _ => self.is_nullable(p),
        }
    }

    /// `ANY` as a whole stop, looking through aliases and labels.
    fn stop_is_any(&self, p: &Pattern, depth: usize) -> bool {
        match &p.kind {
            PatKind::Prim(Prim::Any) => true,
            PatKind::Ref(name) if depth < 100 => {
                self.aliases.get(name.name.as_str()).is_some_and(|a| self.stop_is_any(&a.pattern, depth + 1))
            }
            _ => false,
        }
    }

    /// `true`, `false` and `null` are template constants, so nothing may be named after them: a
    /// rule, label or `FOR` variable so named could never be referenced (loop variables are
    /// checked in `for_each`). Field names after `.` are not names.
    fn reserved_names(&mut self) {
        let q = self.q;
        let mut found = Vec::new();
        for alias in &q.aliases {
            found.push((&alias.name, "alias"));
            labels_in(&alias.pattern, &mut found);
        }
        for rule in &q.rules {
            found.push((&rule.name, "rule"));
            labels_in(&rule.pattern, &mut found);
        }
        for (name, kind) in found {
            if is_constant_name(&name.name) {
                self.errors.push(CheckError::ReservedName { name: name.name.clone(), span: name.span, kind });
            }
        }
    }

    /// Aliases never capture: a label inside one would be a capture hidden from the use site.
    fn no_labels_in_alias(&mut self, p: &Pattern) {
        match &p.kind {
            PatKind::Label(name, _) => {
                self.errors.push(CheckError::LabelInAlias { name: name.name.clone(), span: p.span });
            }
            PatKind::Seq(items) | PatKind::Or(items) => items.iter().for_each(|i| self.no_labels_in_alias(i)),
            PatKind::Repeat(r) => {
                self.no_labels_in_alias(&r.item);
                for p in r.sep.iter().chain(&r.skip).chain(r.until.as_ref().map(|u| &u.stop)) {
                    self.no_labels_in_alias(p);
                }
            }
            _ => {}
        }
    }

    /// An alias is substituted where it is used, so it cannot refer to itself (directly or
    /// through other aliases). Recursion needs a rule.
    fn alias_cycles(&mut self) {
        let mut reported: HashSet<&str> = HashSet::new();
        for alias in &self.q.aliases {
            let start = alias.name.name.as_str();
            if reported.contains(start) {
                continue;
            }
            let alias_refs = |a: &'q Alias| {
                let mut refs = Vec::new();
                all_ref_idents(&a.pattern, &mut refs);
                refs.into_iter().filter(|r| self.aliases.contains_key(r.name.as_str())).collect::<Vec<_>>()
            };
            let mut stack: Vec<(&'q Alias, Vec<&'q Ident>, usize)> = vec![(alias, alias_refs(alias), 0)];
            let mut seen: HashSet<&str> = HashSet::from([start]);
            while let Some((a, refs, idx)) = stack.pop() {
                let Some(&next) = refs.get(idx) else { continue };
                stack.push((a, refs, idx + 1));
                if next.name == start {
                    reported.extend(stack.iter().map(|(a, _, _)| a.name.name.as_str()));
                    self.errors.push(CheckError::AliasCycle { name: start.to_string(), span: next.span });
                    break;
                }
                if seen.insert(&next.name) {
                    let target = self.aliases[next.name.as_str()];
                    stack.push((target, alias_refs(target), 0));
                }
            }
        }
    }

    fn compute_nullable(&mut self) {
        for rule in &self.q.rules {
            self.nullable.insert(&rule.name.name, false);
        }
        loop {
            let mut changed = false;
            for rule in &self.q.rules {
                let n = self.is_nullable(&rule.pattern);
                if n && !self.nullable[rule.name.name.as_str()] {
                    self.nullable.insert(&rule.name.name, true);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    fn is_nullable(&self, p: &Pattern) -> bool {
        match &p.kind {
            PatKind::Lit { .. } => false,
            // A line can be empty, and ROW, COL and EOF match empty text.
            PatKind::Prim(prim) => matches!(prim, Prim::Line | Prim::Row | Prim::Col | Prim::Eof),
            PatKind::Ref(name) => match self.aliases.get(name.name.as_str()) {
                Some(alias) => self.is_nullable(&alias.pattern),
                None => self.nullable.get(name.name.as_str()).copied().unwrap_or(false),
            },
            PatKind::Label(_, inner) => self.is_nullable(inner),
            PatKind::Seq(items) => items.iter().all(|i| self.is_nullable(i)),
            PatKind::Or(alts) => alts.iter().any(|a| self.is_nullable(a)),
            // `UNTIL` consumes its stop, which is only empty at the end of the text (`EOF`).
            PatKind::Repeat(r) if let Some(u) = r.until.as_ref().filter(|u| u.kind == Stop::After) => {
                (r.min == 0 || self.is_nullable(&r.item)) && self.is_nullable(&u.stop)
            }
            PatKind::Repeat(r) => r.min == 0 || self.is_nullable(&r.item),
        }
    }

    fn loops(&mut self, p: &Pattern) {
        match &p.kind {
            PatKind::Label(_, inner) => self.loops(inner),
            PatKind::Seq(items) | PatKind::Or(items) => {
                for i in items {
                    self.loops(i);
                }
            }
            PatKind::Repeat(r) => {
                // A separator that consumes text keeps iterations apart, even if items are empty.
                let sep_consumes = r.sep.as_ref().is_some_and(|s| !self.is_nullable(s));
                if r.max != Some(0) && !sep_consumes && self.is_nullable(&r.item) {
                    self.errors.push(CheckError::EmptyLoop { span: r.item.span });
                }
                if let Some(s) = &r.skip
                    && self.is_nullable(s)
                {
                    self.errors.push(CheckError::EmptyLoop { span: s.span });
                }
                self.loops(&r.item);
                if let Some(s) = &r.sep {
                    self.loops(s);
                }
            }
            _ => {}
        }
    }

    /// References that can make up a whole match on their own (everything else empty).
    fn unit_refs(&self, p: &'q Pattern, out: &mut Vec<(&'q str, Span)>) {
        match &p.kind {
            PatKind::Ref(name) => match self.aliases.get(name.name.as_str()) {
                Some(alias) => self.unit_refs(&alias.pattern, out),
                None => out.push((&name.name, name.span)),
            },
            PatKind::Label(_, inner) => self.unit_refs(inner, out),
            PatKind::Or(alts) => {
                for a in alts {
                    self.unit_refs(a, out);
                }
            }
            PatKind::Seq(items) => {
                for (k, item) in items.iter().enumerate() {
                    let others_nullable = items.iter().enumerate().all(|(j, o)| j == k || self.is_nullable(o));
                    if others_nullable {
                        self.unit_refs(item, out);
                    }
                }
            }
            PatKind::Repeat(r) if r.max != Some(0) => self.unit_refs(&r.item, out),
            _ => {}
        }
    }

    fn empty_cycles(&mut self) {
        let mut edges: HashMap<&str, Vec<(&str, Span)>> = HashMap::new();
        for rule in &self.q.rules {
            let mut out = Vec::new();
            self.unit_refs(&rule.pattern, &mut out);
            edges.insert(&rule.name.name, out);
        }
        // Report each cycle once, at the reference that closes it.
        let mut reported: HashSet<&str> = HashSet::new();
        for rule in &self.q.rules {
            let start = rule.name.name.as_str();
            if reported.contains(start) {
                continue;
            }
            // Iterative DFS looking for a path back to `start`.
            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            let mut seen: HashSet<&str> = HashSet::from([start]);
            while let Some((node, idx)) = stack.pop() {
                let Some(&(next, span)) = edges[node].get(idx) else { continue };
                stack.push((node, idx + 1));
                if next == start {
                    let members: Vec<&str> = stack.iter().map(|(n, _)| *n).collect();
                    reported.extend(members);
                    self.errors.push(CheckError::EmptyCycle { name: start.to_string(), span });
                    break;
                }
                if seen.insert(next) {
                    stack.push((next, 0));
                }
            }
        }
    }

    // ---- lints ----

    fn lints(&mut self) {
        // Unused rules.
        let mut reachable: HashSet<&str> = HashSet::new();
        let root = self.q.root().map(|r| r.name.name.as_str());
        let mut todo: Vec<&str> = root.into_iter().collect();
        while let Some(r) = todo.pop() {
            if !reachable.insert(r) {
                continue;
            }
            let pattern = match (self.rules.get(r), self.aliases.get(r)) {
                (Some(rule), _) => &rule.pattern,
                (None, Some(alias)) => &alias.pattern,
                (None, None) => continue,
            };
            let mut refs = Vec::new();
            all_refs(pattern, &mut refs);
            todo.extend(refs);
        }
        if root.is_some() {
            for rule in &self.q.rules {
                if !reachable.contains(rule.name.name.as_str()) {
                    self.errors.push(CheckError::UnusedRule { name: rule.name.name.clone(), span: rule.name.span });
                }
            }
            for alias in &self.q.aliases {
                if !reachable.contains(alias.name.name.as_str()) {
                    self.errors.push(CheckError::UnusedAlias { name: alias.name.name.clone(), span: alias.name.span });
                }
            }
        }
        // Direct right recursion: `a = x a`.
        for rule in &self.q.rules {
            let alts: Vec<&Pattern> = match &rule.pattern.kind {
                PatKind::Or(alts) => alts.iter().collect(),
                _ => vec![&rule.pattern],
            };
            for alt in alts {
                if let PatKind::Seq(items) = &alt.kind
                    && let Some(last) = items.last()
                    && let PatKind::Ref(name) = &strip_label(last).kind
                    && name.name == rule.name.name
                {
                    self.errors.push(CheckError::RightRecursion { name: name.name.clone(), span: last.span });
                    break;
                }
            }
        }
    }

    // ---- captures and shapes ----

    /// Captures visible in the scope of `p`. Mirrors the runtime rules in `eval`.
    fn captures(&mut self, p: &'q Pattern, report: bool) -> Vec<Capture> {
        match &p.kind {
            PatKind::Lit { .. } | PatKind::Prim(_) => Vec::new(),
            // An alias never captures.
            PatKind::Ref(name) if self.aliases.contains_key(name.name.as_str()) => Vec::new(),
            PatKind::Ref(name) => {
                let shape = self.rule_shape(&name.name);
                vec![Capture { name: name.name.clone(), shape, span: name.span }]
            }
            PatKind::Label(name, inner) => {
                let shape = self.label_shape(inner);
                vec![Capture { name: name.name.clone(), shape, span: p.span }]
            }
            PatKind::Seq(items) => {
                let mut out: Vec<Capture> = Vec::new();
                for item in items {
                    for c in self.captures(item, report) {
                        if let Some(first) = out.iter().find(|o| o.name == c.name) {
                            if report {
                                self.errors.push(CheckError::DuplicateCapture {
                                    name: c.name.clone(),
                                    first: first.span,
                                    second: c.span,
                                });
                            }
                        } else {
                            out.push(c);
                        }
                    }
                }
                out
            }
            PatKind::Or(alts) => {
                let mut out: Vec<Capture> = Vec::new();
                for alt in alts {
                    for c in self.captures(alt, report) {
                        match out.iter_mut().find(|o| o.name == c.name) {
                            Some(existing) => {
                                existing.shape = std::mem::replace(&mut existing.shape, Shape::Unknown).unify(c.shape)
                            }
                            None => out.push(c),
                        }
                    }
                }
                out
            }
            PatKind::Repeat(r) => self
                .captures(&r.item, report)
                .into_iter()
                .map(|c| if single_item(r) { c } else { Capture { shape: Shape::Array(Box::new(c.shape)), ..c } })
                .collect(),
        }
    }

    /// Shape of the value captured by a single step (see `eval::child_value`).
    fn step_shape(&mut self, p: &'q Pattern) -> Shape {
        match &p.kind {
            PatKind::Prim(Prim::Row | Prim::Col) => Shape::Num,
            PatKind::Lit { .. } | PatKind::Prim(_) => Shape::Text,
            // An alias captures nothing, so its value is the text it matched (see `eval::default_value`).
            PatKind::Ref(name) if self.aliases.contains_key(name.name.as_str()) => Shape::Text,
            PatKind::Ref(name) => self.rule_shape(&name.name),
            PatKind::Label(_, inner) => self.label_shape(inner),
            PatKind::Repeat(r) => self.repeat_shape(r),
            PatKind::Or(alts) => {
                let shapes: Vec<Shape> = alts.iter().map(|a| self.alt_shape(a)).collect();
                shapes.into_iter().reduce(Shape::unify).unwrap_or(Shape::Unknown)
            }
            PatKind::Seq(_) => self.alt_shape(p),
        }
    }

    /// Default value of a repetition: a list of its items, or one item for `0 TO 1`.
    fn repeat_shape(&mut self, r: &'q Repeat) -> Shape {
        let item = self.item_shape(&r.item);
        if single_item(r) { item } else { Shape::Array(Box::new(item)) }
    }

    /// A repetition item or label target that may have been wrapped in a group.
    fn item_shape(&mut self, p: &'q Pattern) -> Shape {
        if single_step(p) { self.step_shape(p) } else { self.alt_shape(p) }
    }

    fn label_shape(&mut self, inner: &'q Pattern) -> Shape {
        match inner.kind {
            // A label on a repetition captures the text it matched; for a list, label the item.
            PatKind::Repeat(_) => Shape::Text,
            // A label on an explicitly labelled pattern wraps it in a group.
            PatKind::Label(..) => self.alt_shape(inner),
            _ => self.item_shape(inner),
        }
    }

    /// Default value of one alternative of a rule or group (see `eval::default_value`).
    fn alt_shape(&mut self, p: &'q Pattern) -> Shape {
        match &p.kind {
            PatKind::Repeat(r) => return self.repeat_shape(r),
            PatKind::Ref(name) if self.aliases.contains_key(name.name.as_str()) => {
                return Shape::Text;
            }
            PatKind::Ref(name) => return self.rule_shape(&name.name),
            _ => {}
        }
        let caps = self.captures(p, false);
        if !caps.is_empty() {
            return Shape::Object(Some(caps.into_iter().map(|c| (c.name, c.shape)).collect()));
        }
        if matches!(p.kind, PatKind::Or(_)) {
            return self.step_shape(p);
        }
        Shape::Text
    }

    fn rule_shape(&mut self, name: &'q str) -> Shape {
        if let Some(s) = self.shapes.get(name) {
            return s.clone();
        }
        let Some(rule) = self.rules.get(name).copied() else { return Shape::Unknown };
        if !self.shape_in_progress.insert(name) {
            return Shape::Unknown;
        }
        let shape = match &rule.template {
            Some(t) => {
                let caps = self.captures(&rule.pattern, false);
                let env = Env { vars: caps.into_iter().map(|c| (c.name, c.shape)).collect(), open: true };
                self.tmpl_shape(t, &env)
            }
            None => match &rule.pattern.kind {
                PatKind::Or(alts) => {
                    let shapes: Vec<Shape> = alts.iter().map(|a| self.alt_shape(a)).collect();
                    shapes.into_iter().reduce(Shape::unify).unwrap_or(Shape::Unknown)
                }
                _ => self.alt_shape(&rule.pattern),
            },
        };
        self.shape_in_progress.remove(name);
        self.shapes.insert(name, shape.clone());
        shape
    }

    /// Shape of a template without reporting errors.
    fn tmpl_shape(&mut self, t: &Template, env: &Env) -> Shape {
        let saved = self.errors.len();
        let s = self.tmpl(t, env);
        self.errors.truncate(saved);
        s
    }

    // ---- templates and conditions ----

    fn cond(&mut self, c: &Cond, env: &Env) {
        match &c.kind {
            CondKind::Or(a, b) | CondKind::And(a, b) => {
                self.cond(a, env);
                self.cond(b, env);
            }
            CondKind::Not(a) => self.cond(a, env),
            CondKind::Cmp(a, _, b) => {
                self.tmpl(a, env);
                self.tmpl(b, env);
            }
            CondKind::Truthy(a) => {
                self.tmpl(a, env);
            }
        }
    }

    fn unknown_name(&mut self, name: &Ident, env: &Env, as_key: bool) {
        let mut names: Vec<&str> = Vec::new();
        for (n, _) in &env.vars {
            if !names.contains(&n.as_str()) {
                names.push(n);
            }
        }
        let mut help = match suggest(&name.name, names.iter().copied()) {
            Some(s) => format!("did you mean `{s}`?"),
            None if names.is_empty() => {
                "this rule captures nothing; label parts of the pattern, e.g. `name:WORD`".to_string()
            }
            None => format!("available names: {}", names.join(", ")),
        };
        if as_key {
            help.push_str(&format!("\nto use `{0}` as a fixed key, quote it: '{0}'", name.name));
        }
        self.errors.push(CheckError::UnknownCapture { name: name.name.clone(), span: name.span, help: Some(help) });
    }

    fn tmpl(&mut self, t: &Template, env: &Env) -> Shape {
        self.tmpl_at(t, env, false)
    }

    /// `as_key` is set for object keys, to suggest quoting unknown names.
    fn tmpl_at(&mut self, t: &Template, env: &Env, as_key: bool) -> Shape {
        match &t.kind {
            TmplKind::Str(_) => Shape::Text,
            TmplKind::Num(_) => Shape::Num,
            TmplKind::Bool(_) => Shape::Bool,
            TmplKind::Null => Shape::Null,
            TmplKind::Path(parts) => {
                let first = &parts[0];
                let mut shape = match env.get(&first.name) {
                    Some(s) => s.clone(),
                    None if env.open => Shape::Unknown,
                    None => {
                        self.unknown_name(first, env, as_key);
                        return Shape::Unknown;
                    }
                };
                for (k, field) in parts.iter().enumerate().skip(1) {
                    shape = match shape {
                        Shape::Object(Some(fields)) => match fields.iter().find(|(n, _)| *n == field.name) {
                            Some((_, s)) => s.clone(),
                            None => {
                                let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
                                let help = match suggest(&field.name, names.iter().copied()) {
                                    Some(s) => format!("did you mean `{s}`?"),
                                    None => format!("available fields: {}", names.join(", ")),
                                };
                                let of = parts[..k].iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
                                self.errors.push(CheckError::UnknownField {
                                    field: field.name.clone(),
                                    of,
                                    span: field.span,
                                    help: Some(help),
                                });
                                return Shape::Unknown;
                            }
                        },
                        _ => Shape::Unknown,
                    };
                }
                shape
            }
            TmplKind::Call(name, args) => {
                let arg_shapes: Vec<Shape> = args.iter().map(|a| self.tmpl(a, env)).collect();
                let Some(&(_, lo, hi)) = FUNCTIONS.iter().find(|(n, _, _)| *n == name.name) else {
                    let help =
                        suggest(&name.name, FUNCTIONS.iter().map(|f| f.0)).map(|s| format!("did you mean `{s}`?"));
                    self.errors.push(CheckError::UnknownFunction { name: name.name.clone(), span: name.span, help });
                    return Shape::Unknown;
                };
                if args.len() < lo || args.len() > hi {
                    let expected = if lo == hi { lo.to_string() } else { format!("{lo} or {hi}") };
                    // JOIN used to default its separator; show the call written out with one.
                    let help = (name.name == "JOIN" && args.len() == 1).then(|| {
                        format!(
                            "JOIN needs a separator: `JOIN({}, ' ')`",
                            &self.src[args[0].span.start..args[0].span.end]
                        )
                    });
                    self.errors.push(CheckError::Arity {
                        name: name.name.clone(),
                        expected,
                        got: args.len(),
                        span: t.span,
                        help,
                    });
                    return Shape::Unknown;
                }
                match name.name.as_str() {
                    "NUM" | "COUNT" => Shape::Num,
                    "LOWER" | "UPPER" | "TRIM" | "JOIN" => Shape::Text,
                    "ZIP" => Shape::Object(None),
                    "FIRST" | "LAST" => match &arg_shapes[0] {
                        Shape::Array(inner) => (**inner).clone(),
                        _ => Shape::Unknown,
                    },
                    _ => Shape::Unknown,
                }
            }
            TmplKind::Object(entries) => {
                let mut fields = Some(Vec::new());
                for e in entries {
                    match e {
                        ObjEntry::Pair { key, value, each, listof } => {
                            let inner_env = match each {
                                Some(f) => self.for_each(f, env),
                                None => env.clone(),
                            };
                            let key_shape = self.tmpl_at(key, &inner_env, true);
                            let mut value_shape = self.tmpl(value, &inner_env);
                            if *listof {
                                value_shape = Shape::Array(Box::new(value_shape));
                            }
                            match (&mut fields, &key.kind, each) {
                                (Some(f), TmplKind::Str(k), None) => f.push((k.clone(), value_shape)),
                                _ => fields = None,
                            }
                            let _ = key_shape;
                        }
                        ObjEntry::Merge(src) => {
                            let shape = self.tmpl(src, env);
                            let item = match &shape {
                                Shape::Array(inner) => inner,
                                other => other,
                            };
                            if matches!(item, Shape::Text | Shape::Num | Shape::Bool) {
                                self.errors.push(CheckError::NotMergeable {
                                    name: crate::printer::print_tmpl(src),
                                    shape: shape.describe().into(),
                                    span: src.span,
                                    help: crate::eval::merge_help(src),
                                });
                            }
                            fields = None;
                        }
                    }
                }
                Shape::Object(fields)
            }
            TmplKind::Array(elems) => {
                let mut shape: Option<Shape> = None;
                for e in elems {
                    let inner_env = match &e.each {
                        Some(f) => self.for_each(f, env),
                        None => env.clone(),
                    };
                    let s = self.tmpl(&e.value, &inner_env);
                    shape = Some(match shape {
                        None => s,
                        Some(prev) => prev.unify(s),
                    });
                }
                Shape::Array(Box::new(shape.unwrap_or(Shape::Unknown)))
            }
        }
    }

    /// Checks a `FOR` clause and returns the environment for its body.
    fn for_each(&mut self, f: &ForEach, env: &Env) -> Env {
        if is_constant_name(&f.var.name) {
            self.errors.push(CheckError::ReservedName {
                name: f.var.name.clone(),
                span: f.var.span,
                kind: "loop variable",
            });
        }
        let source = match &f.source {
            Some(src) => self.tmpl(src, env),
            None => match env.get(&f.var.name) {
                Some(s) => s.clone(),
                None if env.open => Shape::Unknown,
                None => {
                    self.unknown_name(&f.var, env, false);
                    Shape::Unknown
                }
            },
        };
        let elem = match source {
            Shape::Array(inner) => *inner,
            Shape::Unknown | Shape::Null => Shape::Unknown,
            other => {
                let (name, span) = match &f.source {
                    Some(src) => (crate::printer::print_tmpl(src), src.span),
                    None => (f.var.name.clone(), f.var.span),
                };
                self.errors.push(CheckError::NotRepeated { name, shape: other.describe().into(), span });
                Shape::Unknown
            }
        };
        let mut inner = env.clone();
        match &elem {
            Shape::Object(Some(fields)) => inner.vars.extend(fields.iter().cloned()),
            Shape::Object(None) | Shape::Unknown => inner.open = true,
            _ => {}
        }
        inner.vars.push((f.var.name.clone(), elem));
        inner
    }
}

/// At most one item (`0 TO 1`): captures are values, not lists.
fn single_item(r: &Repeat) -> bool {
    r.max.is_some_and(|m| m <= 1)
}

fn strip_label(p: &Pattern) -> &Pattern {
    match &p.kind {
        PatKind::Label(_, inner) => strip_label(inner),
        _ => p,
    }
}

/// True if `p` compiles to exactly one IR step.
pub fn single_step(p: &Pattern) -> bool {
    !matches!(p.kind, PatKind::Seq(_))
}

fn all_ref_idents<'q>(p: &'q Pattern, out: &mut Vec<&'q Ident>) {
    match &p.kind {
        PatKind::Ref(n) => out.push(n),
        PatKind::Label(_, inner) => all_ref_idents(inner, out),
        PatKind::Seq(items) | PatKind::Or(items) => items.iter().for_each(|i| all_ref_idents(i, out)),
        PatKind::Repeat(r) => {
            all_ref_idents(&r.item, out);
            for p in r.sep.iter().chain(&r.skip).chain(r.until.as_ref().map(|u| &u.stop)) {
                all_ref_idents(p, out);
            }
        }
        _ => {}
    }
}

fn all_refs<'q>(p: &'q Pattern, out: &mut Vec<&'q str>) {
    match &p.kind {
        PatKind::Ref(n) => out.push(&n.name),
        PatKind::Label(_, inner) => all_refs(inner, out),
        PatKind::Seq(items) | PatKind::Or(items) => items.iter().for_each(|i| all_refs(i, out)),
        PatKind::Repeat(r) => {
            all_refs(&r.item, out);
            for p in r.sep.iter().chain(&r.skip).chain(r.until.as_ref().map(|u| &u.stop)) {
                all_refs(p, out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_basics() {
        assert_eq!(distance("colors", "colours"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("same", "same"), 0);
    }

    #[test]
    fn suggestions() {
        assert_eq!(suggest("colrs", ["colors", "rhyme"]), Some("colors".into()));
        assert_eq!(suggest("xyz", ["colors", "rhyme"]), None);
    }
}
