//! The cost of an order: how each line passes through a node (a chord between two edge ends,
//! [`node_views`]), and what the crossings and separations there cost ([`node_cost`], [`cost`]).

use std::cmp::Ordering;

use super::{Cost, Order, Weights};
use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::topology::{Dir, Topology, Traversal};

/// Where a line passes through a node: from one edge end to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Chord {
    pub(super) line: LineId,
    pub(super) a: End,
    pub(super) b: End,
}

/// One end of an edge: the edge, and whether it is its `from` end (else its `to` end).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct End {
    pub(super) edge: EdgeId,
    pub(super) from: bool,
}

/// A node's edge ends, counterclockwise from east, and the chords through it.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct NodeView {
    pub(super) ends: Vec<End>,
    pub(super) chords: Vec<Chord>,
}

/// Every node's view: its edge ends in counterclockwise order and the lines passing through it.
pub(super) fn node_views(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
) -> IdVec<NodeId, NodeView> {
    let mut views: IdVec<NodeId, NodeView> = vec![
        NodeView {
            ends: Vec::new(),
            chords: Vec::new()
        };
        topo.nodes.len()
    ]
    .into();
    let mut angled: IdVec<NodeId, Vec<(Point, End)>> = vec![Vec::new(); topo.nodes.len()].into();
    for (k, e) in topo.edges.iter_enumerated() {
        let n = e.path.len();
        if n < 2 || topo.nodes.get(e.from).is_none() || topo.nodes.get(e.to).is_none() {
            continue;
        }
        angled[e.from].push((
            leaving(&e.path, false),
            End {
                edge: k,
                from: true,
            },
        ));
        angled[e.to].push((
            leaving(&e.path, true),
            End {
                edge: k,
                from: false,
            },
        ));
    }
    for (v, mut list) in angled.into_iter_enumerated() {
        list.sort_by(|x, y| counterclockwise(x.0, y.0).then(x.1.cmp(&y.1)));
        views[v].ends = list.into_iter().map(|(_, end)| end).collect();
    }
    for (line, walk) in walks.iter_enumerated() {
        for w in walk.windows(2) {
            let ((e1, d1), (e2, d2)) = (w[0], w[1]);
            let (Some(x), Some(y)) = (topo.edges.get(e1), topo.edges.get(e2)) else {
                continue;
            };
            // The step in arrives at the end it runs towards; the step out leaves from its start.
            let a = End {
                edge: e1,
                from: d1 == Dir::Backward,
            };
            let b = End {
                edge: e2,
                from: d2 != Dir::Backward,
            };
            let at_a = if a.from { x.from } else { x.to };
            let at_b = if b.from { y.from } else { y.to };
            if at_a != at_b || a == b {
                continue; // a jump in the walk, or a turn back at the end of an edge
            }
            views[at_a].chords.push(Chord { line, a, b });
        }
    }
    views
}

/// The direction an edge leaves its node in, as a vector: from its first point (or, `at_to`, its
/// last) to the next point that differs.
fn leaving(path: &[Point], at_to: bool) -> Point {
    let pts: Vec<Point> = if at_to {
        path.iter().rev().copied().collect()
    } else {
        path.to_vec()
    };
    let start = pts[0];
    let next = pts.iter().find(|p| **p != start).copied().unwrap_or(start);
    Point::new(next.x - start.x, next.y - start.y)
}

