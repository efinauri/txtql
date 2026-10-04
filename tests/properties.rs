//! Property tests:
//!
//! - **differential**: the Earley recogniser agrees with a naive reference matcher written
//!   directly against the syntax tree, on random grammars and inputs;
//! - **generation**: text generated from a random grammar is always accepted and extracted;
//! - **round trip**: printing a random syntax tree and parsing it back is the identity;
//! - **robustness**: arbitrary queries and inputs never panic;
//! - **invariants** of the input model and of the output.

mod common;
use proptest::prelude::*;
use std::collections::BTreeSet;
use txtql::ast::*;
use txtql::error::Span;
use txtql::input::{Input, is_mark};
use txtql::{Options, Query, RunError};

// ---- a tiny deterministic RNG, seeded by proptest ----

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u64) as usize]
    }

    fn pick_ref<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }

    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }
}

fn rng(seed: u64) -> Rng {
    Rng(seed | 1)
}

// ---- random grammars ----

const LITERALS: &[&str] = &["a", "b", "A", ",", "1", " ", "a b", "x-y", ";", ", ", "\n", "\r"];
const LABELS: &[&str] = &["p", "q", "s", "t"];

fn ident(name: &str) -> Ident {
    Ident { name: name.to_string(), span: Span::default() }
}

fn pat(kind: PatKind) -> Pattern {
    Pattern { kind, span: Span::default() }
}

fn rule_name(i: usize) -> String {
    if i == 0 { "TEXT".into() } else { format!("r{i}") }
}

fn lit(r: &mut Rng) -> Pattern {
    pat(PatKind::Lit { text: r.pick(LITERALS).to_string(), ci: r.chance(8) })
}

fn leaf(r: &mut Rng) -> Pattern {
    match r.below(15) {
        0..=3 => lit(r),
        4 => pat(PatKind::Prim(Prim::Word)),
        5 => pat(PatKind::Prim(r.pick(&[Prim::Float, Prim::Int, Prim::Hex, Prim::Bin, Prim::Ipv4, Prim::Ipv6]))),
        6 => pat(PatKind::Prim(Prim::Punct)),
        7 => pat(PatKind::Prim(Prim::Any)),
        8 => pat(PatKind::Prim(r.pick(&[Prim::Newline, Prim::Tab]))),
        9 => pat(PatKind::Prim(Prim::Line)),
        10 => until_any(r, pat(PatKind::Prim(Prim::Newline))),
        11 => pat(PatKind::Prim(Prim::Digit)),
        12 => pat(PatKind::Prim(Prim::Letter)),
        _ => {
            let inner = if r.chance(4) {
                pat(PatKind::Or(vec![lit(r), pat(PatKind::Prim(Prim::Newline))]))
            } else if r.chance(2) {
                lit(r)
            } else {
                pat(PatKind::Prim(r.pick(&[
                    Prim::Word,
                    Prim::Float,
                    Prim::Int,
                    Prim::Hex,
                    Prim::Bin,
                    Prim::Ipv4,
                    Prim::Ipv6,
                    Prim::Punct,
                    Prim::Digit,
                    Prim::Letter,
                ])))
            };
            until_any(r, inner)
        }
    }
}

/// `ANY UNTILBEFORE stop` or `ANY UNTIL stop`.
fn until_any(r: &mut Rng, stop: Pattern) -> Pattern {
    let kind = if r.chance(2) { Stop::Before } else { Stop::After };
    let item = pat(PatKind::Prim(Prim::Any));
    let rep = Repeat { min: 0, max: None, lazy: false, item, sep: None, skip: None, until: Some(Until { stop, kind }) };
    pat(PatKind::Repeat(Box::new(rep)))
}

/// A stop for `UNTIL` / `UNTILBEFORE`: literals and primitives, with sequences, `OR` and
/// repetitions.
fn random_stop(r: &mut Rng) -> Pattern {
    match r.below(5) {
        0 => lit(r),
        1 => pat(PatKind::Prim(r.pick(&[Prim::Newline, Prim::Punct, Prim::Digit, Prim::Word]))),
        2 => pat(PatKind::Seq(vec![pat(PatKind::Prim(Prim::Newline)), lit(r)])),
        3 => {
            let item = lit(r);
            pat(PatKind::Repeat(Box::new(Repeat {
                min: 1,
                max: None,
                lazy: false,
                item,
                sep: None,
                skip: None,
                until: None,
            })))
        }
        _ => pat(PatKind::Or(vec![lit(r), pat(PatKind::Prim(Prim::Newline))])),
    }
}

