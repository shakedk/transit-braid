//! Station bars ([`station_bars`]): where each station's bar goes (same-name stops merged,
//! [`merge_named_stations_on_edges`]) and the bar across the whole bundle there.

use std::collections::BTreeMap;

use super::fronts::node_fronts;
use crate::geom::{Point, path_length};
use crate::ids::EdgeId;
use crate::ordering::Order;
use crate::topology::Topology;

/// Stops with the same name closer than this (meters, chained) are one station, so they get
/// one bar: a stop pair on both sides of a street, or a stop and its twin across a junction.
pub const STATION_MERGE_M: f64 = 50.0;

/// Where each station's bar goes. Stations with the same nonzero `group` (the same name)
/// within [`STATION_MERGE_M`] of each other, chained, all move to their average; a station with
/// group 0 (no name), or alone in its group, stays where it is. One place per station, in order,
/// so each still gets its own bar (merged ones at the same place). `groups` shorter than
/// `stations` counts the rest as 0. Plain arithmetic, so every platform merges alike.
/// The pipeline uses [`merge_named_stations_on_edges`], which also needs the same street piece.
#[must_use]
pub fn merge_named_stations(stations: &[Point], groups: &[u32]) -> Vec<Point> {
    merge_where(stations, groups, |_, _| true)
}

/// [`merge_named_stations`], merging only stations that snap to the same edge, so each side of a
/// split street keeps its own bar. A same-name stop pair on a boulevard's two carriageways, or on a
/// one-way pair, sits on two edges, so each keeps its own bar across its own bundle; a pair on
/// both sides of one bundle still shares one. A station that snaps to no edge (farther than
/// [`STATION_SNAP_M`], or not finite) merges with nothing.
#[must_use]
pub fn merge_named_stations_on_edges(
    topo: &Topology,
    order: &Order,
    stations: &[Point],
    groups: &[u32],
) -> Vec<Point> {
    let edge_of: Vec<Option<EdgeId>> = stations
        .iter()
        .map(|&s| nearest_edge(topo, order, s).map(|(_, e, _)| e))
        .collect();
    merge_where(stations, groups, |a, b| {
        edge_of[a].is_some() && edge_of[a] == edge_of[b]
    })
}

/// Merges same-name stations within [`STATION_MERGE_M`] of each other (chained) for which
/// `can_join(a, b)` holds (station indices), each group to its average.
fn merge_where(
    stations: &[Point],
    groups: &[u32],
    can_join: impl Fn(usize, usize) -> bool,
) -> Vec<Point> {
    let mut by_group: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, &g) in groups.iter().enumerate().take(stations.len()) {
        if g != 0 {
            by_group.entry(g).or_default().push(i);
        }
    }
    let mut out = stations.to_vec();
    for members in by_group.values() {
        // Union-find over this name's stations: two within STATION_MERGE_M join.
        let mut root: Vec<usize> = (0..members.len()).collect();
        let find = |root: &mut Vec<usize>, mut a: usize| {
            while root[a] != a {
                root[a] = root[root[a]];
                a = root[a];
            }
            a
        };
        for a in 0..members.len() {
            for b in a + 1..members.len() {
                if stations[members[a]].distance(stations[members[b]]) <= STATION_MERGE_M
                    && can_join(members[a], members[b])
                {
                    let (ra, rb) = (find(&mut root, a), find(&mut root, b));
                    if ra != rb {
                        root[ra.max(rb)] = ra.min(rb);
                    }
                }
            }
        }
        let mut sums: BTreeMap<usize, (f64, f64, f64)> = BTreeMap::new();
        for a in 0..members.len() {
            let r = find(&mut root, a);
            let p = stations[members[a]];
            let e = sums.entry(r).or_insert((0.0, 0.0, 0.0));
            *e = (e.0 + p.x, e.1 + p.y, e.2 + 1.0);
        }
        for a in 0..members.len() {
            let r = find(&mut root, a);
            let (x, y, n) = sums[&r];
            if n > 1.0 {
                out[members[a]] = Point::new(x / n, y / n);
            }
        }
    }
    out
}

/// The edge (with lines) whose centerline is nearest to `s`, within [`STATION_SNAP_M`]: its
/// distance, index and how far along it the nearest point is. None when `s` isn't finite or no
/// edge is that close.
fn nearest_edge(topo: &Topology, order: &Order, s: Point) -> Option<(f64, EdgeId, f64)> {
    if !(s.x.is_finite() && s.y.is_finite()) {
        return None;
    }
    let mut best: Option<(f64, EdgeId, f64)> = None; // (distance, edge, meters along)
    for (e, edge) in topo.edges.iter_enumerated() {
        if order.get(e).is_none_or(Vec::is_empty) {
            continue;
        }
        if let Some((d, along)) = nearest_along(&edge.path, s)
            && best.is_none_or(|(bd, _, _)| d < bd)
        {
            best = Some((d, e, along));
        }
    }
    best.filter(|&(d, _, _)| d <= STATION_SNAP_M)
}

