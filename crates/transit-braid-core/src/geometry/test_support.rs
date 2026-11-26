//! Small graphs shared by the geometry's tests.

use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::ordering::Order;
use crate::topology::{Dir, TopoEdge, Topology, Traversal};

pub(super) const F: Dir = Dir::Forward;

/// An edge between nodes by their place in the graph, carrying lines by their number.
pub(super) fn edge(from: usize, to: usize, path: &[(f64, f64)], lines: &[usize]) -> TopoEdge {
    TopoEdge {
        from: NodeId::new(from),
        to: NodeId::new(to),
        path: path.iter().map(|&(x, y)| Point::new(x, y)).collect(),
        lines: lines.iter().map(|&l| (LineId::new(l), F)).collect(),
    }
}

/// A straight edge from (0, 0) to (300, 0) carrying `lines`.
pub(super) fn street(lines: &[usize]) -> Topology {
    Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(300.0, 0.0)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), (300.0, 0.0)], lines)].into(),
    }
}

pub(super) fn pts(xy: &[(f64, f64)]) -> Vec<Point> {
    xy.iter().map(|&(x, y)| Point::new(x, y)).collect()
}

/// An order from lines by their number: `order[edge]` lists its lines from right to left.
pub(super) fn order_of(o: Vec<Vec<usize>>) -> Order {
    o.into_iter()
        .map(|ls| ls.into_iter().map(LineId::new).collect())
        .collect()
}

/// Walks from edges by their place in the graph: `walks[line]` lists its `(edge, dir)` steps.
pub(super) fn walks_of(w: Vec<Vec<(usize, Dir)>>) -> IdVec<LineId, Traversal> {
    w.into_iter()
        .map(|walk| walk.into_iter().map(|(e, d)| (EdgeId::new(e), d)).collect())
        .collect()
}
