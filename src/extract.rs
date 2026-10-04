//! Picks one parse tree out of the chart (disambiguation) and looks for output-changing
//! ambiguity.
//!
//! For a node `(nt, i, j)` we search the NT's NFA for a path from its start state at token `i`
//! to an accepting state at token `j`. Options at each step are tried in preference order:
//!
//! 1. transitions in declaration order (earlier `OR` branches first; for repetitions, another
//!    iteration before stopping, or the reverse when LAZY);
//! 2. for each transition, longer spans before shorter ones (shorter first when LAZY).
//!
//! The first path whose rule `WHERE` clause holds wins. Child nodes are chosen the same way,
//! independently of their context, and memoised, which keeps the search polynomial.

use crate::earley::Chart;
use crate::error::{Ambiguity, EvalError, MatchError, Span};
use crate::eval::{self, Ctx, Overlay};
use crate::input::Input;
use crate::ir::{Grammar, NtId, NtKind, Role, StateId, Sym};
use crate::util::{Budget, FxHashMap, FxHashSet};

pub type NodeId = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    /// State the transition leaves from, and its index in that state's list.
    pub state: StateId,
    pub trans: u16,
    pub start: u32,
    pub end: u32,
    /// Set for NT transitions.
    pub node: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub nt: NtId,
    pub start: u32,
    pub end: u32,
    pub children: Vec<Child>,
}

#[derive(Debug)]
pub enum Error {
    Match(MatchError),
    Eval(EvalError),
}

impl From<MatchError> for Error {
    fn from(e: MatchError) -> Self {
        Error::Match(e)
    }
}

impl From<EvalError> for Error {
    fn from(e: EvalError) -> Self {
        Error::Eval(e)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Opt {
    Stop,
    Take { trans: u16, end: u32 },
}

struct Frame {
    state: StateId,
    pos: u32,
    opts: Vec<Opt>,
    next: usize,
    /// A complete path was found below this frame (so it must not be marked dead).
    found: bool,
}

/// Resumable depth-first search for paths through one NT's NFA, in preference order.
struct PathSearch {
    target: u32,
    initial: (StateId, u32),
    frames: Vec<Frame>,
    path: Vec<Child>,
    dead: FxHashSet<(StateId, u32)>,
    on_stack: FxHashSet<(StateId, u32)>,
    started: bool,
    /// Memo for `Extractor::can_complete`: can `(state, pos)` reach an accepting state at
    /// `target` using spans from the chart (ignoring WHERE)?
    reach: FxHashMap<(StateId, u32), bool>,
}

pub struct Extractor<'a> {
    pub g: &'a Grammar,
    pub input: &'a Input<'a>,
    pub chart: &'a Chart,
    pub nodes: Vec<Node>,
    memo: FxHashMap<(NtId, u32, u32), Option<NodeId>>,
    in_progress: FxHashSet<(NtId, u32, u32)>,
    depth: usize,
    max_depth: usize,
    budget: &'a mut Budget,
    /// Ambiguities found so far, kept when the analysis runs out of budget.
    partial: Option<Vec<Ambiguity>>,
}

impl<'a> Extractor<'a> {
    pub fn new(
        g: &'a Grammar,
        input: &'a Input<'a>,
        chart: &'a Chart,
        budget: &'a mut Budget,
        max_depth: usize,
    ) -> Self {
        Extractor {
            g,
            input,
            chart,
            nodes: Vec::new(),
            memo: FxHashMap::default(),
            in_progress: FxHashSet::default(),
            depth: 0,
            max_depth,
            budget,
            partial: None,
        }
    }

    pub fn steps(&self) -> u64 {
        self.budget.used
    }

    pub fn ctx(&self) -> Ctx<'_> {
        Ctx::new(self.g, self.input, &self.nodes)
    }

    fn byte_span(&self, i: u32, j: u32) -> Span {
        let (a, b) = self.input.byte_span(i as usize, j as usize);
        Span::new(a, b)
    }

