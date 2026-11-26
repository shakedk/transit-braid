//! Line ordering: which side of each edge every line takes, so lines cross and separate as
//! little as possible. The cost and the search follow the published line-ordering work of Bast,
//! Brosi and Storandt.
//!
//! An **order** gives each edge its lines from right to left, looking from the edge's `from` node
//! to its `to` node: slot 0 is the rightmost. Its cost is counted at every node from how lines
//! pass through it ([`Traversal`]s):
//!
//! - Around a node, every edge end holds its lines side by side. Going counterclockwise around
//!   the node, the edges come in the order they leave it, and each edge's lines from its right to
//!   its left (looking out of the node). A line passing through the node joins two of these
//!   places: a chord. Two lines cross at the node exactly when their chords interleave.
//! - A **same-segment crossing**: the two lines come from the same edge and go on to the same
//!   edge (they swap sides while staying together). A **different-segment crossing**: any other
//!   crossing of two lines that share at least one of their two edges there.
//! - A **separation**: two lines that come from the same edge and go on to the same edge are
//!   next to each other on one of them but not on the other.
//!
//! Cost = `same_segment` × same-segment crossings + `different_segment` × different-segment
//! crossings + `separation` × separations ([`Weights`]). Lines that end at a node, or turn back
//! at it (out-and-back), make no chord there.
//!
//! [`solve`] splits the graph into independent parts (edges linked by lines passing from one to
//! the other) and tries every order of a part when there are at most [`ExhaustiveLimit`] of
//! them. Larger parts get a greedy descent, then simulated annealing, then a last descent
//! ([`Annealing`]): a fixed number of steps from a fixed seed, so the result is the same on every
//! machine.
//!
//! The parts live in their own files: `cost` (how lines pass through a node, and what an order
//! costs there), `exhaustive` (every order of a small part), `search` and `anneal` (greedy
//! descent and simulated annealing for large parts), `chains` (edges joined at pass-through
//! nodes share one order) and `warm` (the order after an edit, from the order before).

mod anneal;
mod chains;
mod cost;
mod exhaustive;
mod search;
mod warm;

pub use cost::cost;
pub use warm::{Previous, WarmSolution, solve_warm};

use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::topology::{Topology, Traversal};

use anneal::anneal_part;
use chains::Chains;
use cost::{NodeView, node_views, views_cost};
use exhaustive::{best_in_part, factorial};

/// Weights of the three costs. The defaults are our own: a twist of two lines running together
/// costs most, a separation less, any other crossing least.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weights {
    /// A crossing of two lines that run together on both edges at the node.
    pub same_segment: f64,
    /// A crossing of two lines that share only one of their edges at the node.
    pub different_segment: f64,
    /// Two lines next to each other on one edge but not on the next one they share.
    pub separation: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            same_segment: 3.0,
            different_segment: 1.0,
            separation: 2.0,
        }
    }
}

/// The most orders [`solve`] tries in one part (10⁵ by default).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExhaustiveLimit(pub u64);

impl Default for ExhaustiveLimit {
    fn default() -> Self {
        Self(100_000)
    }
}

/// How [`solve`] searches parts too large to try every order. Steps, not time, bound it, so
/// the same input and seed give the same order on every machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Annealing {
    /// Seed of the random moves.
    pub seed: u64,
    /// Steps per line slot in the part (the sum of its edges' line counts).
    pub steps_per_slot: u64,
    /// The most steps for one part.
    pub max_steps: u64,
    /// The fewest steps for one part (small parts are cheap, and few steps can stall in a local
    /// minimum); never more than `max_steps`.
    pub min_steps: u64,
}

/// The default budget, for a good first draw: 200 steps per line slot, at
/// most 100 000, so a 50-line city gets the full 100 000 (its parts have 600–930 slots). Only a
/// cold solve anneals (the first draw, or a full rebuild); edits are warm and never do. Measured
/// against 20 000 steps, mean of 5 seeds: the cities' order cost 22–30% lower, about +190 ms native
/// on Dublin's first draw.
impl Default for Annealing {
    fn default() -> Self {
        Self {
            seed: 0x5EED,
            steps_per_slot: 200,
            max_steps: 100_000,
            min_steps: 5_000,
        }
    }
}

/// An order: for each edge, its lines from right to left, looking from its `from` node to its
/// `to` node.
pub type Order = IdVec<EdgeId, Vec<LineId>>;

/// What an order costs, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cost {
    /// Same-segment crossings.
    pub same_segment: usize,
    /// Different-segment crossings.
    pub different_segment: usize,
    /// Separations.
    pub separations: usize,
    /// The weighted total.
    pub total: f64,
}

