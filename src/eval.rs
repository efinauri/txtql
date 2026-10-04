//! Turns parse trees into JSON values.
//!
//! **Captures.** A rule's captures are its labelled parts (`name:pattern`) and the rules it
//! references (captured under their own name). Captures inside a repetition become lists,
//! unless it has at most one item (`0 TO 1`): then they are the value, or null.
//! Parentheses do not create a scope; a label does.
//!
//! **Default values** (rules without `AS`, labelled groups, repetition items):
//! 1. a body that is a single unlabelled repetition → the list of its items' values (with at
//!    most one item: that item's value, or null);
//! 2. a body that is a single rule reference (not labelled) → that rule's value;
//! 3. a body (alternative) with captures → an object of those captures;
//! 4. a body that is a single parenthesised group → that group's value;
//! 5. otherwise → the matched text.

use crate::ast::{CmpOp, Cond, CondKind, ObjEntry, Template, TmplKind};
use crate::error::{EvalError, RepeatedKey, Span};
use crate::extract::{Child, Node, NodeId};
use crate::input::Input;
use crate::ir::{Grammar, NtKind, Role, Sym, Trans};
use crate::util::FxHashMap;
use serde_json::{Map, Number, Value};
use std::cell::{Cell, RefCell};
use std::sync::Arc;

pub struct Ctx<'a> {
    pub g: &'a Grammar,
    pub input: &'a Input<'a>,
    pub nodes: &'a [Node],
    /// Tree nodes visited while building values; callers charge it to the step budget.
    pub work: Cell<u64>,
    /// Current nesting of value construction, guarded by `MAX_VALUE_DEPTH`.
    depth: Cell<usize>,
    /// Object entries that set a key a second time, one per entry: (key, query span, input span).
    pub repeated_keys: RefCell<Vec<RepeatedKey>>,
}

/// Backstop against runaway recursion while building values. `Options::max_depth` bounds only
/// nested rule matches, so this is reached only with a very large user-set limit.
const MAX_VALUE_DEPTH: usize = 4_000_000;

impl<'a> Ctx<'a> {
    pub fn new(g: &'a Grammar, input: &'a Input<'a>, nodes: &'a [Node]) -> Self {
        Ctx { g, input, nodes, work: Cell::new(0), depth: Cell::new(0), repeated_keys: RefCell::new(Vec::new()) }
    }
}

/// Hypothetical changes to a tree, used by the ambiguity analysis.
#[derive(Debug, Default)]
pub struct Overlay {
    /// Read node `.1` wherever the tree refers to node `.0`.
    pub subst: Option<(NodeId, NodeId)>,
    /// Use these values for the given rule nodes.
    pub overrides: FxHashMap<NodeId, Value>,
}

impl Overlay {
    fn resolve(&self, id: NodeId) -> NodeId {
        match self.subst {
            Some((from, to)) if from == id => to,
            _ => id,
        }
    }
}

type Env = Vec<(Arc<str>, Value)>;

/// Builds values bottom-up. Every node's value is computed when its parent needs it and moved
/// into the parent, so building the output is linear and never deep-clones.
struct Eval<'a> {
    ctx: &'a Ctx<'a>,
    overlay: &'a Overlay,
}

pub fn rule_value(ctx: &Ctx, overlay: &Overlay, id: NodeId) -> Result<Value, EvalError> {
    Eval { ctx, overlay }.rule_value(id)
}

pub fn check_where(ctx: &Ctx, overlay: &Overlay, id: NodeId, cond: &Cond) -> Result<bool, EvalError> {
    let e = Eval { ctx, overlay };
    let env = e.rule_env(id)?;
    let span = e.span_of(id);
    let mut scope = Scope::new(env, |_| false);
    let result = eval_cond(cond, &mut scope, span);
    for (_, v) in scope.vars {
        drop_deep(v.value);
    }
    result
}

/// True if nodes `a` and `b` (derivations of the same NT over the same span) contribute the
/// same captures and the same default value to their enclosing rule.
pub fn same_contribution(ctx: &Ctx, a: NodeId, b: NodeId) -> Result<bool, EvalError> {
    let overlay = Overlay::default();
    let e = Eval { ctx, overlay: &overlay };
    let contribution = |id: NodeId| -> Result<Value, EvalError> {
        let mut env = Env::new();
        match ctx.g.nts[ctx.nodes[id as usize].nt as usize].kind {
            NtKind::Rep { .. } => e.collect_rep(id, &mut env)?,
            _ => e.collect(id, &mut env)?,
        }
        let mut parts: Vec<Value> =
            env.into_iter().map(|(n, v)| Value::Array(vec![Value::String(n.to_string()), v])).collect();
        parts.push(e.default_value(id)?);
        Ok(Value::Array(parts))
    };
    let (x, y) = (contribution(a)?, contribution(b)?);
    let same = values_equal(&x, &y);
    drop_deep(x);
    drop_deep(y);
    Ok(same)
}