    /// Best valid derivation of `nt` over tokens `i..j`, or `None` if there is none.
    pub fn best(&mut self, nt: NtId, i: u32, j: u32) -> Result<Option<NodeId>, Error> {
        crate::util::with_stack(|| self.best_inner(nt, i, j))
    }

    fn best_inner(&mut self, nt: NtId, i: u32, j: u32) -> Result<Option<NodeId>, Error> {
        let key = (nt, i, j);
        if let Some(&r) = self.memo.get(&key) {
            return Ok(r);
        }
        if !self.chart.has(nt, i as usize, j as usize) {
            return Ok(None);
        }
        if !self.in_progress.insert(key) {
            // A cycle through the same span; `check` rules these out, but stay safe.
            return Ok(None);
        }
        // Only rule matches count towards the nesting limit, not the groups and repetitions inside them.
        let nests = usize::from(self.is_rule(nt));
        self.depth += nests;
        // The ambiguity analysis keeps going after some errors, so leave no trace of this call.
        let result = self.choose(nt, i, j);
        self.depth -= nests;
        self.in_progress.remove(&key);
        let result = result?;
        self.memo.insert(key, result);
        Ok(result)
    }

    fn is_rule(&self, nt: NtId) -> bool {
        matches!(self.g.nts[nt as usize].kind, NtKind::Rule(_))
    }

    fn choose(&mut self, nt: NtId, i: u32, j: u32) -> Result<Option<NodeId>, Error> {
        if self.depth > self.max_depth {
            return Err(MatchError::TooDeep { limit: self.max_depth, span: self.byte_span(i, j) }.into());
        }
        let start = self.g.nts[nt as usize].start;
        let mut search = PathSearch::new(start, i, j);
        while let Some(children) = self.next_path(&mut search)? {
            let id = self.push_node(nt, i, j, children);
            if self.accepts(id)? {
                return Ok(Some(id));
            }
        }
        Ok(None)
    }

    fn push_node(&mut self, nt: NtId, start: u32, end: u32, children: Vec<Child>) -> NodeId {
        self.nodes.push(Node { nt, start, end, children });
        (self.nodes.len() - 1) as NodeId
    }

    /// Checks the rule's WHERE clause, if any.
    fn accepts(&mut self, id: NodeId) -> Result<bool, Error> {
        let NtKind::Rule(r) = self.g.nts[self.nodes[id as usize].nt as usize].kind else { return Ok(true) };
        let Some(cond) = &self.g.rules[r].filter else { return Ok(true) };
        let ctx = Ctx::new(self.g, self.input, &self.nodes);
        let ok = eval::check_where(&ctx, &Overlay::default(), id, cond)?;
        self.budget.spend(ctx.work.get())?;
        Ok(ok)
    }

    fn options(&self, state: StateId, pos: u32, target: u32) -> Vec<Opt> {
        let st = &self.g.states[state as usize];
        let mut opts = Vec::new();
        let can_stop = st.accept && pos == target;
        if can_stop && st.accept_first {
            opts.push(Opt::Stop);
        }
        for (ti, t) in st.trans.iter().enumerate() {
            let trans = ti as u16;
            match t.sym {
                Sym::Term(tid) => {
                    if let Some(end) = self.g.term_match(tid, self.input, pos as usize)
                        && end <= target as usize
                    {
                        opts.push(Opt::Take { trans, end: end as u32 });
                    }
                }
                Sym::Assert(a) => {
                    if a.holds(&self.g.terms, self.input, pos as usize) {
                        opts.push(Opt::Take { trans, end: pos });
                    }
                }
                Sym::Nt(c) => {
                    let ends = self.chart.ends(c, pos as usize).filter(|&e| e <= target as usize);
                    if self.g.nts[c as usize].is_lazy() {
                        opts.extend(ends.map(|e| Opt::Take { trans, end: e as u32 }));
                    } else {
                        opts.extend(ends.rev().map(|e| Opt::Take { trans, end: e as u32 }));
                    }
                }
            }
        }
        if can_stop && !st.accept_first {
            opts.push(Opt::Stop);
        }
        opts
    }

