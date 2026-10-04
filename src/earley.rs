//! Earley recogniser over NFA-bodied nonterminals.
//!
//! Positions are characters. An item is `(state, origin)`: we are in `state` of some NT's NFA,
//! and that NT started at position `origin`. Terminals can span several characters (a WORD, a
//! literal), so scanned items may land beyond the next position.
//!
//! Repetitions are NFA loops, not recursion, so a list of k items creates O(k) items
//! instead of the O(k²) that right-recursive Earley would.
//!
//! The chart keeps what extraction needs: for every NT and origin, the positions where it can
//! end. Items themselves are dropped after each set, except the "waiters" (items expecting an
//! NT), which completion needs.

use crate::error::MatchError;
use crate::input::Input;
use crate::ir::{Grammar, NtId, StateId, Sym};
use crate::util::{Budget, FxHashMap, FxHashSet};

pub struct Chart {
    /// Sorted `(nt, origin, end)` triples: NT `nt` derives positions `origin..end`.
    completions: Vec<(NtId, u32, u32)>,
    /// Last position the parser reached.
    pub furthest: usize,
    /// Symbols that were expected at `furthest`, with the state expecting them when its NT
    /// started right there (to tell which rule they would have begun).
    pub expected: Vec<(Sym, Option<StateId>)>,
    /// Start of the earliest `UNTIL` / `UNTILBEFORE` repetition still running at `furthest`, if any. They
    /// cross line breaks, so the real problem may be where one started rather than where it failed.
    pub open_until: Option<(usize, crate::ast::Stop)>,
    pub accepted: bool,
}

impl Chart {
    /// End positions of `nt` starting at `origin`, ascending.
    pub fn ends(&self, nt: NtId, origin: usize) -> impl DoubleEndedIterator<Item = usize> + '_ {
        let key = (nt, origin as u32);
        let lo = self.completions.partition_point(|&(n, o, _)| (n, o) < key);
        let hi = self.completions.partition_point(|&(n, o, _)| (n, o) <= key);
        self.completions[lo..hi].iter().map(|&(_, _, e)| e as usize)
    }

    pub fn has(&self, nt: NtId, origin: usize, end: usize) -> bool {
        self.completions.binary_search(&(nt, origin as u32, end as u32)).is_ok()
    }

    pub fn completion_count(&self) -> usize {
        self.completions.len()
    }
}

type Item = (StateId, u32);

/// Items waiting for an NT: `(nt, state after it, origin of the waiting item)`.
type Waiter = (NtId, StateId, u32);

struct Set {
    items: Vec<Item>,
    seen: FxHashSet<Item>,
}

impl Set {
    fn new() -> Set {
        Set { items: Vec::new(), seen: FxHashSet::default() }
    }

    fn add(&mut self, item: Item, budget: &mut Budget) -> Result<(), MatchError> {
        if self.seen.insert(item) {
            budget.spend(1)?;
            self.items.push(item);
        }
        Ok(())
    }

    fn clear(&mut self) {
        self.items.clear();
        self.seen.clear();
    }
}