/// Deep equality without recursion.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    let mut stack = vec![(a, b)];
    while let Some((a, b)) = stack.pop() {
        match (a, b) {
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                stack.extend(x.iter().zip(y));
            }
            (Value::Object(x), Value::Object(y)) => {
                if x.len() != y.len() {
                    return false;
                }
                for ((kx, vx), (ky, vy)) in x.iter().zip(y) {
                    if kx != ky {
                        return false;
                    }
                    stack.push((vx, vy));
                }
            }
            // Scalars, or different kinds (compared shallowly by discriminant).
            _ => {
                if a != b {
                    return false;
                }
            }
        }
    }
    true
}

/// Drops a value without recursion, so arbitrarily deep values cannot overflow the stack.
pub fn drop_deep(v: Value) {
    let mut stack = vec![v];
    while let Some(v) = stack.pop() {
        match v {
            Value::Array(items) => stack.extend(items),
            Value::Object(map) => stack.extend(map.into_iter().map(|(_, v)| v)),
            _ => {}
        }
    }
}

impl Eval<'_> {
    fn node(&self, id: NodeId) -> &Node {
        &self.ctx.nodes[id as usize]
    }

    fn trans(&self, c: &Child) -> &Trans {
        &self.ctx.g.states[c.state as usize].trans[c.trans as usize]
    }

    fn text(&self, start: u32, end: u32) -> Value {
        Value::String(self.ctx.input.slice(start as usize, end as usize).to_string())
    }

    fn span_of(&self, id: NodeId) -> Span {
        let n = self.node(id);
        let (a, b) = self.ctx.input.byte_span(n.start as usize, n.end as usize);
        Span::new(a, b)
    }

    fn rule_value(&self, id: NodeId) -> Result<Value, EvalError> {
        if let Some(v) = self.overlay.overrides.get(&id) {
            return Ok(v.clone());
        }
        let NtKind::Rule(r) = self.ctx.g.nts[self.node(id).nt as usize].kind else {
            unreachable!("rule_value on a non-rule node")
        };
        match &self.ctx.g.rules[r].template {
            Some(t) => {
                let env = self.rule_env(id)?;
                let span = self.span_of(id);
                // Captures used once in the template are moved into the output, not cloned.
                let mut scope = Scope::new(env, |name| uses(t, name) == 1);
                let v = eval_tmpl(t, &mut scope, span);
                for (_, b) in scope.vars {
                    drop_deep(b.value);
                }
                // Report each entry once, at its first repeated key.
                let mut found = self.ctx.repeated_keys.borrow_mut();
                for (key, query_span) in scope.repeated {
                    if !found.iter().any(|r| r.query_span == query_span) {
                        found.push(RepeatedKey::new(key, query_span, span));
                    }
                }
                v
            }
            None => self.default_value(id),
        }
    }

    /// All captures of a rule, with `null` for those not matched.
    fn rule_env(&self, id: NodeId) -> Result<Env, EvalError> {
        let mut env = Env::new();
        self.collect(id, &mut env)?;
        let nt = &self.ctx.g.nts[self.node(id).nt as usize];
        for name in nt.names() {
            if !env.iter().any(|(n, _)| n == name) {
                env.push((name.clone(), Value::Null));
            }
        }
        Ok(env)
    }

    fn collect(&self, id: NodeId, out: &mut Env) -> Result<(), EvalError> {
        crate::util::with_stack(|| {
            for c in &self.node(id).children {
                self.collect_child(c, out)?;
            }
            Ok(())
        })
    }

    fn collect_child(&self, c: &Child, out: &mut Env) -> Result<(), EvalError> {
        let t = self.trans(c);
        if matches!(t.role, Role::Sep | Role::Skip | Role::Stop) {
            return Ok(());
        }
        if let Some(label) = &t.label {
            let v = self.child_value(c)?;
            out.push((label.clone(), v));
            return Ok(());
        }
        if let Sym::Nt(nt) = t.sym {
            let id = self.overlay.resolve(c.node.expect("NT child has a node"));
            match self.ctx.g.nts[nt as usize].kind {
                NtKind::Group => self.collect(id, out)?,
                NtKind::Rep { .. } => self.collect_rep(id, out)?,
                NtKind::Rule(_) | NtKind::Builtin => {}
            }
        }
        Ok(())
    }

    fn collect_rep(&self, id: NodeId, out: &mut Env) -> Result<(), EvalError> {
        let names = &self.ctx.g.nts[self.node(id).nt as usize].alt_names[0];
        let mut lists: Vec<Vec<Value>> = vec![Vec::new(); names.len()];
        for c in &self.node(id).children {
            if self.trans(c).role != Role::Item {
                continue;
            }
            let mut m = Env::new();
            self.collect_child(c, &mut m)?;
            for (name, v) in m {
                if let Some(idx) = names.iter().position(|n| *n == name) {
                    lists[idx].push(v);
                }
            }
        }
        // At most one item (`0 TO 1`): each capture is its value, or null.
        let single = matches!(self.ctx.g.nts[self.node(id).nt as usize].kind, NtKind::Rep { single: true, .. });
        let value =
            |list: Vec<Value>| if single { list.into_iter().next().unwrap_or(Value::Null) } else { Value::Array(list) };
        out.extend(names.iter().cloned().zip(lists.into_iter().map(value)));
        Ok(())
    }

    /// Value of one step of a path (a token, an assertion, or a child node).
    fn child_value(&self, c: &Child) -> Result<Value, EvalError> {
        self.ctx.work.set(self.ctx.work.get() + 1);
        let depth = self.ctx.depth.get() + 1;
        if depth > MAX_VALUE_DEPTH {
            let (a, b) = self.ctx.input.byte_span(c.start as usize, c.end as usize);
            return Err(EvalError::new("the output is nested too deeply", Span::default(), Span::new(a, b)));
        }
        self.ctx.depth.set(depth);
        let result = self.child_value_inner(c);
        self.ctx.depth.set(depth - 1);
        result
    }

    fn child_value_inner(&self, c: &Child) -> Result<Value, EvalError> {
        let t = self.trans(c);
        crate::util::with_stack(|| match t.sym {
            // `ROW` and `COL` are worth the position where they matched.
            Sym::Assert(crate::ir::Assert::Row) => Ok(self.ctx.input.row_col(c.start as usize).0.into()),
            Sym::Assert(crate::ir::Assert::Col) => Ok(self.ctx.input.row_col(c.start as usize).1.into()),
            Sym::Term(_) | Sym::Assert(_) => Ok(self.text(c.start, c.end)),
            Sym::Nt(nt) => {
                let id = self.overlay.resolve(c.node.expect("NT child has a node"));
                match self.ctx.g.nts[nt as usize].kind {
                    NtKind::Rule(_) => self.rule_value(id),
                    // A label on a repetition captures its text; a label on the item captures a list.
                    NtKind::Rep { .. } if t.label.is_some() && !t.implicit => Ok(self.rep_text(id)),
                    NtKind::Group | NtKind::Rep { .. } => self.default_value(id),
                    NtKind::Builtin => Ok(self.text(c.start, c.end)),
                }
            }
        })
    }

    /// The text a repetition matched, without the stop an `UNTIL` consumed.
    fn rep_text(&self, id: NodeId) -> Value {
        let node = self.node(id);
        let end = match node.children.last() {
            Some(c) if self.trans(c).role == Role::Stop => c.start,
            _ => node.end,
        };
        self.text(node.start, end)
    }

    fn default_value(&self, id: NodeId) -> Result<Value, EvalError> {
        let node = self.node(id);
        let (start, end) = (node.start, node.end);
        let nt = &self.ctx.g.nts[node.nt as usize];
        match nt.kind {
            NtKind::Builtin => Ok(self.text(start, end)),
            // An alias captures nothing, so its value is the text it matched.
            NtKind::Group if nt.alias => Ok(self.text(start, end)),
            NtKind::Rep { single, .. } => {
                let mut items = Vec::new();
                for c in &node.children {
                    if self.trans(c).role == Role::Item {
                        items.push(self.child_value(c)?);
                    }
                }
                Ok(if single { items.pop().unwrap_or(Value::Null) } else { Value::Array(items) })
            }
            NtKind::Rule(_) | NtKind::Group => {
                let Some(first) = node.children.first() else { return Ok(self.text(start, end)) };
                let t = self.trans(first);
                let single_kind = match (node.children.len(), &t.label, t.sym) {
                    (1, None, Sym::Nt(n)) => Some(self.ctx.g.nts[n as usize].kind),
                    _ => None,
                };
                let child_id = first.node.map(|n| self.overlay.resolve(n));
                if let Some(NtKind::Rep { .. }) = single_kind {
                    return self.default_value(child_id.unwrap());
                }
                if node.children.len() == 1 && t.implicit {
                    return self.child_value(first);
                }
                let names = &nt.alt_names[t.alt as usize];
                if !names.is_empty() {
                    let mut env = Env::new();
                    self.collect(id, &mut env)?;
                    let mut obj = Map::new();
                    for name in names {
                        let v = env.iter().position(|(n, _)| n == name).map(|i| env.swap_remove(i).1);
                        obj.insert(name.to_string(), v.unwrap_or(Value::Null));
                    }
                    return Ok(Value::Object(obj));
                }
                if let Some(NtKind::Group) = single_kind {
                    return self.default_value(child_id.unwrap());
                }
                Ok(self.text(start, end))
            }
        }
    }
}

