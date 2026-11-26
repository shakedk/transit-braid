//! Small graphs and random inputs shared by the ordering's tests.

use proptest::prelude::*;

use super::*;
use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::topology::{Dir, TopoEdge};

pub(super) const A: LineId = LineId::new(0);
pub(super) const B: LineId = LineId::new(1);
pub(super) const C: LineId = LineId::new(2);
pub(super) const X: LineId = LineId::new(3);
pub(super) const F: Dir = Dir::Forward;

/// A graph from node positions and straight edges `(from, to, lines)`, nodes by their place in
/// `nodes`.
pub(super) fn graph(nodes: &[(f64, f64)], edges: &[(usize, usize, &[LineId])]) -> Topology {
    let nodes: IdVec<NodeId, Point> = nodes.iter().map(|&(x, y)| Point::new(x, y)).collect();
    let edges = edges
        .iter()
        .map(|&(from, to, lines)| {
            let (from, to) = (NodeId::new(from), NodeId::new(to));
            TopoEdge {
                from,
                to,
                path: vec![nodes[from], nodes[to]],
                lines: lines.iter().map(|&l| (l, F)).collect(),
            }
        })
        .collect();
    Topology { nodes, edges }
}

/// Walks from edges by their place in the graph: `walks[line]` lists its `(edge, dir)` steps.
pub(super) fn walks_of(w: Vec<Vec<(usize, Dir)>>) -> IdVec<LineId, Traversal> {
    w.into_iter()
        .map(|walk| walk.into_iter().map(|(e, d)| (EdgeId::new(e), d)).collect())
        .collect()
}

/// Edges by their place in the graph.
pub(super) fn edge_ids(ids: &[usize]) -> Vec<EdgeId> {
    ids.iter().copied().map(EdgeId::new).collect()
}

pub(super) fn w() -> Weights {
    Weights::default()
}

pub(super) fn costs(
    t: &Topology,
    walks: &IdVec<LineId, Traversal>,
    order: &Order,
) -> (usize, usize, usize) {
    let c = cost(t, walks, order, &w());
    (c.same_segment, c.different_segment, c.separations)
}

/// Y: a trunk S→J carrying A, B, C; after J, B turns north-east, A goes east, C south-east.
pub(super) fn y() -> (Topology, IdVec<LineId, Traversal>) {
    let t = graph(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (200.0, 100.0),
            (200.0, 0.0),
            (200.0, -100.0),
        ],
        &[(0, 1, &[A, B, C]), (1, 2, &[B]), (1, 3, &[A]), (1, 4, &[C])],
    );
    let walks = walks_of(vec![
        vec![(0, F), (2, F)],
        vec![(0, F), (1, F)],
        vec![(0, F), (3, F)],
    ]);
    (t, walks)
}

/// X: A runs north-west → south-east and B south-west → north-east, sharing the middle edge.
pub(super) fn x() -> (Topology, IdVec<LineId, Traversal>) {
    let t = graph(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (-100.0, 100.0),
            (-100.0, -100.0),
            (200.0, -100.0),
            (200.0, 100.0),
        ],
        &[
            (0, 1, &[A, B]),
            (2, 0, &[A]),
            (3, 0, &[B]),
            (1, 4, &[A]),
            (1, 5, &[B]),
        ],
    );
    let walks = walks_of(vec![
        vec![(1, F), (0, F), (3, F)],
        vec![(2, F), (0, F), (4, F)],
    ]);
    (t, walks)
}

/// Dog-bone: A comes from the north-west and leaves to the north-east, B from the south-west to
/// the south-east; they share the middle edge.
pub(super) fn dog_bone() -> (Topology, IdVec<LineId, Traversal>) {
    let t = graph(
        &[
            (0.0, 0.0),
            (100.0, 0.0),
            (-100.0, 100.0),
            (-100.0, -100.0),
            (200.0, 100.0),
            (200.0, -100.0),
        ],
        &[
            (0, 1, &[A, B]),
            (2, 0, &[A]),
            (3, 0, &[B]),
            (1, 4, &[A]),
            (1, 5, &[B]),
        ],
    );
    let walks = walks_of(vec![
        vec![(1, F), (0, F), (3, F)],
        vec![(2, F), (0, F), (4, F)],
    ]);
    (t, walks)
}

/// Two edges in a row, both carrying A and B, which run through.
pub(super) fn straight() -> (Topology, IdVec<LineId, Traversal>) {
    let t = graph(
        &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)],
        &[(0, 1, &[A, B]), (1, 2, &[A, B])],
    );
    let walks = walks_of(vec![vec![(0, F), (1, F)], vec![(0, F), (1, F)]]);
    (t, walks)
}

