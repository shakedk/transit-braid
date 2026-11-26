//! The order after an edit, warm-started from the order before it ([`solve_warm`]): edges the
//! edit left alone keep their order, and only the parts it touched are searched again.

use super::chains::Chains;
use super::cost::{node_views, views_cost};
use super::exhaustive::{best_in_part, factorial};
use super::search::Search;
use super::{ExhaustiveLimit, Order, Solution, Weights, input_order, nodes_of, parts};
use crate::geom::{Point, path_length};
use crate::ids::{EdgeId, IdVec, LineId};
use crate::topology::{Dir, Topology, Traversal};

/// The graph and order before an edit, for [`solve_warm`].
#[derive(Debug, Clone, Copy)]
pub struct Previous<'a> {
    /// The line graph before the edit.
    pub topo: &'a Topology,
    /// Its order.
    pub order: &'a Order,
    /// How the routes changed ([`crate::topology::route_edit`]).
    pub edit: &'a crate::topology::RouteEdit,
}

/// What [`solve_warm`] found, and how much it solved again.
#[derive(Debug, Clone, PartialEq)]
pub struct WarmSolution {
    /// The order and its cost.
    pub solution: Solution,
    /// Parts with no edge the edit touched: their old order, not solved again.
    pub kept_parts: usize,
    /// Parts with an edge the edit touched: solved again from the old order.
    pub solved_parts: usize,
}

/// A new edge this close (meters) to an old edge with the same lines takes its order:
/// the topology's merge distance.
const SAME_PLACE_M: f64 = 5.0;

/// The order after an edit, warm-started from the order before it, so an edit is quick and moves
/// no line it doesn't touch. An edge the edit left as it was (the same path and the same
/// routes) keeps its old order; another edge takes the old order of an edge with the same
/// routes in the same place, else the input order (an edited route counts as the same route
/// as before). An edge is **touched** when it has no old twin or carries a route the edit
/// changed or added. A part (as in [`solve`](super::solve)) with no touched edge keeps its order:
/// it is not solved again. A part with one is solved again with only its touched edges free:
/// every order of them when at most `limit`, else greedy descent from the warm start. So the edit
/// changes no order on an edge it does not touch. The result depends on the order before; it is
/// deterministic.
#[must_use]
pub fn solve_warm(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    weights: &Weights,
    limit: ExhaustiveLimit,
    previous: &Previous,
) -> WarmSolution {
    let views = node_views(topo, walks);
    let lines = topo
        .edges
        .iter()
        .flat_map(|e| e.lines.iter().map(|&(l, _)| l.index() + 1))
        .max()
        .unwrap_or(0);
    let mut changed: IdVec<LineId, bool> = vec![false; lines].into();
    for &n in &previous.edit.changed {
        if let Some(c) = changed.get_mut(n) {
            *c = true;
        }
    }
    let to_new = |old: LineId| previous.edit.old_to_new.get(old).copied().flatten();

    // Old edges by path and (renumbered) lines, for exact twins.
    let twin_key = |path: &[Point], lines: &[(LineId, Dir)]| -> Vec<u64> {
        let mut key: Vec<u64> = path
            .iter()
            .flat_map(|p| [p.x.to_bits(), p.y.to_bits()])
            .collect();
        key.push(u64::MAX);
        key.extend(
            lines
                .iter()
                .flat_map(|&(l, d)| [l.index() as u64, d as u64]),
        );
        key
    };
    let mut twins: std::collections::HashMap<Vec<u64>, EdgeId> = std::collections::HashMap::new();
    for (k, e) in previous.topo.edges.iter_enumerated() {
        let lines: Option<Vec<(LineId, Dir)>> = e
            .lines
            .iter()
            .map(|&(l, d)| to_new(l).map(|n| (n, d)))
            .collect();
        if let Some(ls) = &lines {
            twins.entry(twin_key(&e.path, ls)).or_insert(k);
        }
    }
    let old_order = |k: EdgeId| -> Option<Vec<LineId>> {
        previous.order.get(k)?.iter().map(|&l| to_new(l)).collect()
    };
    // Each old edge's lines still there, renumbered (removed ones left out).
    let still_there: IdVec<EdgeId, Vec<LineId>> = previous
        .topo
        .edges
        .iter()
        .map(|e| e.lines.iter().filter_map(|&(l, _)| to_new(l)).collect())
        .collect();

    let mut order = input_order(topo);
    let mut touched: IdVec<EdgeId, bool> = vec![false; topo.edges.len()].into();
    for (e, edge) in topo.edges.iter_enumerated() {
        let twin = twins.get(&twin_key(&edge.path, &edge.lines)).copied();
        let warm = match twin {
            Some(k) => old_order(k),
            None => same_place(edge, previous.topo, &still_there, &changed).map(|(k, flip)| {
                // The old order without removed lines, this edge's way round; lines new here
                // (changed or added) go on the left, for the search to place.
                let mut o: Vec<LineId> = previous.order[k]
                    .iter()
                    .filter_map(|&l| to_new(l))
                    .collect();
                if flip {
                    o.reverse();
                }
                for &(l, _) in &edge.lines {
                    if !o.contains(&l) {
                        o.push(l);
                    }
                }
                o
            }),
        };
        if let Some(o) = warm.filter(|o| same_lines(o, &order[e])) {
            order[e] = o;
        }
        touched[e] = twin.is_none()
            || edge
                .lines
                .iter()
                .any(|&(l, _)| changed.get(l).copied().unwrap_or(true));
    }

    let (mut kept_parts, mut solved_parts) = (0, 0);
    for part in parts(topo, &views) {
        let free: Vec<EdgeId> = part
            .iter()
            .copied()
            .filter(|&e| touched[e] && order[e].len() > 1)
            .collect();
        if !part.iter().any(|&e| order[e].len() > 1) {
            continue;
        }
        if free.is_empty() {
            kept_parts += 1;
            continue;
        }
        solved_parts += 1;
        let count = free
            .iter()
            .map(|&e| factorial(order[e].len()))
            .try_fold(1_u64, u64::checked_mul);
        order = match count {
            // Only the free edges' nodes can change cost.
            Some(n) if n <= limit.0 => best_in_part(
                &views,
                &nodes_of(topo, &free),
                &free,
                order,
                weights,
                &Chains::identity(topo.edges.len()),
            ),
            _ => {
                let mut s = Search::new(topo, &views, order, weights, lines);
                for (e, f) in s.frozen.iter_mut_enumerated() {
                    *f = !touched[e];
                }
                s.descend(&free);
                s.order
            }
        };
    }
    let cost = views_cost(&views, &order, weights);
    WarmSolution {
        solution: Solution {
            order,
            cost,
            exhaustive_parts: 0,
            annealed_parts: 0,
        },
        kept_parts,
        solved_parts,
    }
}