// ---- templates ----

struct Binding {
    value: Value,
    /// Referenced at most once by the code that runs in this scope, so it can be moved out.
    movable: bool,
}

struct Scope {
    vars: Vec<(Arc<str>, Binding)>,
    /// Keys set a second time while building objects, with the entry that did it.
    repeated: Vec<(String, Span)>,
}

impl Scope {
    fn new(env: Env, movable: impl Fn(&str) -> bool) -> Scope {
        Scope {
            repeated: Vec::new(),
            vars: env
                .into_iter()
                .map(|(n, value)| {
                    let movable = movable(&n);
                    (n, Binding { value, movable })
                })
                .collect(),
        }
    }

    /// The value bound to `name`: moved out if this is its only use, cloned otherwise.
    fn fetch(&mut self, name: &str) -> Value {
        match self.vars.iter_mut().rev().find(|(n, _)| &**n == name) {
            Some((_, b)) if b.movable => std::mem::take(&mut b.value),
            Some((_, b)) => b.value.clone(),
            None => Value::Null,
        }
    }
}

/// How often `name` may be read by `t`. Reads inside `FOR` bodies count twice, since the
/// body can run many times; the count is conservative (a shadowed name still counts).
fn uses(t: &Template, name: &str) -> u32 {
    fn each_uses(f: &Option<Box<crate::ast::ForEach>>, name: &str) -> u32 {
        match f {
            Some(f) => match &f.source {
                Some(src) => uses(src, name),
                None => u32::from(f.var.name == name),
            },
            None => 0,
        }
    }
    let body = |n: u32, each: bool| if each { n.saturating_mul(2) } else { n };
    match &t.kind {
        TmplKind::Str(_) | TmplKind::Num(_) | TmplKind::Bool(_) | TmplKind::Null => 0,
        TmplKind::Path(parts) => u32::from(parts[0].name == name),
        TmplKind::Call(_, args) => args.iter().map(|a| uses(a, name)).sum(),
        TmplKind::Object(entries) => entries
            .iter()
            .map(|e| match e {
                ObjEntry::Pair { key, value, each, .. } => {
                    body(uses(key, name) + uses(value, name), each.is_some()) + each_uses(each, name)
                }
                ObjEntry::Merge(src) => uses(src, name),
            })
            .sum(),
        TmplKind::Array(elems) => {
            elems.iter().map(|e| body(uses(&e.value, name), e.each.is_some()) + each_uses(&e.each, name)).sum()
        }
    }
}

