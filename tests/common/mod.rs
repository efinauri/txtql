//! Shared helpers for integration tests.
#![allow(dead_code)]

use serde_json::Value;
use txtql::ast::*;
use txtql::{CompileError, Options, Query, RunError, render_plain};

/// Compiles and runs, panicking with rendered diagnostics on failure.
pub fn run(query: &str, input: &str) -> Value {
    run_opts(query, input, &Options::default()).value
}

pub fn run_opts(query: &str, input: &str, opts: &Options) -> txtql::Output {
    let q = compile(query);
    match q.run(input, opts) {
        Ok(out) => out,
        Err(e) => panic!("run failed:\n{}", render(&e.reports("query", query, "input", input))),
    }
}

pub fn compile(query: &str) -> Query {
    match Query::compile(query) {
        Ok(q) => q,
        Err(e) => panic!("compile failed:\n{}", render(&e.reports("query", query))),
    }
}

/// Rendered compile diagnostics; panics if the query compiles without errors.
pub fn compile_err(query: &str) -> String {
    match Query::compile(query) {
        Ok(_) => panic!("expected a compile error for:\n{query}"),
        Err(e) => render(&e.reports("query", query)),
    }
}

pub fn compile_error(query: &str) -> CompileError {
    Query::compile(query).expect_err("expected a compile error")
}

/// Rendered warnings of a query that compiles.
pub fn warnings(query: &str) -> String {
    let q = compile(query);
    render(&txtql::warning_reports(&q.warnings, "query", query))
}

pub fn run_error(query: &str, input: &str) -> RunError {
    compile(query).run(input, &Options::default()).expect_err("expected a run error")
}

/// Rendered run diagnostics; panics if the run succeeds.
pub fn run_err(query: &str, input: &str) -> String {
    let e = run_error(query, input);
    render(&e.reports("query", query, "input", input))
}

pub fn render(reports: &[miette::Report]) -> String {
    reports.iter().map(render_plain).collect::<Vec<_>>().join("\n")
}

/// Runs `f` on a thread with a large stack (for deep recursion tests).
pub fn big_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().stack_size(1 << 30).spawn(f).unwrap().join().unwrap()
}

// ---- S-expression dump of the syntax tree ----

pub fn sexpr_query(q: &txtql::ast::Query) -> String {
    let aliases = q.aliases.iter().map(|a| format!("(alias {} {})", a.name.name, sexpr_pat(&a.pattern)));
    aliases.chain(q.rules.iter().map(sexpr_rule)).collect::<Vec<_>>().join("\n")
}

pub fn sexpr_rule(r: &Rule) -> String {
    let mut s = format!("(rule {}{} {}", if r.strict { "STRICT " } else { "" }, r.name.name, sexpr_pat(&r.pattern));
    if let Some(c) = &r.filter {
        s.push_str(&format!(" (where {})", sexpr_cond(c)));
    }
    if let Some(t) = &r.template {
        s.push_str(&format!(" (as {})", sexpr_tmpl(t)));
    }
    s.push(')');
    s
}

pub fn sexpr_pat(p: &Pattern) -> String {
    match &p.kind {
        PatKind::Lit { text, ci } => format!("{}{:?}", if *ci { "i" } else { "" }, text),
        PatKind::Prim(prim) => prim.as_str().to_string(),
        PatKind::Ref(n) => n.name.clone(),
        PatKind::Label(n, inner) => format!("(label {} {})", n.name, sexpr_pat(inner)),
        PatKind::Seq(items) => format!("(seq {})", items.iter().map(sexpr_pat).collect::<Vec<_>>().join(" ")),
        PatKind::Or(items) => format!("(or {})", items.iter().map(sexpr_pat).collect::<Vec<_>>().join(" ")),
        PatKind::Repeat(r) => {
            let max = r.max.map_or("n".to_string(), |m| m.to_string());
            let mut s =
                format!("(repeat {} {}{} {}", r.min, max, if r.lazy { " lazy" } else { "" }, sexpr_pat(&r.item));
            if let Some(sep) = &r.sep {
                s.push_str(&format!(" (sep {})", sexpr_pat(sep)));
            }
            if let Some(skip) = &r.skip {
                s.push_str(&format!(" (skip {})", sexpr_pat(skip)));
            }
            if let Some(u) = &r.until {
                let kind = if u.kind == Stop::Before { "before" } else { "after" };
                s.push_str(&format!(" (until-{kind} {})", sexpr_pat(&u.stop)));
            }
            s.push(')');
            s
        }
    }
}

pub fn sexpr_cond(c: &Cond) -> String {
    match &c.kind {
        CondKind::Or(a, b) => format!("(or {} {})", sexpr_cond(a), sexpr_cond(b)),
        CondKind::And(a, b) => format!("(and {} {})", sexpr_cond(a), sexpr_cond(b)),
        CondKind::Not(a) => format!("(not {})", sexpr_cond(a)),
        CondKind::Cmp(a, op, b) => format!("({} {} {})", op.as_str(), sexpr_tmpl(a), sexpr_tmpl(b)),
        CondKind::Truthy(a) => format!("(truthy {})", sexpr_tmpl(a)),
    }
}

fn sexpr_each(each: &Option<Box<ForEach>>) -> String {
    match each {
        None => String::new(),
        Some(f) => match &f.source {
            None => format!(" (for {})", f.var.name),
            Some(src) => format!(" (for {} in {})", f.var.name, sexpr_tmpl(src)),
        },
    }
}

pub fn sexpr_tmpl(t: &Template) -> String {
    match &t.kind {
        TmplKind::Str(s) => format!("{s:?}"),
        TmplKind::Num(n) => n.to_string(),
        TmplKind::Bool(b) => b.to_string(),
        TmplKind::Null => "null".into(),
        TmplKind::Path(parts) => parts.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("."),
        TmplKind::Call(name, args) => {
            format!("({} {})", name.name, args.iter().map(sexpr_tmpl).collect::<Vec<_>>().join(" "))
        }
        TmplKind::Object(entries) => {
            let parts: Vec<String> = entries
                .iter()
                .map(|e| match e {
                    ObjEntry::Pair { key, value, each, listof } => format!(
                        "({} {}{}{})",
                        sexpr_tmpl(key),
                        if *listof { "listof " } else { "" },
                        sexpr_tmpl(value),
                        sexpr_each(each)
                    ),
                    ObjEntry::Merge(t) => format!("(merge {})", sexpr_tmpl(t)),
                })
                .collect();
            format!("{{{}}}", parts.join(" "))
        }
        TmplKind::Array(elems) => {
            let parts: Vec<String> =
                elems.iter().map(|e| format!("{}{}", sexpr_tmpl(&e.value), sexpr_each(&e.each))).collect();
            format!("[{}]", parts.join(" "))
        }
    }
}

/// Parses and returns the S-expression form, or the rendered error.
pub fn parse_sexpr(src: &str) -> String {
    match txtql::parser::parse(src) {
        Ok(q) => sexpr_query(&q),
        Err(e) => panic!("parse failed:\n{}", render(&CompileError::Parse(e).reports("query", src))),
    }
}

pub fn parse_err(src: &str) -> String {
    match txtql::parser::parse(src) {
        Ok(q) => panic!("expected a parse error, got {}", sexpr_query(&q)),
        Err(e) => render(&CompileError::Parse(e).reports("query", src)),
    }
}