/// What [`solve`] found.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    /// The order.
    pub order: Order,
    /// Its cost.
    pub cost: Cost,
    /// Parts solved by trying every order.
    pub exhaustive_parts: usize,
    /// Parts too large to try every order, solved by greedy descent and annealing.
    pub annealed_parts: usize,
}

/// The input order: each edge's lines as the edge lists them (by line).
#[must_use]
pub fn input_order(topo: &Topology) -> Order {
    topo.edges
        .iter()
        .map(|e| e.lines.iter().map(|&(l, _)| l).collect())
        .collect()
}

/// The best order: each independent part with at most `limit` orders gets its cheapest one (the
/// first found, trying orders from the input order on, so ties keep the input order); larger
/// parts are searched with [`Annealing::default`]. Deterministic.
#[must_use]
pub fn solve(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    weights: &Weights,
    limit: ExhaustiveLimit,
) -> Solution {
    solve_with(topo, walks, weights, limit, &Annealing::default())
}

/// [`solve`] with the annealing settings given.
#[must_use]
pub fn solve_with(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    weights: &Weights,
    limit: ExhaustiveLimit,
    annealing: &Annealing,
) -> Solution {
    solve_inner(topo, walks, weights, limit, annealing, true)
}

/// [`solve_with`], with chain contraction on or off: off is the plain search, kept for the tests
/// that check contraction finds the same optimum.
fn solve_inner(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    weights: &Weights,
    limit: ExhaustiveLimit,
    annealing: &Annealing,
    contract: bool,
) -> Solution {
    let views = node_views(topo, walks);
    let chains = if contract {
        Chains::new(topo, &views)
    } else {
        Chains::identity(topo.edges.len())
    };
    let mut order = input_order(topo);
    chains.carry_all(&mut order);
    let (mut exhaustive_parts, mut annealed_parts) = (0, 0);
    for (index, part) in parts(topo, &views).into_iter().enumerate() {
        let nodes = nodes_of(topo, &part);
        // One order per chain: its root's (the others carry it).
        let multi: Vec<EdgeId> = part
            .iter()
            .copied()
            .filter(|&e| order[e].len() > 1 && chains.root[e] == e)
            .collect();
        if multi.is_empty() {
            continue;
        }
        let count = multi
            .iter()
            .map(|&e| factorial(order[e].len()))
            .try_fold(1_u64, u64::checked_mul);
        match count {
            Some(n) if n <= limit.0 => {
                exhaustive_parts += 1;
                order = best_in_part(&views, &nodes, &multi, order, weights, &chains);
            }
            _ => {
                annealed_parts += 1;
                let seed = annealing.seed ^ (index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                order = anneal_part(
                    topo, &views, &multi, order, weights, annealing, seed, &chains,
                );
            }
        }
    }
    let cost = views_cost(&views, &order, weights);
    Solution {
        order,
        cost,
        exhaustive_parts,
        annealed_parts,
    }
}

/// Independent parts: edges linked when some line passes from one to the other at a node. Each
/// part's edges sorted; parts in order of their first edge.
fn parts(topo: &Topology, views: &IdVec<NodeId, NodeView>) -> Vec<Vec<EdgeId>> {
    let mut parent: IdVec<EdgeId, EdgeId> = topo.edges.ids().collect();
    fn find(parent: &mut IdVec<EdgeId, EdgeId>, mut x: EdgeId) -> EdgeId {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for view in views {
        for c in &view.chords {
            let (a, b) = (find(&mut parent, c.a.edge), find(&mut parent, c.b.edge));
            if a != b {
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let mut by_root: std::collections::BTreeMap<EdgeId, Vec<EdgeId>> =
        std::collections::BTreeMap::new();
    for e in topo.edges.ids() {
        let r = find(&mut parent, e);
        by_root.entry(r).or_default().push(e);
    }
    by_root.into_values().collect()
}

/// The nodes touching any of `edges`, sorted.
fn nodes_of(topo: &Topology, edges: &[EdgeId]) -> Vec<NodeId> {
    let mut nodes: Vec<NodeId> = edges
        .iter()
        .flat_map(|&e| [topo.edges[e].from, topo.edges[e].to])
        .collect();
    nodes.sort_unstable();
    nodes.dedup();
    nodes
}

#[cfg(test)]
mod chains_tests;
#[cfg(test)]
mod cost_tests;
#[cfg(test)]
mod solve_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod warm_tests;