/// The hint for a value without a key that is not an object.
pub fn merge_help(src: &Template) -> String {
    let text = crate::printer::print_tmpl(src);
    let key = match &src.kind {
        TmplKind::Path(path) => path.last().map_or(text.clone(), |p| p.name.clone()),
        _ => "key".to_string(),
    };
    format!("an entry without a key is merged; to put the value under a key, write `'{key}': {text}`")
}

pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "an object",
    }
}

/// Short JSON rendering for messages. Depth- and length-bounded, so it is cheap and cannot
/// recurse deeply on large values.
pub fn preview(v: &Value) -> String {
    fn go(v: &Value, depth: u32, out: &mut String) {
        if out.len() > 100 {
            return;
        }
        match v {
            Value::Array(items) if depth == 0 && !items.is_empty() => out.push_str("[…]"),
            Value::Object(m) if depth == 0 && !m.is_empty() => out.push_str("{…}"),
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if out.len() > 100 {
                        out.push('…');
                        break;
                    }
                    go(item, depth - 1, out);
                }
                out.push(']');
            }
            Value::Object(m) => {
                out.push('{');
                for (i, (k, item)) in m.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if out.len() > 100 {
                        out.push('…');
                        break;
                    }
                    out.push_str(&serde_json::to_string(k).unwrap_or_default());
                    out.push(':');
                    go(item, depth - 1, out);
                }
                out.push('}');
            }
            scalar => out.push_str(&serde_json::to_string(scalar).unwrap_or_default()),
        }
    }
    let mut out = String::new();
    go(v, 4, &mut out);
    truncate(&out, 80)
}

pub fn preview_text(s: &str) -> String {
    // Text on one line, or blank text, is shown exactly (escaped): its spaces may be the point.
    if !s.contains(['\n', '\r']) || s.trim().is_empty() {
        return serde_json::to_string(&truncate(s, 50)).expect("strings always serialize");
    }
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("\"{}\"", truncate(&flat, 50))
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{cut}…")
    }
}