/// A random pattern for rule `ri`; references only go to later rules, so grammars are not
/// recursive and the reference matcher terminates.
fn random_pattern(r: &mut Rng, depth: u32, ri: usize, nrules: usize) -> Pattern {
    if depth == 0 {
        return leaf(r);
    }
    match r.below(10) {
        0..=2 => leaf(r),
        3 if ri + 1 < nrules => {
            let target = ri + 1 + r.below((nrules - ri - 1) as u64) as usize;
            pat(PatKind::Ref(ident(&rule_name(target))))
        }
        3 | 4 => {
            let n = 2 + r.below(2) as usize;
            pat(PatKind::Seq((0..n).map(|_| random_pattern(r, depth - 1, ri, nrules)).collect()))
        }
        5 => {
            let n = 2 + r.below(2) as usize;
            pat(PatKind::Or((0..n).map(|_| random_pattern(r, depth - 1, ri, nrules)).collect()))
        }
        6 => pat(PatKind::Label(ident(r.pick(LABELS)), Box::new(random_pattern(r, depth - 1, ri, nrules)))),
        7 => pat(PatKind::Ref(ident(r.pick(ALIASES)))),
        _ => {
            let min = r.below(3) as u32;
            let max = if r.chance(2) { None } else { Some(min + r.below(3) as u32) };
            let item = random_pattern(r, depth - 1, ri, nrules);
            let sep = r.chance(3).then(|| pat(PatKind::Lit { text: r.pick(&[",", " ", "\n"]).to_string(), ci: false }));
            let skip = r.chance(5).then(|| pat(PatKind::Prim(r.pick(&[Prim::Punct, Prim::Any]))));
            let until = r
                .chance(4)
                .then(|| Until { stop: random_stop(r), kind: if r.chance(2) { Stop::Before } else { Stop::After } });
            pat(PatKind::Repeat(Box::new(Repeat { min, max, lazy: r.chance(4), item, sep, skip, until })))
        }
    }
}

const ALIASES: &[&str] = &["a0", "a1"];

/// Aliases cannot contain labels; references are replaced too, so aliases stay acyclic.
fn strip_for_alias(p: Pattern) -> Pattern {
    let kind = match p.kind {
        PatKind::Label(_, inner) => return strip_for_alias(*inner),
        PatKind::Ref(_) => PatKind::Prim(Prim::Word),
        PatKind::Seq(items) => PatKind::Seq(items.into_iter().map(strip_for_alias).collect()),
        PatKind::Or(items) => PatKind::Or(items.into_iter().map(strip_for_alias).collect()),
        PatKind::Repeat(rep) => {
            let Repeat { min, max, lazy, item, sep, skip, until } = *rep;
            PatKind::Repeat(Box::new(Repeat {
                min,
                max,
                lazy,
                item: strip_for_alias(item),
                sep: sep.map(strip_for_alias),
                skip: skip.map(strip_for_alias),
                until,
            }))
        }
        other => other,
    };
    Pattern { kind, span: p.span }
}

fn random_ast(r: &mut Rng) -> txtql::ast::Query {
    let aliases = ALIASES
        .iter()
        .map(|name| Alias { name: ident(name), pattern: strip_for_alias(random_pattern(r, 2, 0, 1)) })
        .collect();
    let nrules = 1 + r.below(4) as usize;
    let rules = (0..nrules)
        .map(|i| Rule {
            name: ident(&rule_name(i)),
            strict: false,
            pattern: random_pattern(r, 3, i, nrules),
            filter: None,
            template: None,
        })
        .collect();
    txtql::ast::Query { aliases, rules, span: Span::default() }
}

/// The pattern a rule or alias name stands for.
fn definition<'q>(q: &'q txtql::ast::Query, name: &str) -> &'q Pattern {
    match q.alias(name) {
        Some(a) => &a.pattern,
        None => &q.rule(name).unwrap().pattern,
    }
}

/// A random grammar that passes the static checks, as query text.
fn random_query(r: &mut Rng) -> (String, Query) {
    loop {
        let text = txtql::printer::print_query(&random_ast(r));
        if let Ok(q) = Query::compile(&text) {
            return (text, q);
        }
    }
}

const INPUT_PIECES: &[&str] =
    &["a", "b", "x", "ab", "A", "Ab", "-", "y", ",", ";", " ", " ", "\n", "\r\n", "\r", "1", "7", "42", "é"];

fn random_input(r: &mut Rng) -> String {
    (0..r.below(10)).map(|_| r.pick(INPUT_PIECES)).collect()
}

// ---- the reference matcher ----