/// Random small graphs: a star of `n` arms around one node, lines running between arms.
pub(super) fn star() -> impl Strategy<Value = (Topology, IdVec<LineId, Traversal>)> {
    (
        3_usize..6,
        prop::collection::vec((0_usize..6, 0_usize..6), 1..5),
    )
        .prop_map(|(arms, pairs)| {
            let mut nodes = vec![(0.0, 0.0)];
            for k in 0..arms {
                let a = k as f64 * std::f64::consts::TAU / arms as f64;
                nodes.push((100.0 * a.cos(), 100.0 * a.sin()));
            }
            let mut lines_on: Vec<Vec<usize>> = vec![Vec::new(); arms];
            let mut walks: Vec<Vec<(usize, Dir)>> = Vec::new();
            for (line, &(p, q)) in pairs.iter().enumerate() {
                let (p, q) = (p % arms, q % arms);
                if p == q {
                    walks.push(vec![]);
                    continue;
                }
                lines_on[p].push(line);
                lines_on[q].push(line);
                // In along arm p (towards the center: Backward), out along arm q (Forward).
                walks.push(vec![(p, Dir::Backward), (q, F)]);
            }
            let nodes_p: Vec<Point> = nodes.iter().map(|&(x, y)| Point::new(x, y)).collect();
            let edges = (0..arms)
                .map(|k| TopoEdge {
                    from: NodeId::new(0),
                    to: NodeId::new(k + 1),
                    path: vec![nodes_p[0], nodes_p[k + 1]],
                    lines: lines_on[k].iter().map(|&l| (LineId::new(l), F)).collect(),
                })
                .collect();
            (
                Topology {
                    nodes: nodes_p.into(),
                    edges,
                },
                walks_of(walks),
            )
        })
}

/// A corridor of `nodes` stops along the x axis, each with a north and a south arm; lines
/// come in on one stop's arm, run along the corridor and leave on a later stop's arm.
pub(super) fn corridor() -> impl Strategy<Value = (Topology, IdVec<LineId, Traversal>)> {
    let line = (0_usize..4, 0_usize..4, any::<bool>(), any::<bool>());
    (3_usize..5, prop::collection::vec(line, 2..5)).prop_map(|(stops, picks)| {
        let mut nodes: Vec<Point> = (0..stops)
            .map(|i| Point::new(100.0 * i as f64, 0.0))
            .collect();
        // Edge ids: corridor edges 0..stops-1, then arms: stop i north = c + 2i, south = c + 2i + 1.
        let c = stops - 1;
        let mut ends: Vec<(usize, usize)> = (0..c).map(|i| (i, i + 1)).collect();
        for i in 0..stops {
            for dy in [100.0, -100.0] {
                nodes.push(Point::new(100.0 * i as f64 + 30.0, dy));
                ends.push((i, nodes.len() - 1));
            }
        }
        let mut on: Vec<Vec<usize>> = vec![Vec::new(); ends.len()];
        let mut walks: Vec<Vec<(usize, Dir)>> = Vec::new();
        for (line, &(a, b, north_in, north_out)) in picks.iter().enumerate() {
            let (i, j) = (a % stops, b % stops);
            if i >= j {
                walks.push(vec![]);
                continue;
            }
            let arm = |stop: usize, north: bool| c + 2 * stop + usize::from(!north);
            let mut walk = vec![(arm(i, north_in), Dir::Backward)];
            walk.extend((i..j).map(|k| (k, F)));
            walk.push((arm(j, north_out), F));
            for &(e, _) in &walk {
                on[e].push(line);
            }
            walks.push(walk);
        }
        let edges = ends
            .iter()
            .zip(&on)
            .map(|(&(from, to), lines)| TopoEdge {
                from: NodeId::new(from),
                to: NodeId::new(to),
                path: vec![nodes[from], nodes[to]],
                lines: lines.iter().map(|&l| (LineId::new(l), F)).collect(),
            })
            .collect();
        (
            Topology {
                nodes: nodes.into(),
                edges,
            },
            walks_of(walks),
        )
    })
}

/// One to six routes along a 100 m street grid, jittered under 2 m.
pub(super) fn grid_walks() -> impl Strategy<Value = Vec<Vec<Point>>> {
    let walk = (
        (0..4_i32, 0..4_i32),
        prop::collection::vec(0..4_usize, 1..5),
        (-1.5..1.5_f64, -1.5..1.5_f64),
    )
        .prop_map(|((x, y), moves, (jx, jy))| {
            let mut at = (x, y);
            let mut out = vec![Point::new(
                f64::from(x) * 100.0 + jx,
                f64::from(y) * 100.0 + jy,
            )];
            for m in moves {
                let (dx, dy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][m];
                at = (at.0 + dx, at.1 + dy);
                out.push(Point::new(
                    f64::from(at.0) * 100.0 + jx,
                    f64::from(at.1) * 100.0 + jy,
                ));
            }
            out
        });
    prop::collection::vec(walk, 1..7)
}

/// Each edge's order holds exactly its lines.
pub(super) fn valid(t: &Topology, order: &Order) -> bool {
    order.len() == t.edges.len()
        && t.edges.iter().zip(order).all(|(e, o)| {
            let mut a: Vec<LineId> = e.lines.iter().map(|&(l, _)| l).collect();
            let mut b = o.clone();
            a.sort_unstable();
            b.sort_unstable();
            a == b
        })
}
