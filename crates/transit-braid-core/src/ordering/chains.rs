//! Chains of edges joined at pass-through nodes ([`Chains`]): each chain gets one order, so the
//! search has fewer orders to choose from.

use super::Order;
use super::cost::{End, NodeView};
use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::topology::Topology;

/// Chains (the line-ordering papers' chain contraction): edges joined at **pass-through
/// nodes**, where exactly two edge ends carry lines, both edges carry the same lines, and every one of them
/// passes from one to the other there once. At such a node the order carried straight through
/// costs nothing (no crossing, no separation), so a chain gets one order, its root's (its lowest
/// edge): the search has fewer states. Carrying keeps each line on its side as it travels: an
/// edge entering and one leaving the node keep the slot order, two edges both leaving it (or both
/// entering) reverse it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Chains {
    /// Each edge's chain root (itself when it is in no chain).
    pub(super) root: IdVec<EdgeId, EdgeId>,
    /// Whether the edge's slot order is its root's reversed.
    pub(super) flip: IdVec<EdgeId, bool>,
    /// For each root, its chain's edges, sorted (root first); empty for other edges.
    pub(super) members: IdVec<EdgeId, Vec<EdgeId>>,
}

impl Chains {
    /// No chains: every edge its own.
    pub(super) fn identity(edges: usize) -> Self {
        Self {
            root: (0..edges).map(EdgeId::new).collect(),
            flip: vec![false; edges].into(),
            members: (0..edges).map(|e| vec![EdgeId::new(e)]).collect(),
        }
    }

    pub(super) fn new(topo: &Topology, views: &IdVec<NodeId, NodeView>) -> Self {
        let n = topo.edges.len();
        // Union-find with parity: `up[e]` and whether e's order is up[e]'s reversed.
        let mut up: IdVec<EdgeId, EdgeId> = topo.edges.ids().collect();
        let mut odd: IdVec<EdgeId, bool> = vec![false; n].into();
        fn find(
            up: &mut IdVec<EdgeId, EdgeId>,
            odd: &mut IdVec<EdgeId, bool>,
            x: EdgeId,
        ) -> (EdgeId, bool) {
            let mut path = Vec::new();
            let mut r = x;
            while up[r] != r {
                path.push(r);
                r = up[r];
            }
            // Compress: each node on the path points at the root with its total parity.
            for &p in path.iter().rev() {
                let parent = up[p];
                if parent != r {
                    odd[p] ^= odd[parent];
                }
                up[p] = r;
            }
            (r, if x == r { false } else { odd[x] })
        }
        for view in views {
            let busy: Vec<End> = view
                .ends
                .iter()
                .copied()
                .filter(|e| topo.edges.get(e.edge).is_some_and(|x| !x.lines.is_empty()))
                .collect();
            let [x, y] = busy[..] else { continue };
            if x.edge == y.edge || !same_lines_of(topo, x.edge, y.edge) {
                continue;
            }
            // Every line passes from x to y here exactly once.
            let lines = topo.edges[x.edge].lines.len();
            let through = view
                .chords
                .iter()
                .filter(|c| (c.a == x && c.b == y) || (c.a == y && c.b == x))
                .count();
            if through != lines || view.chords.len() != lines {
                continue;
            }
            let mut seen: Vec<LineId> = view.chords.iter().map(|c| c.line).collect();
            seen.sort_unstable();
            seen.dedup();
            if seen.len() != lines {
                continue;
            }
            let reversed = x.from == y.from;
            let (rx, px) = find(&mut up, &mut odd, x.edge);
            let (ry, py) = find(&mut up, &mut odd, y.edge);
            if rx == ry {
                continue; // a ring: keep it open at this node (its parity may not agree)
            }
            let (lo, hi) = (rx.min(ry), rx.max(ry));
            up[hi] = lo;
            odd[hi] = px ^ py ^ reversed;
        }
        let mut root: IdVec<EdgeId, EdgeId> = vec![EdgeId::new(0); n].into();
        let mut flip: IdVec<EdgeId, bool> = vec![false; n].into();
        let mut members: IdVec<EdgeId, Vec<EdgeId>> = vec![Vec::new(); n].into();
        for e in topo.edges.ids() {
            let (r, p) = find(&mut up, &mut odd, e);
            root[e] = r;
            flip[e] = p;
            members[r].push(e);
        }
        Self {
            root,
            flip,
            members,
        }
    }

    /// Sets every other edge of `e`'s chain from `e`'s order (`e` a root).
    pub(super) fn carry(&self, order: &mut Order, e: EdgeId) {
        for &m in &self.members[e] {
            if m != e {
                let mut o = order[e].clone();
                if self.flip[m] != self.flip[e] {
                    o.reverse();
                }
                order[m] = o;
            }
        }
    }

    /// [`Chains::carry`] for every chain.
    pub(super) fn carry_all(&self, order: &mut Order) {
        for e in self.root.ids() {
            if self.root[e] == e && self.members[e].len() > 1 {
                self.carry(order, e);
            }
        }
    }

    /// The search's states: the product over chains of their line count's factorial, as a base-10
    /// logarithm (how many orders there are to choose from).
    #[cfg(test)]
    pub(super) fn log10_states(&self, order: &Order) -> f64 {
        self.root
            .ids()
            .filter(|&e| self.root[e] == e)
            .map(|e| {
                (1..=order[e].len())
                    .map(|k| (k as f64).log10())
                    .sum::<f64>()
            })
            .sum()
    }
}

/// Whether two edges carry the same lines.
fn same_lines_of(topo: &Topology, a: EdgeId, b: EdgeId) -> bool {
    let mut x: Vec<LineId> = topo.edges[a].lines.iter().map(|&(l, _)| l).collect();
    let mut y: Vec<LineId> = topo.edges[b].lines.iter().map(|&(l, _)| l).collect();
    x.sort_unstable();
    y.sort_unstable();
    x == y
}