/// Straightforward set-of-end-positions matcher over the syntax tree. Independent of the IR,
/// the Earley parser and the extractor; it finds word and number runs by itself and shares only
/// the definition of combining marks.
struct Reference<'a> {
    q: &'a txtql::ast::Query,
    c: Vec<char>,
    /// End of the word or number run starting at each position, if one starts there.
    word: Vec<Option<usize>>,
    number: Vec<Option<usize>>,
}

type Ends = BTreeSet<usize>;

impl<'a> Reference<'a> {
    fn new(q: &'a txtql::ast::Query, text: &str) -> Self {
        let c: Vec<char> = text.chars().collect();
        let n = c.len();
        let (mut word, mut number) = (vec![None; n], vec![None; n]);
        let mut i = 0;
        while i < n {
            if c[i].is_alphabetic() {
                let mut j = i + 1;
                while j < n && (c[j].is_alphabetic() || is_mark(c[j])) {
                    j += 1;
                }
                word[i] = Some(j);
                i = j;
            } else if c[i].is_ascii_digit() {
                let mut j = i;
                while j < n && c[j].is_ascii_digit() {
                    j += 1;
                }
                if j + 1 < n && c[j] == '.' && c[j + 1].is_ascii_digit() {
                    j += 1;
                    while j < n && c[j].is_ascii_digit() {
                        j += 1;
                    }
                }
                number[i] = Some(j);
                i = j;
            } else {
                i += 1;
            }
        }
        Reference { q, c, word, number }
    }

    fn is_nl(&self, i: usize) -> bool {
        matches!(self.c.get(i), Some('\n' | '\r'))
    }

    fn line_start(&self, i: usize) -> bool {
        i == 0 || self.c[i - 1] == '\n' || (self.c[i - 1] == '\r' && self.c.get(i) != Some(&'\n'))
    }

    /// First line break (or the end) at or after `i`.
    fn line_end(&self, mut i: usize) -> usize {
        while i < self.c.len() && !self.is_nl(i) {
            i += 1;
        }
        i
    }

    /// A maximal run of `digit` characters at `i`; with `alnum_after`, not followed by any
    /// letter or digit.
    fn run(&self, i: usize, digit: impl Fn(char) -> bool, alnum_after: bool) -> Option<usize> {
        let n = self.c.len();
        if i >= n || !digit(self.c[i]) || (i > 0 && digit(self.c[i - 1])) {
            return None;
        }
        let mut j = i;
        while j < n && digit(self.c[j]) {
            j += 1;
        }
        let blocked = j < n && if alnum_after { self.c[j].is_alphanumeric() } else { digit(self.c[j]) };
        (!blocked).then_some(j)
    }

    /// The longest IPv6 address at `i`, checked with the standard library (after replacing an
    /// IPv4 tail, which may have leading zeros here, by two groups).
    fn ipv6(&self, i: usize) -> Option<usize> {
        let addr_char = |c: char| c.is_ascii_hexdigit() || c == ':' || c == '.';
        if i > 0 && (self.c[i - 1].is_alphanumeric() || matches!(self.c[i - 1], ':' | '.')) {
            return None;
        }
        let mut span = i;
        while span < self.c.len() && span - i < 45 && addr_char(self.c[span]) {
            span += 1;
        }
        let valid = |text: String| -> bool {
            let Some((head, last)) = text.rsplit_once(':') else { return false };
            let text = if last.contains('.') {
                let parts: Vec<&str> = last.split('.').collect();
                let v4 = parts.len() == 4
                    && parts.iter().all(|p| {
                        !p.is_empty()
                            && p.len() <= 3
                            && p.bytes().all(|b| b.is_ascii_digit())
                            && p.parse::<u32>().unwrap() <= 255
                    });
                if !v4 {
                    return false;
                }
                format!("{head}:0:0")
            } else {
                text.clone()
            };
            text.parse::<std::net::Ipv6Addr>().is_ok()
        };
        (i + 2..=span).rev().find(|&end| {
            let next = self.c.get(end).copied();
            let after = self.c.get(end + 1).copied();
            let cut = match next {
                None => false,
                Some(':') => after.is_some_and(|d| d.is_ascii_hexdigit() || d == ':'),
                Some('.') => after.is_some_and(|d| d.is_ascii_hexdigit()),
                Some(c) => c.is_alphanumeric(),
            };
            !cut && valid(self.c[i..end].iter().collect())
        })
    }

