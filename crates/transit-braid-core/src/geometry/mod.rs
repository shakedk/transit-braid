//! Geometry: every line drawn in its slot along each edge of the line graph, and joined smoothly
//! at the nodes.
//!
//! Each edge's lines sit side by side across its centerline in the order's slots, one pitch apart
//! (line width + spacing), centered: slot `k` of `n` is `(k − (n − 1) / 2) × pitch` to the left of
//! the edge's `from` → `to` direction. Every line on an edge is offset from the **same**
//! centerline, so neighbors are exactly one pitch apart however far apart their input routes
//! were.
//!
//! A route's output follows its walk ([`Traversal`]): the piece of each edge it runs, reversed
//! where it runs against the edge. An out-and-back uses its one slot both ways, so its two passes
//! fold onto each other.
//!
//! At every node the pieces are cut back by a **node front** ([`node_fronts`]), so lines have room
//! to turn and change slots, and consecutive pieces are joined by a short smooth connector (two
//! circular arcs, or a cubic where they don't fit). A walk's own two ends are not cut, so a line
//! reaches its start and end.
//!
//! Each edge's corners are rounded with circular arcs before its lines are offset, so a bundle
//! turns as concentric arcs and no line folds.
//!
//! A station's bar crosses the whole bundle where the station is ([`station_bars`]).
//!
//! Offsetting a stretch that bends more tightly than its offset folds the line into a small loop
//! on the inside of the bend; each piece has those loops cut out.
//!
//! The parts live in their own files: `fronts` (how far each node cuts its edges back), `walk`
//! (one route along its walk: pieces in its slot, joined by connectors), `loops` (cutting the
//! small loops offsetting and connectors can fold into a line) and `stations` (station bars).

mod fronts;
mod loops;
mod stations;
mod walk;

pub use fronts::node_fronts;
pub use stations::{
    STATION_MERGE_M, STATION_SNAP_M, merge_named_stations, merge_named_stations_on_edges,
    station_bars,
};

use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId};
use crate::ordering::Order;
use crate::polyline::{DEFAULT_MITER_LIMIT, offset_polyline, round_corners};
use crate::topology::{Dir, Topology, Traversal};

use walk::walk_line;

/// Slot `slot` of `n` lines as a distance left of the edge's direction, in pitches.
#[must_use]
pub fn slot_offset(slot: usize, n: usize) -> f64 {
    slot as f64 - (n.max(1) - 1) as f64 / 2.0
}

/// Every line's piece on every edge: `pieces[edge]` lists `(line, points)` in the edge's order,
/// each piece running from the edge's `from` node to its `to` node, `pitch_m` meters from its
/// neighbors. A line whose piece can't be drawn (an edge with fewer than two distinct points) gets
/// none.
#[must_use]
pub fn edge_pieces(
    topo: &Topology,
    order: &Order,
    pitch_m: f64,
) -> IdVec<EdgeId, Vec<(LineId, Vec<Point>)>> {
    topo.edges
        .iter_enumerated()
        .map(|(e, edge)| {
            let lines = order.get(e).map_or(&[][..], Vec::as_slice);
            lines
                .iter()
                .enumerate()
                .filter_map(|(slot, &line)| {
                    let d = slot_offset(slot, lines.len()) * pitch_m;
                    offset_polyline(&edge.path, d, DEFAULT_MITER_LIMIT).map(|p| (line, p))
                })
                .collect()
        })
        .collect()
}

/// Route `line`'s output: the pieces of its walk joined end to end (straight joins), or
/// None when its walk is empty or has no drawable piece.
#[must_use]
pub fn assemble(
    walk: &Traversal,
    pieces: &IdVec<EdgeId, Vec<(LineId, Vec<Point>)>>,
    line: LineId,
) -> Option<Vec<Point>> {
    let mut out: Vec<Point> = Vec::new();
    for &(e, dir) in walk {
        let Some((_, piece)) = pieces
            .get(e)
            .and_then(|ps| ps.iter().find(|(l, _)| *l == line))
        else {
            continue;
        };
        let pts: Box<dyn Iterator<Item = &Point>> = if dir == Dir::Backward {
            Box::new(piece.iter().rev())
        } else {
            Box::new(piece.iter())
        };
        for &p in pts {
            if out.last() != Some(&p) {
                out.push(p);
            }
        }
    }
    (out.len() >= 2).then_some(out)
}