pub fn recognize(g: &Grammar, input: &Input, budget: &mut Budget) -> Result<Chart, MatchError> {
    let n = input.len();
    let mut completions: Vec<(NtId, u32, u32)> = Vec::new();
    // Waiters of every position, flattened; `waiter_idx[i]..waiter_idx[i+1]` is position i, sorted by NT.
    let mut waiters: Vec<Waiter> = Vec::new();
    let mut waiter_idx: Vec<usize> = vec![0];

    let mut cur = Set::new();
    let mut next = Set::new();
    // Items scanned past a multi-character terminal, by the position they continue at.
    let mut later: FxHashMap<u32, Vec<Item>> = FxHashMap::default();
    let mut here: Vec<Waiter> = Vec::new();
    let mut empty_done: Vec<NtId> = Vec::new();
    let mut completed_here: FxHashSet<(NtId, u32)> = FxHashSet::default();

    let mut furthest = 0;
    let mut expected = Vec::new();
    let mut open_until = None;

    cur.add((g.nts[g.root as usize].start, 0), budget)?;

    for i in 0..=n {
        let pos = i as u32;
        if let Some(items) = later.remove(&pos) {
            for item in items {
                cur.add(item, budget)?;
            }
        }
        here.clear();
        empty_done.clear();
        completed_here.clear();
        if cur.items.is_empty() {
            // Inside a multi-character terminal: nothing happens here, but the parse goes on.
            waiter_idx.push(waiters.len());
            if later.is_empty() {
                break;
            }
            continue;
        }
        furthest = i;

        let mut k = 0;
        while k < cur.items.len() {
            let (s, origin) = cur.items[k];
            k += 1;
            let st = &g.states[s as usize];
            if st.accept && completed_here.insert((st.nt, origin)) {
                completions.push((st.nt, origin, pos));
                if origin == pos {
                    empty_done.push(st.nt);
                    let matching: Vec<Item> = here.iter().filter(|w| w.0 == st.nt).map(|&(_, to, o)| (to, o)).collect();
                    for item in matching {
                        cur.add(item, budget)?;
                    }
                } else {
                    let (a, b) = (waiter_idx[origin as usize], waiter_idx[origin as usize + 1]);
                    let set = &waiters[a..b];
                    let lo = set.partition_point(|w| w.0 < st.nt);
                    for &(_, to, o) in set[lo..].iter().take_while(|w| w.0 == st.nt) {
                        cur.add((to, o), budget)?;
                    }
                }
            }
            for t in &st.trans {
                match t.sym {
                    Sym::Term(tid) => match g.term_match(tid, input, i) {
                        // Only a stop ending in EOF can be empty (`UNTIL (NL OR EOF)` at the end).
                        Some(end) if end == i => cur.add((t.to, origin), budget)?,
                        Some(end) if end == i + 1 => next.add((t.to, origin), budget)?,
                        Some(end) => {
                            budget.spend(1)?;
                            later.entry(end as u32).or_default().push((t.to, origin));
                        }
                        None => {}
                    },
                    Sym::Assert(a) => {
                        if a.holds(&g.terms, input, i) {
                            cur.add((t.to, origin), budget)?;
                        }
                    }
                    Sym::Nt(b) => {
                        here.push((b, t.to, origin));
                        cur.add((g.nts[b as usize].start, pos), budget)?;
                        if empty_done.contains(&b) {
                            cur.add((t.to, origin), budget)?;
                        }
                    }
                }
            }
        }

        // Remember what could have come next, in case this is where the parse dies.
        if (next.items.is_empty() && later.is_empty()) || i == n {
            expected.clear();
            open_until = cur
                .items
                .iter()
                .filter(|&&(_, origin)| origin < pos)
                .filter_map(|&(s, origin)| Some((origin as usize, g.nts[g.states[s as usize].nt as usize].until?)))
                .min_by_key(|&(origin, _)| origin);
            for &(s, origin) in &cur.items {
                for t in &g.states[s as usize].trans {
                    let failed = match t.sym {
                        Sym::Term(tid) => g.term_match(tid, input, i).is_none(),
                        Sym::Assert(a) => !a.holds(&g.terms, input, i),
                        Sym::Nt(_) => false,
                    };
                    let e = (t.sym, (origin == pos).then_some(s));
                    if failed && !expected.contains(&e) {
                        expected.push(e);
                    }
                }
            }
        }

        here.sort_unstable_by_key(|w| w.0);
        waiters.extend_from_slice(&here);
        waiter_idx.push(waiters.len());
        std::mem::swap(&mut cur, &mut next);
        next.clear();
    }

    completions.sort_unstable();
    completions.dedup();
    let accepted = completions.binary_search(&(g.root, 0, n as u32)).is_ok();
    Ok(Chart { completions, furthest, expected, open_until, accepted })
}