fn eval_tmpl(t: &Template, scope: &mut Scope, input: Span) -> Result<Value, EvalError> {
    crate::util::with_stack(|| eval_tmpl_inner(t, scope, input))
}

fn eval_tmpl_inner(t: &Template, scope: &mut Scope, input: Span) -> Result<Value, EvalError> {
    let err = |msg: String| EvalError::new(msg, t.span, input);
    Ok(match &t.kind {
        TmplKind::Str(s) => Value::String(s.clone()),
        TmplKind::Num(n) => Value::Number(n.clone()),
        TmplKind::Bool(b) => Value::Bool(*b),
        TmplKind::Null => Value::Null,
        TmplKind::Path(parts) => {
            let mut v = scope.fetch(&parts[0].name);
            for field in &parts[1..] {
                v = match v {
                    Value::Object(mut m) => m.remove(&field.name).unwrap_or(Value::Null),
                    Value::Null => Value::Null,
                    other => {
                        return Err(EvalError::new(
                            format!("cannot read field `{}` of {}", field.name, type_name(&other)),
                            field.span,
                            input,
                        ));
                    }
                };
            }
            v
        }
        TmplKind::Call(name, args) => {
            let mut vals = Vec::with_capacity(args.len());
            for a in args {
                vals.push(eval_tmpl(a, scope, input)?);
            }
            let mut repeated = None;
            let v = call(&name.name, vals, &mut repeated).map_err(|(msg, help)| {
                let e = err(msg);
                match help {
                    Some(h) => e.with_help(h),
                    None => e,
                }
            })?;
            // Like an object entry that sets a key again; the whole call is the culprit.
            scope.repeated.extend(repeated.map(|key| (key, t.span)));
            v
        }
        TmplKind::Object(entries) => {
            let mut obj = Obj::default();
            for e in entries {
                match e {
                    ObjEntry::Pair { key, value, each: None, listof } => {
                        let k = key_string(eval_tmpl(key, scope, input)?, key.span, input)?;
                        let v = eval_tmpl(value, scope, input)?;
                        obj.set(k, v, *listof, key.span, scope);
                    }
                    ObjEntry::Pair { key, value, each: Some(f), listof } => {
                        let body_uses = |n: &str| uses(key, n) + uses(value, n);
                        for_each(f, scope, input, body_uses, |scope| {
                            let k = key_string(eval_tmpl(key, scope, input)?, key.span, input)?;
                            let v = eval_tmpl(value, scope, input)?;
                            obj.set(k, v, *listof, key.span, scope);
                            Ok(())
                        })?;
                    }
                    ObjEntry::Merge(src) => match eval_tmpl(src, scope, input)? {
                        Value::Null => {}
                        Value::Object(m) => obj.merge(m, src.span, scope),
                        Value::Array(items) => {
                            for item in items {
                                match item {
                                    Value::Object(m) => obj.merge(m, src.span, scope),
                                    Value::Null => {}
                                    other => {
                                        return Err(EvalError::new(
                                            format!(
                                                "only objects can be merged, but the list contains {}",
                                                type_name(&other)
                                            ),
                                            src.span,
                                            input,
                                        )
                                        .with_help(format!(
                                            "{}, or give the repeated rule an object template, e.g. `AS {{ key: value }}`",
                                            merge_help(src)
                                        )));
                                    }
                                }
                            }
                        }
                        other => {
                            return Err(EvalError::new(
                                format!("only objects can be merged, not {}", type_name(&other)),
                                src.span,
                                input,
                            )
                            .with_help(merge_help(src)));
                        }
                    },
                }
            }
            Value::Object(obj.map)
        }
        TmplKind::Array(elems) => {
            let mut out = Vec::new();
            for e in elems {
                match &e.each {
                    None => out.push(eval_tmpl(&e.value, scope, input)?),
                    Some(f) => for_each(
                        f,
                        scope,
                        input,
                        |n| uses(&e.value, n),
                        |scope| {
                            out.push(eval_tmpl(&e.value, scope, input)?);
                            Ok(())
                        },
                    )?,
                }
            }
            Value::Array(out)
        }
    })
}

/// An object under construction.
#[derive(Default)]
struct Obj {
    map: Map<String, Value>,
    /// Keys whose value is a `LISTOF` list, so later `LISTOF` values join it.
    collected: Vec<String>,
}

impl Obj {
    /// `key: value`, or `key: LISTOF value`. Setting a key a second time keeps the later value
    /// and is recorded, unless both are `LISTOF`.
    fn set(&mut self, key: String, value: Value, listof: bool, span: Span, scope: &mut Scope) {
        if listof
            && self.collected.contains(&key)
            && let Some(Value::Array(items)) = self.map.get_mut(&key)
        {
            items.push(value);
            return;
        }
        if self.map.contains_key(&key) {
            scope.repeated.push((key.clone(), span));
        }
        self.collected.retain(|k| *k != key);
        if listof {
            self.collected.push(key.clone());
            self.map.insert(key, Value::Array(vec![value]));
        } else if let Some(old) = self.map.insert(key, value) {
            drop_deep(old);
        }
    }