/// How far an outside corner's miter tip may reach beyond the line's slot, in meters: more and
/// the corner is beveled, so the line stays within its slot + 5 m of the street. At street
/// zooms slots are a few meters, so corners stay sharp; far out (zoom 11: ~76 m a pixel at the
/// equator) they are beveled.
pub const MAX_CORNER_OVERSHOOT_M: f64 = 5.0;

/// Every route's output: along its walk, each edge's piece in the line's slot, cut back by the
/// node fronts ([`node_fronts`]) except at the walk's own two ends, joined across each node by a
/// smooth connector. A route with no drawable walk (on no edge) keeps `fallback[i]` (its
/// input), so every input gets one line. A closed input (a loop line that ends where it starts)
/// gives a closed line. The room a corner may take beyond a line's slot is
/// [`MAX_CORNER_OVERSHOOT_M`] here; the pipeline uses [`lines_with_room`].
#[must_use]
pub fn lines(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    order: &Order,
    pitch_m: f64,
    fallback: &[Vec<Point>],
) -> Vec<Vec<Point>> {
    lines_with_room(
        topo,
        walks,
        order,
        pitch_m,
        MAX_CORNER_OVERSHOOT_M,
        fallback,
    )
}

/// The room a corner may take beyond a line's slot at a map scale: [`MAX_CORNER_OVERSHOOT_M`] or
/// one pixel (`m_per_px` meters), whichever is larger, so corners can still round far out, where a
/// pixel is tens of meters. A scale that isn't a finite number above 0 gives the 5 m.
#[must_use]
pub fn room_at(m_per_px: f64) -> f64 {
    if m_per_px.is_finite() && m_per_px > MAX_CORNER_OVERSHOOT_M {
        m_per_px
    } else {
        MAX_CORNER_OVERSHOOT_M
    }
}

/// [`lines`] with the room given (`room_m` meters beyond a line's slot: how far an arc
/// corner may round in and a miter tip may reach out; see [`room_at`]). A room that isn't a finite
/// number above 0 gives [`MAX_CORNER_OVERSHOOT_M`].
#[must_use]
pub fn lines_with_room(
    topo: &Topology,
    walks: &IdVec<LineId, Traversal>,
    order: &Order,
    pitch_m: f64,
    room_m: f64,
    fallback: &[Vec<Point>],
) -> Vec<Vec<Point>> {
    let room_m = if room_m.is_finite() && room_m > 0.0 {
        room_m
    } else {
        MAX_CORNER_OVERSHOOT_M
    };
    let fronts = node_fronts(topo, order, pitch_m);
    let centers = rounded_centerlines(topo, order, pitch_m, room_m);
    fallback
        .iter()
        .enumerate()
        .map(|(i, input)| {
            let i = LineId::new(i);
            let mut line = walks
                .get(i)
                .and_then(|w| walk_line(topo, &centers, order, &fronts, pitch_m, room_m, w, i))
                .unwrap_or_else(|| input.clone());
            close_like(input, &mut line, pitch_m);
            line
        })
        .collect()
}

/// Every edge's centerline with its corners rounded ([`round_corners`]) to the radius that leaves
/// its innermost line half a pitch (half a line width plus half the spacing): the edge's line
/// count × pitch / 2. The room beyond the slot (`room_m`, [`room_at`]) and the legs'
/// lengths can make an arc smaller. Every line of an edge is offset from the same rounded
/// centerline, so they turn as concentric arcs.
fn rounded_centerlines(
    topo: &Topology,
    order: &Order,
    pitch_m: f64,
    room_m: f64,
) -> IdVec<EdgeId, Vec<Point>> {
    topo.edges
        .iter_enumerated()
        .map(|(e, edge)| {
            let n = order.get(e).map_or(1, Vec::len).max(1);
            round_corners(&edge.path, n as f64 * pitch_m / 2.0, room_m)
        })
        .collect()
}

/// A loop line: when `input` ends exactly where it starts and its drawn `line` ends
/// within half a slot of its start (the same slot, from two edges' offsets a few centimeters
/// apart), the line's end is moved onto its start, so it closes instead of overlapping itself.
fn close_like(input: &[Point], line: &mut [Point], pitch_m: f64) {
    let closed = input.len() > 2 && input.first() == input.last();
    if !closed || line.len() < 3 {
        return;
    }
    let first = line[0];
    if let Some(last) = line.last_mut()
        && last.distance(first) <= pitch_m / 2.0
    {
        *last = first;
    }
}

#[cfg(test)]
mod lines_tests;
#[cfg(test)]
mod loops_tests;
#[cfg(test)]
mod stations_tests;
#[cfg(test)]
mod test_support;
