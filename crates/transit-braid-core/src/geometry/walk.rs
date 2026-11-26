//! One route's output along its walk ([`walk_line`]): each edge's piece in the line's slot,
//! cut back by the node fronts and joined across each node by a smooth [`connector`].

use super::fronts::unit_leaving;
use super::loops::{LOOP_SLACK_M, NODE_LOOP_REACH_FRONTS, cut_node_loop, without_offset_loops};
use super::slot_offset;
use crate::geom::{Point, path_length};
use crate::ids::{EdgeId, IdVec, LineId, NodeId};
use crate::ordering::Order;
use crate::polyline::{DEFAULT_MITER_LIMIT, biarc, offset_polyline_within};
use crate::topology::{Dir, Topology, Traversal};

/// The part of `path` from `a` to `b` meters along it (`0 ≤ a < b ≤ its length`), without
/// repeated points.
///
/// Not the topology's `sub_path`: that one keeps repeated points and always returns at least two
/// points; merging them would change one of the two outputs.
pub(super) fn sub_path(path: &[Point], a: f64, b: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let mut at = 0.0;
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        let point = |d: f64| {
            let t = if len == 0.0 {
                0.0
            } else {
                ((d - at) / len).clamp(0.0, 1.0)
            };
            Point::new(
                w[0].x + t * (w[1].x - w[0].x),
                w[0].y + t * (w[1].y - w[0].y),
            )
        };
        if out.is_empty() && a <= at + len {
            out.push(point(a));
        }
        if !out.is_empty() {
            if b <= at + len {
                out.push(point(b));
                break;
            }
            out.push(w[1]);
        }
        at += len;
    }
    out.dedup();
    out
}

/// Points of a smooth connector from `p0`, leaving along `t0`, to `p3`, arriving along `t1`,
/// without its first point; empty when the two ends meet. A [`biarc`] (two circular arcs, one
/// tangent where they meet) when one fits and the gap is at most three fronts (`max_handle`);
/// else a cubic Bézier with handles a third of the gap long, but at most `max_handle`. The cap
/// keeps a connector over a gap much wider than its node (a walk that jumps) close to straight
/// instead of bulging out.
pub(super) fn connector(p0: Point, t0: Point, p3: Point, t1: Point, max_handle: f64) -> Vec<Point> {
    let gap = p0.distance(p3);
    if gap < 1e-9 {
        return Vec::new();
    }
    // Two arcs with one tangent where they meet, when they fit and the gap is the node's
    // own (a few fronts); else the capped cubic below.
    if gap <= 3.0 * max_handle
        && let Some(arcs) = biarc(p0, t0, p3, t1)
    {
        return arcs;
    }
    let h = (gap / 3.0).min(max_handle.max(0.0));
    let p1 = Point::new(p0.x + t0.x * h, p0.y + t0.y * h);
    let p2 = Point::new(p3.x - t1.x * h, p3.y - t1.y * h);
    (1..=CONNECTOR_STEPS)
        .map(|k| {
            let t = k as f64 / CONNECTOR_STEPS as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            Point::new(
                a * p0.x + b * p1.x + c * p2.x + d * p3.x,
                a * p0.y + b * p1.y + c * p2.y + d * p3.y,
            )
        })
        .collect()
}

/// Points per connector.
pub(super) const CONNECTOR_STEPS: usize = 8;

/// The unit direction of a polyline's last segment (or, `first`, its first), or east.
fn end_direction(path: &[Point], first: bool) -> Point {
    let d = if first {
        unit_leaving(path, false)
    } else {
        unit_leaving(path, true).map(|d| Point::new(-d.x, -d.y))
    };
    d.unwrap_or(Point::new(1.0, 0.0))
}

/// One route's output (see [`lines`](super::lines)), or None when no step of its walk can be
/// drawn.
#[expect(
    clippy::too_many_arguments,
    reason = "one private call; each input is distinct, and a struct for them would only rename them"
)]
pub(super) fn walk_line(
    topo: &Topology,
    centers: &IdVec<EdgeId, Vec<Point>>,
    order: &Order,
    fronts: &IdVec<NodeId, f64>,
    pitch_m: f64,
    room_m: f64,
    walk: &Traversal,
    line: LineId,
) -> Option<Vec<Point>> {
    let mut out: Vec<Point> = Vec::new();
    let last = walk.len().saturating_sub(1);
    for (k, &(e, dir)) in walk.iter().enumerate() {
        let Some(edge) = topo.edges.get(e) else {
            continue;
        };
        let lines = order.get(e).map_or(&[][..], Vec::as_slice);
        let Some(slot) = lines.iter().position(|&l| l == line) else {
            continue;
        };
        let forward = dir != Dir::Backward;
        // The walk's own start and end are not cut back, so the line reaches its ends.
        let open_from = (forward && k == 0) || (!forward && k == last);
        let open_to = (forward && k == last) || (!forward && k == 0);
        let path = centers.get(e).map_or(&edge.path[..], Vec::as_slice);
        let len = path_length(path);
        let mut cut_from = if open_from {
            0.0
        } else {
            fronts.get(edge.from).copied().unwrap_or(0.0)
        };
        let mut cut_to = if open_to {
            0.0
        } else {
            fronts.get(edge.to).copied().unwrap_or(0.0)
        };
        if cut_from + cut_to > 0.9 * len {
            let k = 0.9 * len / (cut_from + cut_to);
            cut_from *= k;
            cut_to *= k;
        }
        let center = sub_path(path, cut_from, len - cut_to);
        let Some(mut piece) = offset_polyline_within(
            &center,
            slot_offset(slot, lines.len()) * pitch_m,
            DEFAULT_MITER_LIMIT,
            room_m,
        ) else {
            continue;
        };
        piece = without_offset_loops(piece, &center, slot_offset(slot, lines.len()) * pitch_m);
        if !forward {
            piece.reverse();
        }
        let mut joint = None;
        if let (Some(&p0), Some(&p3)) = (out.last(), piece.first()) {
            let t0 = end_direction(&out, false);
            let t1 = end_direction(&piece, true);
            // The node between the two steps: where this step starts.
            let node = if forward { edge.from } else { edge.to };
            let front = fronts.get(node).copied().unwrap_or(0.0);
            let before = out.len() - 1;
            out.extend(connector(p0, t0, p3, t1, front));
            joint = Some((before, out.len(), front));
        }
        for p in piece {
            if out.last() != Some(&p) {
                out.push(p);
            }
        }
        if let Some((from, to, front)) = joint {
            cut_node_loop(
                &mut out,
                from,
                to,
                NODE_LOOP_REACH_FRONTS * front + LOOP_SLACK_M,
            );
        }
    }
    (out.len() >= 2).then_some(out)
}