    fn ipv4(&self, i: usize) -> Option<usize> {
        let mut j = i;
        for part in 0..4 {
            if part > 0 {
                if self.c.get(j) != Some(&'.') {
                    return None;
                }
                j += 1;
            }
            let end = self.run(j, |c| c.is_ascii_digit(), false)?;
            let value: String = self.c[j..end].iter().collect();
            if end - j > 3 || value.parse::<u32>().ok()? > 255 {
                return None;
            }
            j = end;
        }
        let continues = self.c.get(j) == Some(&'.') && self.c.get(j + 1).is_some_and(|c| c.is_ascii_digit());
        let preceded = i > 0 && self.c[i - 1] == '.';
        (!continues && !preceded).then_some(j)
    }

    fn newline(&self, i: usize) -> Option<usize> {
        match self.c.get(i)? {
            '\r' if self.c.get(i + 1) == Some(&'\n') => Some(i + 2),
            '\r' | '\n' => Some(i + 1),
            _ => None,
        }
    }

    fn literal(&self, text: &str, ci: bool, i: usize) -> Option<usize> {
        let lit: Vec<char> = text.chars().collect();
        let here = self.c.get(i..i + lit.len())?;
        let same = here.iter().zip(&lit).all(|(a, b)| a == b || (ci && a.to_lowercase().eq(b.to_lowercase())));
        same.then_some(i + lit.len())
    }

    /// True if the stop of an `UNTIL` / `UNTILBEFORE` matches at `i`.
    fn stop_at(&self, r: &Repeat, i: usize) -> bool {
        r.until.as_ref().is_some_and(|u| !self.ends(&u.stop, i).is_empty())
    }

    fn all(&self, p: &Pattern, from: &Ends) -> Ends {
        from.iter().flat_map(|&i| self.ends(p, i)).collect()
    }

    /// Positions reachable by zero or more repetitions of `p`.
    fn closure(&self, p: &Pattern, from: Ends) -> Ends {
        let mut seen = from.clone();
        let mut todo: Vec<usize> = from.into_iter().collect();
        while let Some(i) = todo.pop() {
            for j in self.ends(p, i) {
                if seen.insert(j) {
                    todo.push(j);
                }
            }
        }
        seen
    }

    fn ends(&self, p: &Pattern, i: usize) -> Ends {
        let n = self.c.len();
        let at = self.c.get(i).copied();
        let to = |end: Option<usize>| end.into_iter().collect::<Ends>();
        match &p.kind {
            PatKind::Lit { text, ci } => to(self.literal(text, *ci, i)),
            PatKind::Prim(Prim::Word) => to(self.word.get(i).copied().flatten()),
            PatKind::Prim(Prim::Float) => to(self.number.get(i).copied().flatten()),
            PatKind::Prim(Prim::Int) => to(self.run(i, |c| c.is_ascii_digit(), false)),
            PatKind::Prim(Prim::Hex) => to(self.run(i, |c| c.is_ascii_hexdigit(), true)),
            PatKind::Prim(Prim::Bin) => to(self.run(i, |c| c == '0' || c == '1', true)),
            PatKind::Prim(Prim::Ipv4) => to(self.ipv4(i)),
            PatKind::Prim(Prim::Ipv6) => to(self.ipv6(i)),
            PatKind::Prim(Prim::Newline) => to(self.newline(i)),
            PatKind::Prim(Prim::Tab) => to((at == Some('\t')).then_some(i + 1)),
            PatKind::Prim(Prim::Punct) => to(at.filter(|c| !c.is_alphanumeric() && !c.is_whitespace()).map(|_| i + 1)),
            PatKind::Prim(Prim::Any) => to((i < n).then_some(i + 1)),
            PatKind::Prim(Prim::Digit) => to(at.filter(char::is_ascii_digit).map(|_| i + 1)),
            PatKind::Prim(Prim::Letter) => to(at.filter(|c| c.is_alphabetic()).map(|_| {
                let mut j = i + 1;
                while j < n && is_mark(self.c[j]) {
                    j += 1;
                }
                j
            })),
            PatKind::Prim(Prim::Line) => to(self.line_start(i).then(|| self.line_end(i))),
            PatKind::Prim(Prim::Row | Prim::Col) => to(Some(i)),
            PatKind::Prim(Prim::Eof) => to((i == n).then_some(i)),
            PatKind::Ref(name) => self.ends(definition(self.q, &name.name), i),
            PatKind::Label(_, inner) => self.ends(inner, i),
            PatKind::Seq(items) => items.iter().fold(Ends::from([i]), |set, item| self.all(item, &set)),
            PatKind::Or(alts) => alts.iter().flat_map(|a| self.ends(a, i)).collect(),
            PatKind::Repeat(r) => self.repeat(r, i),
        }
    }