    fn merge(&mut self, fields: Map<String, Value>, span: Span, scope: &mut Scope) {
        for (k, v) in fields {
            self.set(k, v, false, span, scope);
        }
    }
}

fn key_string(v: Value, span: Span, input: Span) -> Result<String, EvalError> {
    match v {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        other => {
            Err(EvalError::new(format!("object keys must be text, but this is {}", type_name(&other)), span, input)
                .with_help("use a capture that matches a single piece of text, or JOIN(...) a list"))
        }
    }
}

/// Runs `body` once per element of the `FOR` source. `body_uses(name)` says how often the
/// body reads `name` (see `uses`), which decides whether bindings can be moved.
fn for_each(
    f: &crate::ast::ForEach,
    scope: &mut Scope,
    input: Span,
    body_uses: impl Fn(&str) -> u32,
    mut body: impl FnMut(&mut Scope) -> Result<(), EvalError>,
) -> Result<(), EvalError> {
    let source = match &f.source {
        Some(src) => eval_tmpl(src, scope, input)?,
        None => scope.fetch(&f.var.name),
    };
    let items = match source {
        Value::Array(items) => items,
        Value::Null => Vec::new(),
        other => {
            let span = f.source.as_ref().map_or(f.var.span, |s| s.span);
            return Err(EvalError::new(format!("`FOR` needs a list, but this is {}", type_name(&other)), span, input));
        }
    };
    let var: Arc<str> = f.var.name.as_str().into();
    let var_uses = body_uses(&f.var.name);
    for item in items {
        let mark = scope.vars.len();
        match item {
            // Fields of object elements are also in scope. Move them out when the element
            // itself is never read; otherwise clone them.
            Value::Object(m) if var_uses == 0 => {
                for (k, v) in m {
                    let movable = body_uses(&k) == 1;
                    scope.vars.push((k.as_str().into(), Binding { value: v, movable }));
                }
                scope.vars.push((var.clone(), Binding { value: Value::Null, movable: false }));
            }
            item => {
                if let Value::Object(m) = &item {
                    for (k, v) in m {
                        scope.vars.push((k.as_str().into(), Binding { value: v.clone(), movable: false }));
                    }
                }
                scope.vars.push((var.clone(), Binding { value: item, movable: var_uses == 1 }));
            }
        }
        let r = body(scope);
        for (_, b) in scope.vars.drain(mark..) {
            drop_deep(b.value);
        }
        r?;
    }
    Ok(())
}

type CallError = (String, Option<String>);

fn to_number(v: &Value) -> Option<Number> {
    match v {
        Value::Number(n) => Some(n.clone()),
        Value::String(s) => {
            let s = s.trim();
            if let Ok(i) = s.parse::<i64>() {
                Some(i.into())
            } else {
                s.parse::<f64>().ok().filter(|f| f.is_finite()).and_then(Number::from_f64)
            }
        }
        _ => None,
    }
}

