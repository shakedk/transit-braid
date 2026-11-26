//! Node fronts ([`node_fronts`]): how far each node cuts its edges back, so lines have room to
//! turn and change slots there.

use crate::geom::{Point, path_length};
use crate::ids::{IdVec, NodeId};
use crate::ordering::Order;
use crate::topology::Topology;

/// How far each node's fronts cut its edges back, in meters: room for lines to turn and
/// change slots without crossing the other edges' lines. For a node where edges meet at a smallest
/// angle φ, with the widest bundle there `W` wide (lines × pitch): `W/2 / tan(φ/2)`, at least `W/2`,
/// and at most 45% of its shortest edge. Nodes with one edge (a route's end) cut nothing. Exact
/// arithmetic only (`tan(φ/2) = |a × b| / (1 + a · b)` for unit directions), so every platform
/// cuts alike.
#[must_use]
pub fn node_fronts(topo: &Topology, order: &Order, pitch_m: f64) -> IdVec<NodeId, f64> {
    let mut leaving: IdVec<NodeId, Vec<(Point, f64, usize)>> =
        vec![Vec::new(); topo.nodes.len()].into();
    for (e, edge) in topo.edges.iter_enumerated() {
        let n = order.get(e).map_or(0, Vec::len);
        let len = path_length(&edge.path);
        for (v, at_to) in [(edge.from, false), (edge.to, true)] {
            if let (Some(list), Some(d)) = (leaving.get_mut(v), unit_leaving(&edge.path, at_to)) {
                list.push((d, len, n));
            }
        }
    }
    leaving
        .iter()
        .map(|ends| {
            if ends.len() < 2 {
                return 0.0;
            }
            let half_width =
                ends.iter().map(|&(_, _, n)| n).max().unwrap_or(0) as f64 * pitch_m / 2.0;
            let shortest = ends
                .iter()
                .map(|&(_, len, _)| len)
                .fold(f64::INFINITY, f64::min);
            let mut front = half_width;
            for (i, &(a, _, _)) in ends.iter().enumerate() {
                for &(b, _, _) in &ends[i + 1..] {
                    let cos = a.x * b.x + a.y * b.y;
                    let sin = (a.x * b.y - a.y * b.x).abs();
                    // W/2 / tan(φ/2) = W/2 · (1 + cos φ) / sin φ. Parallel edges: straight on
                    // (opposite ways) needs nothing more; the same way needs all the room there is.
                    let need = if sin > 0.0 {
                        half_width * (1.0 + cos) / sin
                    } else if cos < 0.0 {
                        0.0
                    } else {
                        f64::INFINITY
                    };
                    front = front.max(need);
                }
            }
            front.min(0.45 * shortest)
        })
        .collect()
}

/// The unit direction an edge leaves its node in (from its first point, or `at_to` its last, to
/// the next point that differs); None when all its points are the same.
pub(super) fn unit_leaving(path: &[Point], at_to: bool) -> Option<Point> {
    let start = if at_to { *path.last()? } else { *path.first()? };
    let mut rest: Box<dyn Iterator<Item = &Point>> = if at_to {
        Box::new(path.iter().rev())
    } else {
        Box::new(path.iter())
    };
    let next = rest.find(|p| **p != start)?;
    let (dx, dy) = (next.x - start.x, next.y - start.y);
    let len = crate::geom::length(dx, dy);
    Some(Point::new(dx / len, dy / len))
}
