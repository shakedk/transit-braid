//! Simulated annealing for parts too large to try every order ([`anneal_part`]): random swaps
//! from a fixed seed ([`SplitMix64`]) over a fixed number of steps, so every machine finds the
//! same order.

use super::chains::Chains;
use super::cost::NodeView;
use super::search::Search;
use super::{Annealing, Order, Weights};
use crate::det;
use crate::ids::{EdgeId, IdVec, NodeId};
use crate::topology::Topology;

/// Temperatures of the annealing, in cost units: from a little above the dearest single cost (a
/// same-segment crossing) down to where no worse move is taken any more.
pub(super) const START_TEMPERATURE: f64 = 3.0;
/// `ln(END / START)` with END = 0.01: how far the temperature falls over the run.
pub(super) const LN_COOLING: f64 = -5.703_782_474_656_201;
/// Share of moves that swap two neighbors (the rest swap any two lines of the edge).
const NEIGHBOR_SWAPS: f64 = 0.8;
/// Share of moves that swap the two lines on their whole shared stretch, not on one edge only.
const RUN_SWAPS: f64 = 0.5;
/// A large part's search: greedy descent, simulated annealing, then a last descent. A move swaps
/// two lines of one edge, either there only or on their whole shared stretch
/// ([`Search::shared_run`]); it is judged on the nodes it touches, and the best order seen is
/// kept.
#[expect(
    clippy::too_many_arguments,
    reason = "one private call; each input is distinct, and a struct for them would only rename them"
)]
pub(super) fn anneal_part(
    topo: &Topology,
    views: &IdVec<NodeId, NodeView>,
    edges: &[EdgeId],
    order: Order,
    w: &Weights,
    a: &Annealing,
    seed: u64,
    chains: &Chains,
) -> Order {
    let lines = order
        .iter()
        .flatten()
        .copied()
        .max()
        .map_or(0, |m| m.index() + 1);
    let mut s = Search::new(topo, views, order, w, lines).with_chains(chains);
    s.descend(edges);
    let slots: u64 = edges.iter().map(|&e| s.order[e].len() as u64).sum();
    let steps = slots
        .saturating_mul(a.steps_per_slot)
        .max(a.min_steps)
        .min(a.max_steps);
    let mut rng = SplitMix64(seed);
    let (mut current, mut best) = (0.0_f64, 0.0_f64); // relative to the start
    let mut best_order = s.order.clone();
    for step in 0..steps {
        // START · (END / START)^(step / steps), with our own exp: the same on every platform.
        let t = START_TEMPERATURE * det::exp(LN_COOLING * step as f64 / steps.max(1) as f64);
        let e = edges[rng.below(edges.len())];
        let n = s.order[e].len();
        let i = rng.below(n - 1);
        let j = if rng.unit() < NEIGHBOR_SWAPS {
            i + 1
        } else {
            rng.below(n)
        };
        if i == j {
            continue;
        }
        let (la, lb) = (s.order[e][i], s.order[e][j]);
        let run = if rng.unit() < RUN_SWAPS {
            s.shared_run(e, la, lb)
        } else {
            s.members[e].clone()
        };
        let delta = s.swap_delta(&run, la, lb);
        if delta <= 0.0 || rng.unit() < det::exp(-delta / t) {
            current += delta;
            if current < best - 1e-9 {
                best = current;
                best_order.clone_from(&s.order);
            }
        } else {
            s.swap(&run, la, lb);
        }
    }
    let mut s = Search::new(topo, views, best_order, w, lines).with_chains(chains);
    s.descend(edges);
    s.order
}

/// A small, fast random generator (SplitMix64): the same seed gives the same moves everywhere.
pub(super) struct SplitMix64(pub(super) u64);

impl SplitMix64 {
    pub(super) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number from 0 to `n - 1` (`n` > 0).
    pub(super) fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// A number in [0, 1).
    pub(super) fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}