/// Whether two orders hold the same lines.
fn same_lines(a: &[LineId], b: &[LineId]) -> bool {
    let (mut x, mut y) = (a.to_vec(), b.to_vec());
    x.sort_unstable();
    y.sort_unstable();
    x == y
}

/// The old edge nearest `edge`'s middle, within [`SAME_PLACE_M`], with the same lines once the
/// edit's own are set aside (changed or added ones here, removed ones there), and whether it
/// runs the other way.
fn same_place(
    edge: &crate::topology::TopoEdge,
    old: &Topology,
    still_there: &IdVec<EdgeId, Vec<LineId>>,
    changed: &IdVec<LineId, bool>,
) -> Option<(EdgeId, bool)> {
    let mine: Vec<LineId> = edge
        .lines
        .iter()
        .map(|&(l, _)| l)
        .filter(|&l| !changed.get(l).copied().unwrap_or(true))
        .collect();
    let (mid, dir) = middle(&edge.path)?;
    old.edges
        .iter_enumerated()
        .filter_map(|(k, e)| {
            let theirs: Vec<LineId> = still_there
                .get(k)?
                .iter()
                .copied()
                .filter(|&l| !changed.get(l).copied().unwrap_or(true))
                .collect();
            if theirs.is_empty() || !same_lines(&theirs, &mine) {
                return None;
            }
            let (d, along) = e
                .path
                .windows(2)
                .map(|w| {
                    let d = crate::topology::distance_to_segment(mid, w[0], w[1]);
                    let along = (w[1].x - w[0].x) * dir.x + (w[1].y - w[0].y) * dir.y;
                    (d, along)
                })
                .min_by(|a, b| a.0.total_cmp(&b.0))?;
            (d <= SAME_PLACE_M).then_some((d, k, along < 0.0))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(_, k, flip)| (k, flip))
}

/// The point halfway along `path`, and the direction there (not normalized).
fn middle(path: &[Point]) -> Option<(Point, Point)> {
    let total = path_length(path);
    let mut left = total / 2.0;
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        if len > 0.0 && left <= len {
            let t = left / len;
            let at = Point::new(
                w[0].x + t * (w[1].x - w[0].x),
                w[0].y + t * (w[1].y - w[0].y),
            );
            return Some((at, Point::new(w[1].x - w[0].x, w[1].y - w[0].y)));
        }
        left -= len;
    }
    None
}