    fn repeat(&self, r: &Repeat, i: usize) -> Ends {
        let skip = |set: Ends| match &r.skip {
            Some(k) => self.closure(k, set),
            None => set,
        };
        // With a stop, an iteration (its separator, if any) may only begin where the stop does not match.
        let may_begin = |set: Ends| -> Ends { set.into_iter().filter(|&p| !self.stop_at(r, p)).collect() };
        let start = skip(Ends::from([i]));
        let mut result = Ends::new();
        if r.min == 0 {
            result.extend(start.iter().copied());
        }
        if r.max == Some(0) {
            return self.finish(r, result);
        }
        let mut frontier = self.all(&r.item, &may_begin(start));
        let mut count = 1u32;
        let mut seen_after_min = Ends::new();
        loop {
            if frontier.is_empty() {
                break;
            }
            if count >= r.min.max(1) {
                result.extend(skip(frontier.clone()));
                if r.max.is_none() {
                    // Past the minimum, only positions matter: stop once nothing is new.
                    let fresh: Ends = frontier.difference(&seen_after_min).copied().collect();
                    if fresh.is_empty() {
                        break;
                    }
                    seen_after_min.extend(fresh.iter().copied());
                    frontier = fresh;
                }
            }
            if r.max == Some(count) {
                break;
            }
            let mut glue = may_begin(skip(frontier));
            if let Some(sep) = &r.sep {
                glue = skip(self.all(sep, &glue));
            }
            frontier = self.all(&r.item, &glue);
            count += 1;
        }
        self.finish(r, result)
    }

    /// Ends of a repetition that could stop at each of `ends`: `UNTILBEFORE` needs its stop
    /// right there, `UNTIL` consumes it.
    fn finish(&self, r: &Repeat, ends: Ends) -> Ends {
        match &r.until {
            None => ends,
            Some(u) if u.kind == Stop::Before => ends.into_iter().filter(|&p| self.stop_at(r, p)).collect(),
            // `UNTIL` consumes the longest match of its stop.
            Some(u) => ends.into_iter().filter_map(|p| self.ends(&u.stop, p).into_iter().max()).collect(),
        }
    }
}

fn reference_accepts(q: &Query, input: &str) -> bool {
    let rf = Reference::new(&q.ast, input);
    rf.ends(&q.ast.rule("TEXT").unwrap().pattern, 0).contains(&rf.c.len())
}

fn earley_accepts(q: &Query, input: &str) -> bool {
    let input = Input::new(input);
    txtql::earley::recognize(&q.grammar, &input, &mut txtql::util::Budget::new(u64::MAX)).unwrap().accepted
}

// ---- generating text from a grammar ----

/// Text built by following the grammar. It is not always accepted (adjacent runs merge, and
/// `LINE` needs a line start), so callers keep only what the reference matcher accepts.
fn gen_pattern(q: &txtql::ast::Query, p: &Pattern, r: &mut Rng, out: &mut String) {
    let words = ["x", "yy", "zed"];
    match &p.kind {
        // Case-insensitive literals are sometimes generated in upper case.
        PatKind::Lit { text, ci } if *ci && r.chance(2) => out.push_str(&text.to_uppercase()),
        PatKind::Lit { text, .. } => out.push_str(text),
        PatKind::Prim(Prim::Word) => out.push_str(r.pick(&words)),
        PatKind::Prim(Prim::Float) => out.push_str(r.pick(&["7", "42", "3.5"])),
        PatKind::Prim(Prim::Int) => out.push_str(r.pick(&["7", "42"])),
        PatKind::Prim(Prim::Hex) => out.push_str(r.pick(&["ff", "1A"])),
        PatKind::Prim(Prim::Bin) => out.push_str(r.pick(&["0", "101"])),
        PatKind::Prim(Prim::Ipv4) => out.push_str(r.pick(&["10.0.0.1", "255.255.255.0"])),
        PatKind::Prim(Prim::Ipv6) => out.push_str(r.pick(&["::1", "fe80::a:1", "::ffff:10.0.0.1"])),
        PatKind::Prim(Prim::Newline) => out.push_str(r.pick(&["\n", "\r\n"])),
        PatKind::Prim(Prim::Tab) => out.push('\t'),
        PatKind::Prim(Prim::Punct) => out.push_str(r.pick(&[",", ";", "!"])),
        PatKind::Prim(Prim::Any) => out.push_str(r.pick(&["x", "7", "!", " ", "\n"])),
        PatKind::Prim(Prim::Digit) => out.push_str(r.pick(&["1", "9"])),
        PatKind::Prim(Prim::Letter) => out.push_str(r.pick(&["a", "Q"])),
        // Empty text: nothing to generate (EOF is never generated).
        PatKind::Prim(Prim::Row | Prim::Col | Prim::Eof) => {}
        PatKind::Prim(Prim::Line) => {
            let n = r.below(3);
            let parts: Vec<&str> = (0..n).map(|_| r.pick(&words)).collect();
            out.push_str(&parts.join(" "));
        }
        PatKind::Ref(name) => gen_pattern(q, definition(q, &name.name), r, out),
        PatKind::Label(_, inner) => gen_pattern(q, inner, r, out),
        PatKind::Seq(items) => items.iter().for_each(|i| gen_pattern(q, i, r, out)),
        PatKind::Or(alts) => gen_pattern(q, r.pick_ref(alts), r, out),
        PatKind::Repeat(rep) => {
            let spread = match rep.max {
                Some(m) => (m - rep.min).min(2) as u64 + 1,
                None => 3,
            };
            let count = rep.min as u64 + r.below(spread);
            for k in 0..count {
                if let Some(skip) = &rep.skip
                    && r.chance(2)
                {
                    gen_pattern(q, skip, r, out);
                }
                if k > 0
                    && let Some(sep) = &rep.sep
                {
                    gen_pattern(q, sep, r, out);
                }
                gen_pattern(q, &rep.item, r, out);
            }
            if let Some(u) = &rep.until
                && u.kind == Stop::After
            {
                gen_pattern(q, &u.stop, r, out);
            }
        }
    }
}