    /// Advances `search` to its next complete path.
    fn next_path(&mut self, search: &mut PathSearch) -> Result<Option<Vec<Child>>, Error> {
        if !search.started {
            search.started = true;
            let (state, pos) = search.initial();
            let opts = self.options(state, pos, search.target);
            search.push(state, pos, opts);
        }
        loop {
            self.budget.spend(1)?;
            let Some(frame) = search.frames.last_mut() else { return Ok(None) };
            if frame.next >= frame.opts.len() {
                search.pop();
                continue;
            }
            let opt = frame.opts[frame.next];
            frame.next += 1;
            let (state, pos) = (frame.state, frame.pos);
            match opt {
                Opt::Stop => {
                    for f in &mut search.frames {
                        f.found = true;
                    }
                    return Ok(Some(search.path.clone()));
                }
                Opt::Take { trans, end } => {
                    let t = &self.g.states[state as usize].trans[trans as usize];
                    let (to, sym) = (t.to, t.sym);
                    if search.dead.contains(&(to, end)) || search.on_stack.contains(&(to, end)) {
                        continue;
                    }
                    // Building a child can be expensive; first make sure the rest can follow.
                    if matches!(sym, Sym::Nt(_)) && !self.can_complete(search, to, end)? {
                        continue;
                    }
                    let node = match sym {
                        Sym::Nt(c) => match self.best(c, pos, end)? {
                            Some(id) => Some(id),
                            None => continue,
                        },
                        _ => None,
                    };
                    search.path.push(Child { state, trans, start: pos, end, node });
                    let opts = self.options(to, end, search.target);
                    search.push(to, end, opts);
                }
            }
        }
    }