/// Applies a built-in function. `ZIP` stores the first key it sees twice in `repeated`: the
/// later value wins, as in an object template.
fn call(name: &str, args: Vec<Value>, repeated: &mut Option<String>) -> Result<Value, CallError> {
    let mut args = args.into_iter();
    let a = args.next().unwrap_or(Value::Null);
    let text_fn = |a: Value, f: fn(&str) -> String| match a {
        Value::String(s) => Ok(Value::String(f(&s))),
        Value::Null => Ok(Value::Null),
        other => Err((format!("`{name}` needs text, but got {}", type_name(&other)), None)),
    };
    match name {
        "NUM" => match &a {
            Value::Null => Ok(Value::Null),
            _ => to_number(&a).map(Value::Number).ok_or_else(|| {
                (
                    format!("cannot convert {} to a number", preview(&a)),
                    Some("NUM accepts text such as \"42\" or \"3.5\"".to_string()),
                )
            }),
        },
        "LOWER" => text_fn(a, str::to_lowercase),
        "UPPER" => text_fn(a, str::to_uppercase),
        "TRIM" => text_fn(a, |s| s.trim().to_string()),
        "COUNT" => match a {
            Value::Array(v) => Ok(v.len().into()),
            Value::Object(m) => Ok(m.len().into()),
            Value::Null => Ok(0.into()),
            other => Err((format!("`COUNT` needs a list, but got {}", type_name(&other)), None)),
        },
        "FIRST" | "LAST" => match a {
            Value::Array(mut v) => Ok(if name == "FIRST" {
                if v.is_empty() { Value::Null } else { v.swap_remove(0) }
            } else {
                v.pop().unwrap_or(Value::Null)
            }),
            Value::Null => Ok(Value::Null),
            other => Err((format!("`{name}` needs a list, but got {}", type_name(&other)), None)),
        },
        "JOIN" => {
            let sep = match args.next() {
                None => return Err(("`JOIN` needs a separator, e.g. `JOIN(list, ' ')`".to_string(), None)),
                Some(Value::String(s)) => s,
                Some(other) => {
                    return Err((format!("the `JOIN` separator must be text, not {}", type_name(&other)), None));
                }
            };
            match a {
                Value::Array(items) => {
                    let mut parts = Vec::with_capacity(items.len());
                    for item in items {
                        parts.push(match item {
                            Value::String(s) => s,
                            Value::Number(n) => n.to_string(),
                            Value::Null => continue,
                            other => {
                                return Err((
                                    format!("`JOIN` can only join text, but the list contains {}", type_name(&other)),
                                    None,
                                ));
                            }
                        });
                    }
                    Ok(Value::String(parts.join(&sep)))
                }
                Value::String(s) => Ok(Value::String(s)),
                Value::Null => Ok(Value::Null),
                other => Err((format!("`JOIN` needs a list, but got {}", type_name(&other)), None)),
            }
        }
        "ZIP" => {
            let keys = match a {
                Value::Array(keys) => keys,
                Value::Null => Vec::new(),
                other => return Err((format!("`ZIP` needs a list of keys, but got {}", type_name(&other)), None)),
            };
            let mut values = match args.next() {
                Some(Value::Array(values)) => values.into_iter(),
                Some(Value::Null) | None => Vec::new().into_iter(),
                Some(other) => {
                    return Err((format!("`ZIP` needs a list of values, but got {}", type_name(&other)), None));
                }
            };
            if values.len() > keys.len() {
                return Err((
                    format!("`ZIP` got {} values for {} keys", values.len(), keys.len()),
                    Some("every value needs a key; missing values become null, but extra ones are an error".into()),
                ));
            }
            let mut obj = Map::new();
            for key in keys {
                let key = match key {
                    Value::String(s) => s,
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    other => return Err((format!("`ZIP` keys must be text, but one is {}", type_name(&other)), None)),
                };
                if obj.contains_key(&key) && repeated.is_none() {
                    *repeated = Some(key.clone());
                }
                obj.insert(key, values.next().unwrap_or(Value::Null));
            }
            Ok(Value::Object(obj))
        }
        _ => Err((format!("unknown function `{name}`"), None)),
    }
}

// ---- conditions ----

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(m) => !m.is_empty(),
        Value::Number(_) => true,
    }
}

fn eval_cond(c: &Cond, scope: &mut Scope, input: Span) -> Result<bool, EvalError> {
    Ok(match &c.kind {
        CondKind::Or(a, b) => eval_cond(a, scope, input)? || eval_cond(b, scope, input)?,
        CondKind::And(a, b) => eval_cond(a, scope, input)? && eval_cond(b, scope, input)?,
        CondKind::Not(a) => !eval_cond(a, scope, input)?,
        CondKind::Truthy(t) => truthy(&eval_tmpl(t, scope, input)?),
        CondKind::Cmp(a, op, b) => {
            let l = eval_tmpl(a, scope, input)?;
            let r = eval_tmpl(b, scope, input)?;
            compare(&l, *op, &r).map_err(|msg| EvalError::new(msg, c.span, input))?
        }
    })
}