/// Orders directions counterclockwise from east (angle in [0, 2π)), exactly: by half-plane, then
/// by the sign of the cross product. No `atan2`, so every platform sorts alike.
pub(super) fn counterclockwise(a: Point, b: Point) -> Ordering {
    // Upper half: angle in [0, π); lower: [π, 2π). The zero vector counts as east.
    let lower = |p: Point| p.y < 0.0 || (p.y == 0.0 && p.x < 0.0);
    match (lower(a), lower(b)) {
        (false, true) => Ordering::Less,
        (true, false) => Ordering::Greater,
        _ => {
            let cross = a.x * b.y - a.y * b.x;
            if cross > 0.0 {
                Ordering::Less
            } else if cross < 0.0 {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        }
    }
}

/// A line's place around a node: its edge end's position counterclockwise, then its place across
/// that edge from right to left looking out of the node.
fn place(view: &NodeView, order: &Order, end: End, line: LineId) -> Option<usize> {
    let mut before = 0;
    for e in &view.ends {
        let lines = order.get(e.edge)?;
        if *e == end {
            let slot = lines.iter().position(|&l| l == line)?;
            // Out of the `from` end, right to left is slot order; out of the `to` end, reversed.
            let across = if e.from { slot } else { lines.len() - 1 - slot };
            return Some(before + across);
        }
        before += lines.len();
    }
    None
}

/// Whether two lines are next to each other on an edge.
fn adjacent(order: &Order, edge: EdgeId, a: LineId, b: LineId) -> bool {
    let Some(lines) = order.get(edge) else {
        return false;
    };
    match (
        lines.iter().position(|&l| l == a),
        lines.iter().position(|&l| l == b),
    ) {
        (Some(i), Some(j)) => i.abs_diff(j) == 1,
        _ => false,
    }
}

/// The cost of one node.
pub(super) fn node_cost(view: &NodeView, order: &Order) -> (usize, usize, usize) {
    let (mut same, mut diff, mut sep) = (0, 0, 0);
    let spans: Vec<Option<(usize, usize)>> = view
        .chords
        .iter()
        .map(|c| {
            let p = place(view, order, c.a, c.line)?;
            let q = place(view, order, c.b, c.line)?;
            Some((p.min(q), p.max(q)))
        })
        .collect();
    for i in 0..view.chords.len() {
        for j in i + 1..view.chords.len() {
            let (c, d) = (view.chords[i], view.chords[j]);
            if c.line == d.line {
                continue;
            }
            let shared = [c.a, c.b].iter().any(|x| *x == d.a || *x == d.b);
            if !shared {
                continue; // they share no edge here: their crossing doesn't depend on the order
            }
            let together = (c.a == d.a && c.b == d.b) || (c.a == d.b && c.b == d.a);
            if let (Some((p1, q1)), Some((p2, q2))) = (spans[i], spans[j]) {
                let interleave = (p1 < p2 && p2 < q1 && q1 < q2) || (p2 < p1 && p1 < q2 && q2 < q1);
                if interleave {
                    if together {
                        same += 1;
                    } else {
                        diff += 1;
                    }
                }
            }
            if together
                && adjacent(order, c.a.edge, c.line, d.line)
                    != adjacent(order, c.b.edge, c.line, d.line)
            {
                sep += 1;
            }
        }
    }
    (same, diff, sep)
}

pub(super) fn weigh(same: usize, diff: usize, sep: usize, w: &Weights) -> Cost {
    Cost {
        same_segment: same,
        different_segment: diff,
        separations: sep,
        total: w.same_segment * same as f64
            + w.different_segment * diff as f64
            + w.separation * sep as f64,
    }
}

/// The cost of `order` on the whole graph. `order` must list, for each edge, exactly its lines;
/// a line missing from it makes its chords cost nothing.
#[must_use]
pub fn cost(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    order: &Order,
    weights: &Weights,
) -> Cost {
    views_cost(&node_views(topo, walks), order, weights)
}

/// The cost of `order` at every node of `views` ([`cost`] with the views already made).
pub(super) fn views_cost(
    views: &IdVec<NodeId, NodeView>,
    order: &Order,
    weights: &Weights,
) -> Cost {
    let (mut same, mut diff, mut sep) = (0, 0, 0);
    for view in views {
        let (s, d, p) = node_cost(view, order);
        same += s;
        diff += d;
        sep += p;
    }
    weigh(same, diff, sep, weights)
}