/// How far a station may be from the nearest edge's centerline to get a bar, in meters.
/// Bus stops in feeds sit at the curb, usually well within this.
pub const STATION_SNAP_M: f64 = 25.0;

/// A station's bar: a straight segment across the whole bundle where the station is,
/// perpendicular to the edge, from half a line width beyond the rightmost line to half a line
/// width beyond the leftmost. One per station, in input order; None for a station farther than
/// [`STATION_SNAP_M`] from every edge (or with no finite position).
///
/// Where it goes: the nearest point on any edge's centerline, moved off the node
/// fronts ([`node_fronts`]) into the stretch where the lines run parallel; an edge too short for
/// that puts the bar at its middle.
#[must_use]
pub fn station_bars(
    topo: &Topology,
    order: &Order,
    pitch_m: f64,
    line_width_m: f64,
    stations: &[Point],
) -> Vec<Option<(Point, Point)>> {
    let fronts = node_fronts(topo, order, pitch_m);
    stations
        .iter()
        .map(|&s| {
            let (_, e, along) = nearest_edge(topo, order, s)?;
            let edge = &topo.edges[e];
            let len = path_length(&edge.path);
            // Off the node fronts, and a line width inside the edge's ends: at the node
            // itself a line that ends or turns there only grazes the bar.
            let (lo, hi) = (
                fronts
                    .get(edge.from)
                    .copied()
                    .unwrap_or(0.0)
                    .max(line_width_m),
                len - fronts
                    .get(edge.to)
                    .copied()
                    .unwrap_or(0.0)
                    .max(line_width_m),
            );
            let along = if lo <= hi {
                along.clamp(lo, hi)
            } else {
                len / 2.0
            };
            let (at, dir) = point_and_direction(&edge.path, along)?;
            let n = order[e].len();
            let half = (n - 1) as f64 / 2.0 * pitch_m + line_width_m / 2.0;
            let left = Point::new(-dir.y, dir.x);
            Some((
                Point::new(at.x - left.x * half, at.y - left.y * half),
                Point::new(at.x + left.x * half, at.y + left.y * half),
            ))
        })
        .collect()
}

/// The distance from `p` to `path` and how far along `path` its nearest point is; None for a
/// path with no segment.
///
/// Not the topology's `arc_along`: this one divides by `dx² + dy²`, that one by the square of
/// the segment's length (a square root), so their last bits differ; merging them would move
/// station bars.
fn nearest_along(path: &[Point], p: Point) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    let mut at = 0.0;
    for w in path.windows(2) {
        let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
        let len_sq = dx * dx + dy * dy;
        let len = len_sq.sqrt();
        let t = if len_sq == 0.0 {
            0.0
        } else {
            (((p.x - w[0].x) * dx + (p.y - w[0].y) * dy) / len_sq).clamp(0.0, 1.0)
        };
        let d = p.distance(Point::new(w[0].x + t * dx, w[0].y + t * dy));
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, at + t * len));
        }
        at += len;
    }
    best
}

/// The point `along` meters along `path` and the unit direction of the segment it is on. Exactly
/// at a bend (a stop on the outside of a bend snaps onto its corner point), the direction
/// halfway between the two segments: there the drawn bundle, its corner rounded, runs
/// along that bisector, so a bar square to it is square to the lines.
pub(super) fn point_and_direction(path: &[Point], along: f64) -> Option<(Point, Point)> {
    /// Closer than this to a corner point (meters) is at it: far above rounding error.
    const AT_CORNER_M: f64 = 1e-6;
    let segs: Vec<(Point, Point, f64)> = path
        .windows(2)
        .map(|w| (w[0], w[1], w[0].distance(w[1])))
        .filter(|&(_, _, len)| len > 0.0)
        .collect();
    let dir = |(a, b, len): (Point, Point, f64)| Point::new((b.x - a.x) / len, (b.y - a.y) / len);
    let mut left = along.max(0.0);
    for (k, &seg) in segs.iter().enumerate() {
        let (a, _, len) = seg;
        if left <= len {
            let d = dir(seg);
            let at = Point::new(a.x + d.x * left, a.y + d.y * left);
            // The other segment at a corner point this one starts or ends at, if any.
            let other = if len - left <= AT_CORNER_M {
                segs.get(k + 1).copied().map(dir)
            } else if left <= AT_CORNER_M && k > 0 {
                segs.get(k - 1).copied().map(dir)
            } else {
                None
            };
            let d = match other {
                Some(o) => {
                    let (x, y) = (d.x + o.x, d.y + o.y);
                    let norm = (x * x + y * y).sqrt();
                    // A full U-turn has no halfway direction: keep the segment's.
                    if norm > 1e-9 {
                        Point::new(x / norm, y / norm)
                    } else {
                        d
                    }
                }
                None => d,
            };
            return Some((at, d));
        }
        left -= len;
    }
    segs.last().map(|&seg| (seg.1, dir(seg)))
}
