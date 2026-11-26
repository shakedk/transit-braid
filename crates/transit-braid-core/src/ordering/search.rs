//! The search's working state for one part ([`Search`]): the order with each line's slot, so a
//! swap of two lines is costed on the nodes it touches only; and the greedy descent.

use super::chains::Chains;
use super::cost::{End, NodeView};
use super::{Order, Weights, nodes_of};
use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::topology::Topology;

/// The most passes of a greedy descent.
const MAX_DESCENT_PASSES: usize = 50;

/// No slot: the line is not on the edge.
const NO_SLOT: u32 = u32::MAX;

/// The search's working state for one part: the order, each line's slot on each edge, and per
/// node its edge ends' places and its chords by line, so a move is costed in O(chords) per node
/// it touches instead of O(chords²). [`node_cost`](super::cost::node_cost) stays the reference;
/// tests check the two agree.
pub(super) struct Search<'a> {
    topo: &'a Topology,
    views: &'a IdVec<NodeId, NodeView>,
    w: Weights,
    pub(super) order: Order,
    /// `slot[edge][line]`: the line's slot on the edge, or [`NO_SLOT`].
    slot: IdVec<EdgeId, IdVec<LineId, u32>>,
    /// Per node: each edge end with the place its first line takes around the node.
    starts: IdVec<NodeId, Vec<(End, usize)>>,
    /// Per node: chord indices of each line (`by_line[node][line]`).
    by_line: IdVec<NodeId, IdVec<LineId, Vec<usize>>>,
    /// Per node and chord: its two places around the node (low, high), kept up to date on swaps.
    spans: IdVec<NodeId, Vec<Option<(usize, usize)>>>,
    /// Edges no move may change (a warm start changes only edges an edit touched).
    pub(super) frozen: IdVec<EdgeId, bool>,
    /// Per edge, the edges a one-edge move changes with it: its chain, else itself.
    pub(super) members: IdVec<EdgeId, Vec<EdgeId>>,
}

impl<'a> Search<'a> {
    pub(super) fn new(
        topo: &'a Topology,
        views: &'a IdVec<NodeId, NodeView>,
        order: Order,
        w: &Weights,
        lines: usize,
    ) -> Self {
        let mut slot: IdVec<EdgeId, IdVec<LineId, u32>> =
            vec![IdVec::from(vec![NO_SLOT; lines]); order.len()].into();
        for (e, ls) in order.iter_enumerated() {
            for (k, &l) in ls.iter().enumerate() {
                if let Some(s) = slot[e].get_mut(l) {
                    *s = k as u32;
                }
            }
        }
        let starts = views
            .iter()
            .map(|v| {
                let mut before = 0;
                v.ends
                    .iter()
                    .map(|&end| {
                        let at = before;
                        before += order.get(end.edge).map_or(0, Vec::len);
                        (end, at)
                    })
                    .collect()
            })
            .collect();
        let by_line = views
            .iter()
            .map(|v| {
                let mut by: IdVec<LineId, Vec<usize>> = vec![Vec::new(); lines].into();
                for (k, c) in v.chords.iter().enumerate() {
                    if let Some(list) = by.get_mut(c.line) {
                        list.push(k);
                    }
                }
                by
            })
            .collect();
        let mut s = Self {
            topo,
            views,
            w: *w,
            order,
            slot,
            starts,
            by_line,
            spans: views.iter().map(|v| vec![None; v.chords.len()]).collect(),
            frozen: vec![false; topo.edges.len()].into(),
            members: topo.edges.ids().map(|e| vec![e]).collect(),
        };
        for (v, view) in views.iter_enumerated() {
            for k in 0..view.chords.len() {
                s.spans[v][k] = s.span_of(v, k);
            }
        }
        s
    }

    /// Moves on a chain's edge change the whole chain; the order must already carry
    /// each chain's root order along it ([`Chains::carry_all`]).
    pub(super) fn with_chains(mut self, chains: &Chains) -> Self {
        for (e, m) in self.members.iter_mut_enumerated() {
            m.clone_from(&chains.members[chains.root[e]]);
        }
        self
    }

    /// Chord `k` of node `v`: its two places around the node, low first.
    fn span_of(&self, v: NodeId, k: usize) -> Option<(usize, usize)> {
        let c = self.views[v].chords[k];
        let p = self.place(v, c.a, c.line)?;
        let q = self.place(v, c.b, c.line)?;
        Some((p.min(q), p.max(q)))
    }

    /// A line's place around node `v` (see `place` in `cost.rs`), or None when it is not on the
    /// edge.
    fn place(&self, v: NodeId, end: End, line: LineId) -> Option<usize> {
        let &(_, start) = self.starts[v].iter().find(|(e, _)| *e == end)?;
        let s = *self.slot.get(end.edge)?.get(line)?;
        if s == NO_SLOT {
            return None;
        }
        let s = s as usize;
        let across = if end.from {
            s
        } else {
            self.order[end.edge].len() - 1 - s
        };
        Some(start + across)
    }

    fn adjacent(&self, edge: EdgeId, a: LineId, b: LineId) -> bool {
        match (self.slot[edge].get(a), self.slot[edge].get(b)) {
            (Some(&x), Some(&y)) if x != NO_SLOT && y != NO_SLOT => x.abs_diff(y) == 1,
            _ => false,
        }
    }

