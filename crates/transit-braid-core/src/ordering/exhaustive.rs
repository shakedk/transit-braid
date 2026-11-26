//! Every order of a small part, cheapest first found ([`best_in_part`]).

use std::cmp::Ordering;

use super::chains::Chains;
use super::cost::{NodeView, node_cost, weigh};
use super::{Order, Weights};
use crate::ids::{EdgeId, IdVec, LineId, NodeId};

/// Tries every order of the part's edges (an odometer over each edge's permutations, starting
/// from the current order) and keeps the cheapest, judged on the part's nodes only.
pub(super) fn best_in_part(
    views: &IdVec<NodeId, NodeView>,
    nodes: &[NodeId],
    edges: &[EdgeId],
    mut order: Order,
    w: &Weights,
    chains: &Chains,
) -> Order {
    let part_cost = |o: &Order| {
        let (mut s, mut d, mut p) = (0, 0, 0);
        for &v in nodes {
            let (a, b, c) = node_cost(&views[v], o);
            s += a;
            d += b;
            p += c;
        }
        weigh(s, d, p, w).total
    };
    let start: Vec<Vec<LineId>> = edges.iter().map(|&e| order[e].clone()).collect();
    let mut best = order.clone();
    let mut best_cost = part_cost(&order);
    // Each edge's current permutation as positions into its start order.
    let mut perms: Vec<Vec<usize>> = start.iter().map(|l| (0..l.len()).collect()).collect();
    loop {
        // Next order: advance the odometer, the first edge fastest.
        let mut k = 0;
        loop {
            if k == perms.len() {
                return best;
            }
            if next_permutation(&mut perms[k]) {
                break;
            }
            perms[k].sort_unstable(); // wrapped around: back to the first permutation
            k += 1;
        }
        for (i, &e) in edges.iter().enumerate() {
            order[e] = perms[i].iter().map(|&p| start[i][p]).collect();
            chains.carry(&mut order, e);
        }
        let c = part_cost(&order);
        if c.total_cmp(&best_cost) == Ordering::Less {
            best_cost = c;
            best.clone_from(&order);
        }
    }
}

/// Rearranges `v` into the next permutation in lexicographic order; false (and `v` unchanged)
/// when it is the last one.
pub(super) fn next_permutation(v: &mut [usize]) -> bool {
    let Some(i) = (1..v.len()).rev().find(|&i| v[i - 1] < v[i]) else {
        return false;
    };
    let j = (i..v.len()).rev().find(|&j| v[j] > v[i - 1]).unwrap_or(i);
    v.swap(i - 1, j);
    v[i..].reverse();
    true
}

pub(super) fn factorial(n: usize) -> u64 {
    (1..=n as u64)
        .try_fold(1_u64, u64::checked_mul)
        .unwrap_or(u64::MAX)
}