fn generate(q: &Query, r: &mut Rng) -> String {
    let mut out = String::new();
    gen_pattern(&q.ast, &q.ast.rule("TEXT").unwrap().pattern, r, &mut out);
    out
}

fn run_small(q: &Query, input: &str) -> Result<txtql::Output, RunError> {
    q.run(input, &Options { max_steps: 2_000_000, ambiguity_steps: 200_000, ..Options::default() })
}

// ---- random templates and conditions, for the printer round trip ----

fn random_tmpl(r: &mut Rng, depth: u32) -> Template {
    let t = |kind| Template { kind, span: Span::default() };
    let leaf = |r: &mut Rng| match r.below(6) {
        0 => t(TmplKind::Str(r.pick(&["k", "a'b", "x\\y", "\n"]).to_string())),
        1 => t(TmplKind::Num(serde_json::Number::from(r.below(100) as i64 - 50))),
        2 => t(TmplKind::Num(serde_json::Number::from_f64(r.pick(&[-1.5, 1e-8, 1e20, 0.1])).unwrap())),
        3 => t(TmplKind::Bool(r.chance(2))),
        4 => t(TmplKind::Null),
        _ => t(TmplKind::Path((0..1 + r.below(3)).map(|_| ident(r.pick(LABELS))).collect())),
    };
    if depth == 0 {
        return leaf(r);
    }
    let each = |r: &mut Rng| {
        r.chance(2).then(|| {
            Box::new(ForEach {
                var: ident(r.pick(LABELS)),
                source: r.chance(2).then(|| random_tmpl(r, 0)),
                span: Span::default(),
            })
        })
    };
    match r.below(4) {
        0 => leaf(r),
        1 => t(TmplKind::Call(
            ident(r.pick(&["NUM", "LOWER", "JOIN"])),
            (0..r.below(3)).map(|_| random_tmpl(r, depth - 1)).collect(),
        )),
        2 => t(TmplKind::Object(
            (0..r.below(3))
                .map(|_| {
                    if r.chance(4) {
                        ObjEntry::Merge(random_tmpl(r, 0))
                    } else {
                        ObjEntry::Pair {
                            key: random_tmpl(r, 0),
                            value: random_tmpl(r, depth - 1),
                            each: each(r),
                            listof: r.chance(4),
                        }
                    }
                })
                .collect(),
        )),
        _ => t(TmplKind::Array(
            (0..r.below(3)).map(|_| ArrElem { value: random_tmpl(r, depth - 1), each: each(r) }).collect(),
        )),
    }
}

fn random_cond(r: &mut Rng, depth: u32) -> Cond {
    let c = |kind| Cond { kind, span: Span::default() };
    if depth == 0 || r.chance(3) {
        return if r.chance(3) {
            c(CondKind::Truthy(random_tmpl(r, 1)))
        } else {
            let op = r.pick(&[
                CmpOp::Eq,
                CmpOp::NotEq,
                CmpOp::Lt,
                CmpOp::Le,
                CmpOp::Gt,
                CmpOp::Ge,
                CmpOp::Contains,
                CmpOp::StartsWith,
                CmpOp::EndsWith,
            ]);
            c(CondKind::Cmp(random_tmpl(r, 1), op, random_tmpl(r, 1)))
        };
    }
    match r.below(3) {
        0 => c(CondKind::Or(Box::new(random_cond(r, depth - 1)), Box::new(random_cond(r, depth - 1)))),
        1 => c(CondKind::And(Box::new(random_cond(r, depth - 1)), Box::new(random_cond(r, depth - 1)))),
        _ => c(CondKind::Not(Box::new(random_cond(r, depth - 1)))),
    }
}