fn compare(l: &Value, op: CmpOp, r: &Value) -> Result<bool, String> {
    let nums = || -> Option<(f64, f64)> {
        let a = to_number(l)?.as_f64()?;
        let b = to_number(r)?.as_f64()?;
        // Both sides as numbers (numeric text counts). Whether that applies is up to the operator: `=` and `!=`
        // use it only when at least one side is a number, so two texts compare as exact text.
        Some((a, b))
    };
    match op {
        CmpOp::Eq | CmpOp::NotEq => {
            let eq = match (l, r) {
                (Value::Number(_), _) | (_, Value::Number(_)) => match nums() {
                    Some((a, b)) => a == b,
                    None => l == r,
                },
                _ => l == r,
            };
            Ok(eq == (op == CmpOp::Eq))
        }
        CmpOp::Lt | CmpOp::Le | CmpOp::Gt | CmpOp::Ge => {
            let ord = match (nums(), l, r) {
                (Some((a, b)), _, _) => a.partial_cmp(&b),
                (None, Value::String(a), Value::String(b)) => Some(a.cmp(b)),
                _ => None,
            };
            let Some(ord) = ord else {
                return Err(format!(
                    "cannot compare {} ({}) with {} ({}) using `{}`",
                    preview(l),
                    type_name(l),
                    preview(r),
                    type_name(r),
                    op.as_str()
                ));
            };
            Ok(match op {
                CmpOp::Lt => ord.is_lt(),
                CmpOp::Le => ord.is_le(),
                CmpOp::Gt => ord.is_gt(),
                _ => ord.is_ge(),
            })
        }
        CmpOp::Contains => match (l, r) {
            (Value::String(a), Value::String(b)) => Ok(a.contains(b.as_str())),
            (Value::Array(items), needle) => Ok(items.contains(needle)),
            (Value::Object(m), Value::String(k)) => Ok(m.contains_key(k)),
            (Value::Null, _) => Ok(false),
            _ => Err(format!("`CONTAINS` needs text or a list on the left, not {}", type_name(l))),
        },
        CmpOp::StartsWith | CmpOp::EndsWith => match (l, r) {
            (Value::String(a), Value::String(b)) => {
                Ok(if op == CmpOp::StartsWith { a.starts_with(b.as_str()) } else { a.ends_with(b.as_str()) })
            }
            (Value::Null, _) => Ok(false),
            _ => Err(format!("`{}` needs text on both sides, not {} and {}", op.as_str(), type_name(l), type_name(r))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(name: &str, args: Vec<Value>) -> Result<Value, CallError> {
        super::call(name, args, &mut None)
    }

    #[test]
    fn functions() {
        assert_eq!(call("NUM", vec![json!(" 42 ")]).unwrap(), json!(42));
        assert_eq!(call("NUM", vec![json!("3.5")]).unwrap(), json!(3.5));
        assert_eq!(call("NUM", vec![json!(null)]).unwrap(), json!(null));
        assert!(call("NUM", vec![json!("abc")]).is_err());
        assert!(call("NUM", vec![json!("inf")]).is_err());
        assert_eq!(call("LOWER", vec![json!("AbC")]).unwrap(), json!("abc"));
        assert_eq!(call("UPPER", vec![json!("straße")]).unwrap(), json!("STRASSE"));
        assert_eq!(call("TRIM", vec![json!("  x ")]).unwrap(), json!("x"));
        assert!(call("TRIM", vec![json!([1])]).is_err());
        assert_eq!(call("COUNT", vec![json!([1, 2])]).unwrap(), json!(2));
        assert_eq!(call("COUNT", vec![json!(null)]).unwrap(), json!(0));
        assert_eq!(call("FIRST", vec![json!([1, 2])]).unwrap(), json!(1));
        assert_eq!(call("LAST", vec![json!([1, 2])]).unwrap(), json!(2));
        assert_eq!(call("FIRST", vec![json!([])]).unwrap(), json!(null));
        // The checker requires the separator; without one the call is an error rather than a default.
        assert!(call("JOIN", vec![json!(["a", "b"])]).is_err());
        assert_eq!(call("JOIN", vec![json!(["a", "b"]), json!(" ")]).unwrap(), json!("a b"));
        assert_eq!(call("JOIN", vec![json!(["a", 1, null]), json!(", ")]).unwrap(), json!("a, 1"));
        assert!(call("JOIN", vec![json!([[1]]), json!(" ")]).is_err());
    }

    #[test]
    fn comparisons() {
        assert!(compare(&json!("42"), CmpOp::Eq, &json!(42)).unwrap());
        assert!(compare(&json!("a"), CmpOp::NotEq, &json!(1)).unwrap());
        assert!(compare(&json!("10"), CmpOp::Gt, &json!("9")).unwrap(), "numeric text compares as numbers");
        assert!(compare(&json!("b"), CmpOp::Gt, &json!("a")).unwrap());
        assert!(compare(&json!([1]), CmpOp::Lt, &json!(2)).is_err());
        assert!(compare(&json!("roses"), CmpOp::StartsWith, &json!("ro")).unwrap());
        assert!(compare(&json!("roses"), CmpOp::EndsWith, &json!("es")).unwrap());
        assert!(compare(&json!(["a", "b"]), CmpOp::Contains, &json!("b")).unwrap());
        assert!(compare(&json!("abc"), CmpOp::Contains, &json!("bc")).unwrap());
        assert!(!compare(&json!(null), CmpOp::Contains, &json!("x")).unwrap());
    }

    #[test]
    fn truthiness() {
        for v in [json!(null), json!(false), json!(""), json!([]), json!({})] {
            assert!(!truthy(&v), "{v}");
        }
        for v in [json!(0), json!("x"), json!([0]), json!(true)] {
            assert!(truthy(&v), "{v}");
        }
    }

    #[test]
    fn previews() {
        assert_eq!(preview_text("black\n  and yellow"), "\"black and yellow\"");
        assert_eq!(truncate("abcdef", 4), "abc…");
    }
}