    /// The weighted cost of one pair of chords at node `v` (as in
    /// [`node_cost`](super::cost::node_cost)).
    fn pair(&self, v: NodeId, i: usize, j: usize) -> f64 {
        let (c, d) = (self.views[v].chords[i], self.views[v].chords[j]);
        if c.line == d.line {
            return 0.0;
        }
        let shared = [c.a, c.b].iter().any(|x| *x == d.a || *x == d.b);
        if !shared {
            return 0.0;
        }
        let together = (c.a == d.a && c.b == d.b) || (c.a == d.b && c.b == d.a);
        let mut total = 0.0;
        if let (Some((p1, q1)), Some((p2, q2))) = (self.spans[v][i], self.spans[v][j]) {
            let interleave = (p1 < p2 && p2 < q1 && q1 < q2) || (p2 < p1 && p1 < q2 && q2 < q1);
            if interleave {
                total += if together {
                    self.w.same_segment
                } else {
                    self.w.different_segment
                };
            }
        }
        if together
            && self.adjacent(c.a.edge, c.line, d.line) != self.adjacent(c.b.edge, c.line, d.line)
        {
            total += self.w.separation;
        }
        total
    }

    /// The cost of every pair at node `v` that involves line `a` or `b` (each pair once).
    fn cost_of_lines(&self, v: NodeId, a: LineId, b: LineId) -> f64 {
        let chords = &self.views[v].chords;
        let mine = |l: LineId| self.by_line[v].get(l).map_or(&[][..], Vec::as_slice);
        let mut total = 0.0;
        for &i in mine(a).iter().chain(mine(b)) {
            for (j, &d) in chords.iter().enumerate() {
                if j == i || ((d.line == a || d.line == b) && j < i) {
                    continue;
                }
                total += self.pair(v, i, j);
            }
        }
        total
    }

    /// Swaps `a` and `b` on each of `edges` (where both are).
    pub(super) fn swap(&mut self, edges: &[EdgeId], a: LineId, b: LineId) {
        for &e in edges {
            let (sa, sb) = (self.slot[e][a], self.slot[e][b]);
            if sa == NO_SLOT || sb == NO_SLOT {
                continue;
            }
            self.order[e].swap(sa as usize, sb as usize);
            self.slot[e][a] = sb;
            self.slot[e][b] = sa;
        }
        for v in nodes_of(self.topo, edges) {
            for line in [a, b] {
                let ks = self.by_line[v].get(line).cloned().unwrap_or_default();
                for k in ks {
                    self.spans[v][k] = self.span_of(v, k);
                }
            }
        }
    }

    /// What swapping `a` and `b` on `edges` changes the cost by; the swap is kept.
    pub(super) fn swap_delta(&mut self, edges: &[EdgeId], a: LineId, b: LineId) -> f64 {
        let nodes = nodes_of(self.topo, edges);
        let before: f64 = nodes.iter().map(|&v| self.cost_of_lines(v, a, b)).sum();
        self.swap(edges, a, b);
        let after: f64 = nodes.iter().map(|&v| self.cost_of_lines(v, a, b)).sum();
        after - before
    }

    /// The edges where lines `a` and `b` run together with edge `e`: from `e`, every edge both
    /// pass on to through the same node, and so on (`e` included). Swapping them on all of these
    /// flips their order along the whole stretch, with no twist in it.
    pub(super) fn shared_run(&self, e: EdgeId, a: LineId, b: LineId) -> Vec<EdgeId> {
        let mut run = vec![e];
        let mut i = 0;
        while i < run.len() {
            let x = run[i];
            i += 1;
            for (v, from) in [
                (self.topo.edges[x].from, true),
                (self.topo.edges[x].to, false),
            ] {
                let here = End { edge: x, from };
                let next = |line: LineId| -> Vec<End> {
                    let chords = &self.views[v].chords;
                    self.by_line[v]
                        .get(line)
                        .map_or(&[][..], Vec::as_slice)
                        .iter()
                        .filter_map(|&k| {
                            let c = chords[k];
                            if c.a == here {
                                Some(c.b)
                            } else if c.b == here {
                                Some(c.a)
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                let of_b = next(b);
                for end in next(a) {
                    let frozen = self.frozen.get(end.edge).copied().unwrap_or(true);
                    if of_b.contains(&end) && !run.contains(&end.edge) && !frozen {
                        run.push(end.edge);
                    }
                }
            }
        }
        run
    }

    /// Greedy descent: swap neighbors, on one edge or on their whole shared stretch, while that
    /// lowers the cost, until a pass finds nothing (or [`MAX_DESCENT_PASSES`]).
    pub(super) fn descend(&mut self, edges: &[EdgeId]) {
        for _ in 0..MAX_DESCENT_PASSES {
            let mut improved = false;
            for &e in edges {
                for i in 0..self.order[e].len().saturating_sub(1) {
                    let (a, b) = (self.order[e][i], self.order[e][i + 1]);
                    for run in [self.members[e].clone(), self.shared_run(e, a, b)] {
                        if self.swap_delta(&run, a, b) < -1e-9 {
                            improved = true;
                            break;
                        }
                        self.swap(&run, a, b);
                    }
                }
            }
            if !improved {
                return;
            }
        }
    }
}