// ---- properties ----

/// 400 cases per property, or `PROPTEST_CASES` for longer runs.
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(400)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(cases()))]

    /// Earley and the reference matcher agree on acceptance.
    #[test]
    fn earley_matches_reference(seed in any::<u64>()) {
        let mut r = rng(seed);
        let (text, q) = random_query(&mut r);
        for _ in 0..8 {
            let input = random_input(&mut r);
            let expected = reference_accepts(&q, &input);
            prop_assert_eq!(earley_accepts(&q, &input), expected, "query:\n{}\ninput: {:?}", text, input);
        }
    }

    /// Text generated from the grammar is accepted by both matchers, and extraction finds a
    /// parse. Generation is not exact (adjacent runs can merge), so only texts the reference
    /// accepts are checked; `generator_stats` shows how many that is.
    #[test]
    fn generated_text_is_accepted(seed in any::<u64>()) {
        let mut r = rng(seed);
        let (text, q) = random_query(&mut r);
        for _ in 0..4 {
            let input = generate(&q, &mut r);
            if !reference_accepts(&q, &input) {
                continue;
            }
            prop_assert!(earley_accepts(&q, &input), "earley rejects generated input\nquery:\n{}\ninput: {:?}", text, input);
            match run_small(&q, &input) {
                Ok(out) => {
                    // Output is always valid JSON.
                    let s = serde_json::to_string(&out.value).unwrap();
                    prop_assert_eq!(serde_json::from_str::<serde_json::Value>(&s).unwrap(), out.value);
                }
                Err(RunError::Match(txtql::error::MatchError::TooExpensive { .. })) => {}
                Err(e) => prop_assert!(false, "run failed: {:?}\nquery:\n{}\ninput: {:?}", e, text, input),
            }
        }
    }

    /// Whenever the recogniser accepts, extraction succeeds and is deterministic.
    #[test]
    fn accepted_inputs_extract(seed in any::<u64>()) {
        let mut r = rng(seed);
        let (text, q) = random_query(&mut r);
        for _ in 0..8 {
            let input = random_input(&mut r);
            if !earley_accepts(&q, &input) {
                continue;
            }
            match (run_small(&q, &input), run_small(&q, &input)) {
                (Ok(a), Ok(b)) => prop_assert_eq!(a.value, b.value),
                (Err(RunError::Match(txtql::error::MatchError::TooExpensive { .. })), _) => {}
                (a, _) => prop_assert!(false, "run failed: {:?}\nquery:\n{}\ninput: {:?}", a.err(), text, input),
            }
        }
    }

    /// Printing a random syntax tree and parsing it back gives the same tree.
    #[test]
    fn printer_round_trip(seed in any::<u64>()) {
        let mut r = rng(seed);
        let mut ast = random_ast(&mut r);
        for rule in &mut ast.rules {
            rule.strict = r.chance(4);
            if r.chance(2) {
                rule.filter = Some(random_cond(&mut r, 2));
            }
            if r.chance(2) {
                rule.template = Some(random_tmpl(&mut r, 3));
            }
        }
        let printed = txtql::printer::print_query(&ast);
        let parsed = txtql::parser::parse(&printed).map_err(|e| format!("{e:?}\n{printed}"));
        prop_assert!(parsed.is_ok(), "{}", parsed.unwrap_err());
        let parsed = parsed.unwrap();
        prop_assert_eq!(common::sexpr_query(&ast), common::sexpr_query(&parsed), "printed:\n{}", printed);
    }

    /// Arbitrary text as a query never panics.
    #[test]
    fn arbitrary_query_text_never_panics(src in "\\PC{0,80}") {
        let _ = Query::compile(&src);
    }

    /// Query-like noise (keywords, symbols, names) never panics, and neither does running it.
    #[test]
    fn query_noise_never_panics(parts in proptest::collection::vec(prop_oneof![
        Just("TEXT"), Just("="), Just("a"), Just("b"), Just(":"), Just("("), Just(")"), Just("OR"),
        Just("1"), Just("0"), Just("TO"), Just("n"), Just("LAZY"), Just("WORD"), Just("ANY"), Just("'x'"),
        Just("SPLITBY"), Just("DIGIT"), Just("LETTER"), Just("SKIPPING"), Just("UNTILBEFORE"), Just("UNTIL"), Just("LINE"),
        Just("NL"), Just("TAB"), Just("INT"), Just("HEX"), Just("IPV4"), Just("IPV6"), Just("AS"), Just("WHERE"), Just("{"), Just("}"), Just("["), Just("]"), Just(","),
        Just("FOR"), Just("IN"), Just("NUM"), Just("("), Just("x"), Just("'"), Just("STRICT"),
        Just("\n"), Just("-"), Just("."), Just("AND"), Just("NOT"), Just("CONTAINS"),
    ], 0..30), input in "[a-c1 ,\n]{0,20}") {
        let src = parts.join(" ");
        if let Ok(q) = Query::compile(&src) {
            let _ = run_small(&q, &input);
        }
    }

    /// Random grammars never panic on random input, accepted or not.
    #[test]
    fn random_grammars_never_panic(seed in any::<u64>(), input in "\\PC{0,30}") {
        let mut r = rng(seed);
        let (_, q) = random_query(&mut r);
        let _ = run_small(&q, &input);
    }

    /// Grammars the checker rejects (empty loops, empty cycles, ...) still never panic or hang
    /// when run unchecked: the runtime limits do not rely on the checker.
    #[test]
    fn unchecked_random_grammars_never_panic(seed in any::<u64>()) {
        let mut r = rng(seed);
        let text = txtql::printer::print_query(&random_ast(&mut r));
        if let Some(q) = Query::compile_unchecked(&text) {
            for _ in 0..4 {
                let input = random_input(&mut r);
                let _ = run_small(&q, &input);
            }
        }
    }

    /// Word and number runs found by the input model agree with the reference matcher's own
    /// segmentation, and positions map back to the text.
    #[test]
    fn input_runs_match_reference(text in "\\PC{0,60}|[a-z0-9 .,'’\n\r\té\u{301}]{0,60}") {
        let input = Input::new(&text);
        let q = Query::compile("TEXT = ANY").unwrap();
        let rf = Reference::new(&q.ast, &text);
        prop_assert_eq!(input.len(), rf.c.len());
        for i in 0..input.len() {
            prop_assert_eq!(input.word_at(i), rf.word[i], "word at {}", i);
            prop_assert_eq!(input.number_at(i), rf.number[i], "number at {}", i);
            prop_assert_eq!(input.char(i), Some(rf.c[i]));
            prop_assert_eq!(input.slice(i, i + 1).chars().next(), Some(rf.c[i]));
        }
        prop_assert_eq!(input.slice(0, input.len()), text.as_str());
    }

    /// `1 TO n WORD` split by whitespace yields exactly one item per word.
    #[test]
    fn one_item_per_word(words in proptest::collection::vec("[a-zA-Zé]{1,8}", 1..40), seps in proptest::collection::vec("[ \n\t]{1,3}", 40)) {
        let mut text = words[0].clone();
        for (w, s) in words[1..].iter().zip(&seps) {
            text.push_str(s);
            text.push_str(w);
        }
        let q = Query::compile("TEXT = 1 TO n WORD SPLITBY (1 TO n (' ' OR TAB OR NL))").unwrap();
        let out = q.run(&text, &Options::default()).unwrap();
        prop_assert_eq!(out.value, serde_json::json!(words));
    }
}

/// Coverage statistics for the generators (run manually with `--ignored --nocapture`).
#[test]
#[ignore]
fn generator_stats() {
    let (mut accepted, mut total, mut generated, mut gen_ok, mut reps, mut ors, mut refs) = (0, 0, 0, 0, 0, 0, 0);
    for seed in 0..3000u64 {
        let mut r = rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let (text, q) = random_query(&mut r);
        reps += text.matches(" TO ").count();
        ors += text.matches(" OR ").count();
        refs += text.matches(" r").count();
        for _ in 0..8 {
            let input = random_input(&mut r);
            total += 1;
            if earley_accepts(&q, &input) {
                accepted += 1;
            }
        }
        let input = generate(&q, &mut r);
        if reference_accepts(&q, &input) {
            generated += 1;
            if run_small(&q, &input).is_ok() {
                gen_ok += 1;
            }
        }
    }
    println!("random inputs accepted: {accepted}/{total}");
    println!("generated inputs accepted: {generated}/3000, extracted ok: {gen_ok}");
    println!(
        "per grammar: {:.2} repetitions, {:.2} ORs, {:.2} refs",
        reps as f64 / 3000.0,
        ors as f64 / 3000.0,
        refs as f64 / 3000.0
    );
}