    /// Structural check: can `(state, pos)` reach an accepting state at the search target,
    /// using only spans recorded in the chart? Iterative, memoised per search, so a whole
    /// search costs O(states × positions) at most.
    fn can_complete(&mut self, search: &mut PathSearch, state: StateId, pos: u32) -> Result<bool, Error> {
        if let Some(&r) = search.reach.get(&(state, pos)) {
            return Ok(r);
        }
        let target = search.target;
        // Stack of (state, pos, successors, next index).
        type Frame = (StateId, u32, Vec<(StateId, u32)>, usize);
        let mut stack: Vec<Frame> = Vec::new();
        let mut on_path: FxHashSet<(StateId, u32)> = FxHashSet::default();
        let expand = |this: &Self, s: StateId, p: u32| -> (bool, Vec<(StateId, u32)>) {
            let st = &this.g.states[s as usize];
            if st.accept && p == target {
                return (true, Vec::new());
            }
            let mut next = Vec::new();
            for opt in this.options(s, p, target) {
                if let Opt::Take { trans, end } = opt {
                    next.push((st.trans[trans as usize].to, end));
                }
            }
            (false, next)
        };
        let (done, succ) = expand(self, state, pos);
        if done {
            search.reach.insert((state, pos), true);
            return Ok(true);
        }
        stack.push((state, pos, succ, 0));
        on_path.insert((state, pos));
        while let Some(top) = stack.last_mut() {
            self.budget.spend(1)?;
            if top.3 >= top.2.len() {
                let (s, p, _, _) = stack.pop().unwrap();
                on_path.remove(&(s, p));
                search.reach.insert((s, p), false);
                continue;
            }
            let next = top.2[top.3];
            top.3 += 1;
            let found = match search.reach.get(&next) {
                Some(&r) => r,
                None if on_path.contains(&next) => false,
                None => {
                    let (done, succ) = expand(self, next.0, next.1);
                    if !done {
                        stack.push((next.0, next.1, succ, 0));
                        on_path.insert(next);
                        continue;
                    }
                    true
                }
            };
            if found {
                // Everything on the current path can reach the target through `next`.
                search.reach.insert(next, true);
                for (s, p, _, _) in stack.drain(..) {
                    search.reach.insert((s, p), true);
                }
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// First complete path from `(state, pos)`, used to finish alternative derivations.
    fn complete_from(&mut self, state: StateId, pos: u32, target: u32) -> Result<Option<Vec<Child>>, Error> {
        let mut search = PathSearch::new(state, pos, target);
        self.next_path(&mut search)
    }
}

impl PathSearch {
    fn new(state: StateId, pos: u32, target: u32) -> PathSearch {
        PathSearch {
            target,
            initial: (state, pos),
            frames: Vec::new(),
            path: Vec::new(),
            dead: FxHashSet::default(),
            on_stack: FxHashSet::default(),
            started: false,
            reach: FxHashMap::default(),
        }
    }

    fn initial(&self) -> (StateId, u32) {
        self.initial
    }

    fn push(&mut self, state: StateId, pos: u32, opts: Vec<Opt>) {
        self.on_stack.insert((state, pos));
        self.frames.push(Frame { state, pos, opts, next: 0, found: false });
    }

    fn pop(&mut self) {
        let f = self.frames.pop().expect("pop on empty search");
        self.on_stack.remove(&(f.state, f.pos));
        if !f.found {
            self.dead.insert((f.state, f.pos));
        }
        // Every frame but the first was entered through a path step.
        if !self.frames.is_empty() {
            self.path.pop();
        }
    }
}

/// Where an alternative departs from the chosen derivation of a node: after `k` children, at
/// `state` and token `pos`, taking `alt` instead.
#[derive(Clone, Copy)]
struct AltAt {
    id: NodeId,
    k: usize,
    state: StateId,
    pos: u32,
    alt: Opt,
}

/// An alternative reading that changes the output of the rule `rule`.
struct Change {
    rule: usize,
    /// Preview of the rule's value in the chosen reading.
    before: String,
    /// Preview of its value in the alternative, or why that reading fails to evaluate.
    after: Result<String, String>,
    /// Advice if only the chosen reading has a stop repetition running across lines.
    crossing: Option<String>,
}

/// Why an alternative reading cannot be evaluated, or the error itself if it ends the analysis
/// (an exhausted budget): only evaluation errors and the depth limit make an alternative fail.
fn failure_reason(e: Error) -> Result<String, Error> {
    match e {
        Error::Eval(e) => Ok(e.message),
        Error::Match(MatchError::TooDeep { limit, .. }) => Ok(format!("it nests more than {limit} rule matches")),
        e => Err(e),
    }
}

/// Result of the ambiguity analysis.
#[derive(Debug, Default)]
pub struct AmbiguityReport {
    pub found: Vec<Ambiguity>,
    /// False if the analysis ran out of budget before visiting the whole tree.
    pub complete: bool,
}

const MAX_REPORTED: usize = 20;

impl Extractor<'_> {
    /// Looks for places in the chosen tree where another derivation would change the output.
    /// Each node is checked against derivations that differ from the chosen one in exactly
    /// one decision; differences inside skipped text and separators are ignored.
    pub fn analyze(&mut self, root: NodeId, step_limit: u64) -> Result<AmbiguityReport, Error> {
        let saved_limit = self.budget.limit;
        self.budget.limit = self.budget.used.saturating_add(step_limit);
        let result = self.analyze_inner(root);
        self.budget.limit = saved_limit;
        match result {
            Ok(found) => Ok(AmbiguityReport { found, complete: true }),
            Err(Error::Match(MatchError::TooExpensive { .. })) => {
                Ok(AmbiguityReport { found: self.partial.take().unwrap_or_default(), complete: false })
            }
            Err(e) => Err(e),
        }
    }

    fn analyze_inner(&mut self, root: NodeId) -> Result<Vec<Ambiguity>, Error> {
        // Parent links for the chosen tree only.
        let mut parent: FxHashMap<NodeId, NodeId> = FxHashMap::default();
        // Rule nesting depth of each node, to apply `max_depth` to alternatives where they would sit.
        let mut depths: FxHashMap<NodeId, usize> = FxHashMap::default();
        depths.insert(root, 1);
        let mut order = vec![root];
        let mut k = 0;
        while k < order.len() {
            let id = order[k];
            k += 1;
            let node = &self.nodes[id as usize];
            for c in &node.children {
                let t = &self.g.states[c.state as usize].trans[c.trans as usize];
                if matches!(t.role, Role::Sep | Role::Skip | Role::Stop) {
                    continue;
                }
                if let Some(child) = c.node {
                    parent.insert(child, id);
                    depths.insert(child, depths[&id] + usize::from(self.is_rule(self.nodes[child as usize].nt)));
                    order.push(child);
                }
            }
        }
        self.partial = Some(Vec::new());
        for id in order {
            if self.partial.as_ref().is_some_and(|p| p.len() >= MAX_REPORTED) {
                break;
            }
            self.depth = depths[&id];
            if let Some(amb) = self.analyze_node(id, &parent)? {
                self.partial.as_mut().unwrap().push(amb);
            }
        }
        self.depth = 0;
        Ok(self.partial.take().unwrap_or_default())
    }

    fn analyze_node(&mut self, id: NodeId, parent: &FxHashMap<NodeId, NodeId>) -> Result<Option<Ambiguity>, Error> {
        let node = self.nodes[id as usize].clone();
        if matches!(self.g.nts[node.nt as usize].kind, NtKind::Builtin) {
            return Ok(None);
        }
        let mut state = self.g.nts[node.nt as usize].start;
        let mut pos = node.start;
        // Shared completability memo for all alternatives of this node.
        let mut reach = PathSearch::new(state, pos, node.end);
        for k in 0..=node.children.len() {
            let opts = self.options(state, pos, node.end);
            let chosen = match node.children.get(k) {
                Some(c) => Opt::Take { trans: c.trans, end: c.end },
                None => Opt::Stop,
            };
            let Some(idx) = opts.iter().position(|o| *o == chosen) else { break };
            let chosen_skip = self.is_skip(state, chosen);
            for &alt in &opts[idx + 1..] {
                if chosen_skip || self.is_skip(state, alt) || self.lazy_choice(&node, state, chosen, alt) {
                    continue;
                }
                let alt_at = AltAt { id, k, state, pos, alt };
                // An alternative that cannot be evaluated is still another reading of the text.
                let change = match self.try_alternative(&node, alt_at, &mut reach, parent) {
                    Ok(change) => change,
                    Err(e) => {
                        let reason = failure_reason(e)?;
                        self.failing_alternative(id, parent, reason)?
                    }
                };
                let Some(Change { rule, before, after, crossing }) = change else { continue };
                let (chosen_span, chosen_desc) = self.describe_opt(state, pos, chosen, &node);
                let (alt_span, alt_desc) = self.describe_opt(state, pos, alt, &node);
                let strict = self.g.rules[rule].strict;
                let hint = crossing.or_else(|| self.branch_hint(state, pos, chosen, alt));
                let (alt_value, alt_failure) = match after {
                    Ok(value) => (Some(value), None),
                    Err(reason) => (None, Some(reason)),
                };
                return Ok(Some(Ambiguity {
                    rule: self.g.rules[rule].name.clone(),
                    node_span: self.byte_span(node.start, node.end),
                    chosen_span,
                    chosen_desc,
                    alt_span,
                    alt_desc,
                    chosen_value: before,
                    alt_value,
                    alt_failure,
                    strict,
                    hint,
                    label_node: self
                        .input
                        .slice(node.start as usize, node.end as usize)
                        .trim_end_matches(['\n', '\r'])
                        .contains(['\n', '\r']),
                }));
            }
            if let Some(c) = node.children.get(k) {
                state = self.g.states[c.state as usize].trans[c.trans as usize].to;
                pos = c.end;
            }
        }
        Ok(None)
    }

    /// Builds the alternative reading `at.alt` of `node` and, if it is a valid reading that
    /// changes the output, says how. Errors are those of building or evaluating it.
    fn try_alternative(
        &mut self,
        node: &Node,
        at: AltAt,
        reach: &mut PathSearch,
        parent: &FxHashMap<NodeId, NodeId>,
    ) -> Result<Option<Change>, Error> {
        let AltAt { id, k, state, pos, alt } = at;
        // Build the alternative only once it is known to complete: copying the prefix
        // for every candidate would make long lists quadratic.
        let tail = match alt {
            Opt::Stop => Vec::new(),
            Opt::Take { trans, end } => {
                let t = &self.g.states[state as usize].trans[trans as usize];
                let to = t.to;
                if !self.can_complete(reach, to, end)? {
                    return Ok(None);
                }
                let child = match t.sym {
                    Sym::Nt(c) => match self.best(c, pos, end)? {
                        Some(n) => Some(n),
                        None => return Ok(None),
                    },
                    _ => None,
                };
                let Some(rest) = self.complete_from(to, end, node.end)? else { return Ok(None) };
                let mut tail = Vec::with_capacity(rest.len() + 1);
                tail.push(Child { state, trans, start: pos, end, node: child });
                tail.extend(rest);
                tail
            }
        };
        self.budget.spend(node.children.len() as u64)?;
        let mut children = node.children[..k].to_vec();
        children.extend(tail);
        let alt_id = self.push_node(node.nt, node.start, node.end, children);
        // Through an empty cycle, an alternative can contain the very node it replaces;
        // substituting it would make the value infinitely deep.
        if self.contains_same_span(alt_id, id) || !self.accepts(alt_id)? {
            return Ok(None);
        }
        // Cheap local test first: if the node contributes the same captures and value to
        // its rule, nothing above it can change.
        if !matches!(self.g.nts[node.nt as usize].kind, NtKind::Rule(_)) {
            let ctx = Ctx::new(self.g, self.input, &self.nodes);
            let same = eval::same_contribution(&ctx, id, alt_id)?;
            self.budget.spend(ctx.work.get())?;
            if same {
                return Ok(None);
            }
        }
        let Some((rule, before, after)) = self.output_change(id, alt_id, parent)? else { return Ok(None) };
        let crossing = self.crossing_hint(id, alt_id);
        Ok(Some(Change { rule, before, after: Ok(after), crossing }))
    }

    /// The report for an alternative that fails to evaluate: it names the nearest rule at or
    /// above the node, like any other, with the reason in place of the alternative's value.
    fn failing_alternative(
        &mut self,
        id: NodeId,
        parent: &FxHashMap<NodeId, NodeId>,
        reason: String,
    ) -> Result<Option<Change>, Error> {
        let mut top = id;
        let rule = loop {
            if let NtKind::Rule(r) = self.g.nts[self.nodes[top as usize].nt as usize].kind {
                break r;
            }
            match parent.get(&top) {
                Some(&p) => top = p,
                None => return Ok(None),
            }
        };
        let n = &self.nodes[top as usize];
        self.budget.spend(u64::from(n.end - n.start) + 1)?;
        let ctx = Ctx::new(self.g, self.input, &self.nodes);
        let before = eval::rule_value(&ctx, &Overlay::default(), top)?;
        let shown = eval::preview(&before);
        eval::drop_deep(before);
        self.budget.spend(ctx.work.get())?;
        Ok(Some(Change { rule, before: shown, after: Err(reason), crossing: None }))
    }

    /// True if the choice between `a` and `b` was settled by an explicit `LAZY`: either both
    /// are spans of the same lazy repetition, or they decide whether a lazy repetition goes on.
    fn lazy_choice(&self, node: &Node, state: StateId, a: Opt, b: Opt) -> bool {
        if self.g.nts[node.nt as usize].is_lazy() {
            return true;
        }
        match (a, b) {
            (Opt::Take { trans: ta, .. }, Opt::Take { trans: tb, .. }) if ta == tb => {
                match self.g.states[state as usize].trans[ta as usize].sym {
                    Sym::Nt(c) => self.g.nts[c as usize].is_lazy(),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Advice when the choice was between two `OR` branches: the earlier one wins, which may or
    /// may not be what was meant.
    fn branch_hint(&self, state: StateId, pos: u32, chosen: Opt, alt: Opt) -> Option<String> {
        let (Opt::Take { trans: a, end }, Opt::Take { trans: b, .. }) = (chosen, alt) else { return None };
        let trans = &self.g.states[state as usize].trans;
        let (ta, tb) = (&trans[a as usize], &trans[b as usize]);
        if ta.alt == tb.alt {
            return None;
        }
        let name = |t: &crate::ir::Trans| match &t.label {
            Some(l) => format!("`{l}`"),
            None => self.g.describe_sym(t.sym),
        };
        let (first, second) = (name(ta), name(tb));
        let text = self.input.slice(pos as usize, end as usize);
        let text = if text.chars().count() <= 20 { crate::printer::quote(text) } else { "'…'".to_string() };
        Some(format!(
            "{first} was chosen because it comes first in the `OR`.\n\
             - If {second} is the special case, list it first.\n\
             - If this order is intended, exclude the special case from {second} with WHERE, e.g. `WHERE t != {text}`."
        ))
    }

    /// Advice when the chosen reading has a stop repetition running across line breaks and the
    /// other reading does not: that is usually the mistake.
    fn crossing_hint(&self, chosen: NodeId, alt: NodeId) -> Option<String> {
        let crossing = |root: NodeId| -> Option<NtId> {
            let mut stack = vec![root];
            let mut seen = FxHashSet::default();
            while let Some(id) = stack.pop() {
                if !seen.insert(id) {
                    continue;
                }
                let node = &self.nodes[id as usize];
                let nt = &self.g.nts[node.nt as usize];
                if nt.until_fix.is_some() {
                    // The text of its items, without a consumed stop.
                    let end = match node.children.last() {
                        Some(c)
                            if self.g.states[c.state as usize].trans[c.trans as usize].role
                                == crate::ir::Role::Stop =>
                        {
                            c.start
                        }
                        _ => node.end,
                    };
                    if self.input.slice(node.start as usize, end as usize).contains(['\n', '\r']) {
                        return Some(node.nt);
                    }
                }
                stack.extend(node.children.iter().filter_map(|c| c.node));
            }
            None
        };
        let nt = crossing(chosen)?;
        if crossing(alt).is_some() {
            return None;
        }
        let (written, fixed) = self.g.nts[nt as usize].until_fix.as_ref()?;
        Some(format!(
            "In the chosen reading, `{written}` runs across line breaks (ANY includes them).\n\
             - To keep it on one line, add NL to its stop: `{fixed}`\n\
             - If it should cross lines, make the stop say where it ends, e.g. `UNTILBEFORE (NL '[' OR NL EOF)`."
        ))
    }

    /// Is `target` in the subtree of `root`? Only descendants with the same span can be
    /// (a node cannot contain itself over a different span), so only those are searched.
    fn contains_same_span(&self, root: NodeId, target: NodeId) -> bool {
        let span = |id: NodeId| (self.nodes[id as usize].start, self.nodes[id as usize].end);
        let want = span(target);
        let mut stack = vec![root];
        let mut seen = FxHashSet::default();
        while let Some(id) = stack.pop() {
            if id == target {
                return true;
            }
            if !seen.insert(id) {
                continue;
            }
            for c in &self.nodes[id as usize].children {
                if let Some(child) = c.node
                    && span(child) == want
                {
                    stack.push(child);
                }
            }
        }
        false
    }

    fn is_skip(&self, state: StateId, opt: Opt) -> bool {
        match opt {
            Opt::Stop => false,
            Opt::Take { trans, .. } => self.g.states[state as usize].trans[trans as usize].role == Role::Skip,
        }
    }

    fn describe_opt(&self, state: StateId, pos: u32, opt: Opt, node: &Node) -> (Span, String) {
        match opt {
            Opt::Stop => (self.byte_span(pos, pos), "the match ends here".into()),
            Opt::Take { trans, end } => {
                let t = &self.g.states[state as usize].trans[trans as usize];
                let what = match &t.label {
                    Some(l) => format!("`{l}`"),
                    None => self.g.describe_sym(t.sym),
                };
                let text = self.input.slice(pos as usize, end as usize);
                let _ = node;
                (self.byte_span(pos, end), format!("{what} = {}", eval::preview_text(text)))
            }
        }
    }

    /// If replacing node `orig` by `alt` changes the final output, returns the nearest rule
    /// whose value changes, with previews of its value before and after.
    fn output_change(
        &mut self,
        orig: NodeId,
        alt: NodeId,
        parent: &FxHashMap<NodeId, NodeId>,
    ) -> Result<Option<(usize, String, String)>, Error> {
        let rule_of = |this: &Self, id: NodeId| match this.g.nts[this.nodes[id as usize].nt as usize].kind {
            NtKind::Rule(r) => Some(r),
            _ => None,
        };
        // Nearest rule node at or above `orig`.
        let mut r0 = orig;
        while rule_of(self, r0).is_none() {
            match parent.get(&r0) {
                Some(&p) => r0 = p,
                None => return Ok(None),
            }
        }
        // Re-evaluating a rule costs roughly its size.
        let size = |this: &Self, id: NodeId| {
            let n = &this.nodes[id as usize];
            u64::from(n.end - n.start) + 1
        };
        self.budget.spend(size(self, r0))?;
        let r0_rule = rule_of(self, r0).unwrap();
        let ctx = Ctx::new(self.g, self.input, &self.nodes);
        let none = Overlay::default();
        let before = eval::rule_value(&ctx, &none, r0)?;
        let after = if r0 == orig {
            eval::rule_value(&ctx, &none, alt)?
        } else {
            let overlay = Overlay { subst: Some((orig, alt)), ..Overlay::default() };
            if let Some(cond) = &self.g.rules[r0_rule].filter
                && !eval::check_where(&ctx, &overlay, r0, cond)?
            {
                return Ok(None);
            }
            eval::rule_value(&ctx, &overlay, r0)?
        };
        if eval::values_equal(&before, &after) {
            eval::drop_deep(before);
            eval::drop_deep(after);
            return Ok(None);
        }
        let first = (r0_rule, eval::preview(&before), eval::preview(&after));
        eval::drop_deep(before);
        // Propagate the new value up through enclosing rules until it stops mattering.
        let mut changed = (r0, after);
        let mut cur = r0;
        let mut result = Some(());
        while let Some(&p) = parent.get(&cur) {
            cur = p;
            let Some(r) = rule_of(self, cur) else { continue };
            self.budget.spend(size(self, cur))?;
            let overlay = Overlay { subst: None, overrides: [changed].into_iter().collect() };
            if let Some(cond) = &self.g.rules[r].filter
                && !eval::check_where(&ctx, &overlay, cur, cond)?
            {
                result = None;
                break;
            }
            let new = eval::rule_value(&ctx, &overlay, cur)?;
            let old = eval::rule_value(&ctx, &none, cur)?;
            let same = eval::values_equal(&new, &old);
            eval::drop_deep(old);
            overlay.overrides.into_values().for_each(eval::drop_deep);
            if same {
                eval::drop_deep(new);
                result = None;
                break;
            }
            changed = (cur, new);
        }
        self.budget.spend(ctx.work.get())?;
        if result.is_none() {
            return Ok(None);
        }
        Ok(Some(first))
    }
}
