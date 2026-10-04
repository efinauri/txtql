//! Pretty-printer: turns a syntax tree back into query text that parses to the same tree.

use crate::ast::*;
use std::fmt::Write;

pub fn print_query(q: &Query) -> String {
    let mut out = String::new();
    for alias in &q.aliases {
        out.push_str(&format!("ALIAS {} = {}\n", alias.name.name, print_pattern(&alias.pattern)));
    }
    for rule in &q.rules {
        out.push_str(&print_rule(rule));
        out.push('\n');
    }
    out
}

pub fn print_rule(r: &Rule) -> String {
    let mut out = String::new();
    if r.strict {
        out.push_str("STRICT ");
    }
    write!(out, "{} = {}", r.name.name, print_pattern(&r.pattern)).unwrap();
    if let Some(c) = &r.filter {
        write!(out, " WHERE {}", print_cond(c)).unwrap();
    }
    if let Some(t) = &r.template {
        write!(out, " AS {}", print_tmpl(t)).unwrap();
    }
    out
}

pub fn quote(s: &str) -> String {
    let mut out = String::from("'");
    for c in s.chars() {
        match c {
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

pub fn print_pattern(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Or(alts) => alts.iter().map(print_seq_level).collect::<Vec<_>>().join(" OR "),
        _ => print_seq_level(p),
    }
}

fn print_seq_level(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Seq(items) => items.iter().map(print_item).collect::<Vec<_>>().join(" "),
        PatKind::Or(_) => format!("({})", print_pattern(p)),
        _ => print_item(p),
    }
}

/// Prints something in `item` position: a label, a repetition, or an atom.
fn print_item(p: &Pattern) -> String {
    match &p.kind {
        // `k:ANY UNTIL s` would label ANY, not the repetition.
        PatKind::Label(name, inner) if is_short_until(inner) => format!("{}:({})", name.name, print_item_body(inner)),
        PatKind::Label(name, inner) => format!("{}:{}", name.name, print_item_body(inner)),
        _ => print_item_body(p),
    }
}

/// `0 TO n x UNTIL s` with nothing else, printed in its short form `x UNTIL s`.
fn is_short_until(p: &Pattern) -> bool {
    matches!(&p.kind, PatKind::Repeat(r)
        if r.min == 0 && r.max.is_none() && !r.lazy && r.sep.is_none() && r.skip.is_none() && r.until.is_some())
}

fn print_item_body(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Repeat(r) if is_short_until(p) => {
            let u = r.until.as_ref().expect("checked by is_short_until");
            format!("{} {} {}", print_nested_item(&r.item), u.kind.keyword(), print_nested_item(&u.stop))
        }
        PatKind::Repeat(r) => {
            let mut out = String::new();
            write!(out, "{} TO ", r.min).unwrap();
            match r.max {
                Some(m) => write!(out, "{m}").unwrap(),
                None => out.push('n'),
            }
            if r.lazy {
                out.push_str(" LAZY");
            }
            write!(out, " {}", print_nested_item(&r.item)).unwrap();
            if let Some(sep) = &r.sep {
                write!(out, " SPLITBY {}", print_nested_item(sep)).unwrap();
            }
            if let Some(skip) = &r.skip {
                write!(out, " SKIPPING {}", print_nested_item(skip)).unwrap();
            }
            if let Some(u) = &r.until {
                write!(out, " {} {}", u.kind.keyword(), print_nested_item(&u.stop)).unwrap();
            }
            out
        }
        _ => print_atom(p),
    }
}

/// An item nested inside a repetition. Nested repetitions are always parenthesised so that
/// trailing `SPLITBY` / `SKIPPING` / `UNTIL` / `UNTILBEFORE` clauses stay attached to the right repetition.
fn print_nested_item(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Repeat(_) => format!("({})", print_item_body(p)),
        PatKind::Label(..) if matches!(p.kind, PatKind::Label(_, ref inner) if matches!(inner.kind, PatKind::Repeat(_))) =>
        {
            format!("({})", print_item(p))
        }
        _ => print_item(p),
    }
}

fn print_atom(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Lit { text, ci } => format!("{}{}", if *ci { "i" } else { "" }, quote(text)),
        PatKind::Prim(prim) => prim.as_str().to_string(),
        PatKind::Ref(name) => name.name.clone(),
        PatKind::Label(..) | PatKind::Seq(_) | PatKind::Or(_) | PatKind::Repeat(_) => {
            format!("({})", print_pattern(p))
        }
    }
}

pub fn print_cond(c: &Cond) -> String {
    print_cond_prec(c, 0)
}

fn print_cond_prec(c: &Cond, prec: u8) -> String {
    let (s, my_prec) = match &c.kind {
        CondKind::Or(a, b) => (format!("{} OR {}", print_cond_prec(a, 1), print_cond_prec(b, 2)), 1),
        CondKind::And(a, b) => (format!("{} AND {}", print_cond_prec(a, 2), print_cond_prec(b, 3)), 2),
        CondKind::Not(a) => (format!("NOT {}", print_cond_prec(a, 3)), 3),
        CondKind::Cmp(a, op, b) => (format!("{} {} {}", print_tmpl(a), op.as_str(), print_tmpl(b)), 4),
        CondKind::Truthy(a) => (print_tmpl(a), 4),
    };
    if my_prec < prec { format!("({s})") } else { s }
}

pub fn print_tmpl(t: &Template) -> String {
    match &t.kind {
        TmplKind::Str(s) => quote(s),
        TmplKind::Num(n) => n.to_string(),
        TmplKind::Bool(b) => b.to_string(),
        TmplKind::Null => "null".into(),
        TmplKind::Path(parts) => parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("."),
        TmplKind::Call(name, args) => {
            format!("{}({})", name.name, args.iter().map(print_tmpl).collect::<Vec<_>>().join(", "))
        }
        TmplKind::Object(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|e| match e {
                    ObjEntry::Pair { key, value, each, listof } => format!(
                        "{}: {}{}{}",
                        print_tmpl(key),
                        if *listof { "LISTOF " } else { "" },
                        print_tmpl(value),
                        print_each(each)
                    ),
                    ObjEntry::Merge(t) => print_tmpl(t),
                })
                .collect();
            if parts.is_empty() { "{}".into() } else { format!("{{ {} }}", parts.join(", ")) }
        }
        TmplKind::Array(elems) => {
            let parts: Vec<String> =
                elems.iter().map(|e| format!("{}{}", print_tmpl(&e.value), print_each(&e.each))).collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

fn print_each(each: &Option<Box<ForEach>>) -> String {
    match each {
        None => String::new(),
        Some(f) => match &f.source {
            None => format!(" FOR {}", f.var.name),
            Some(src) => format!(" FOR {} IN {}", f.var.name, print_tmpl(src)),
        },
    }
}
