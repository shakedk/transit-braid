//! The quality rules, measured on the pipeline's output at zooms 11, 14 and
//! 17 ([`measure`]). Test tooling, never shipped: each rule is a plain function with tests that
//! fail on deliberately broken output, and the gate test checks every committed fixture
//! against its known misses.
//!
//! The rules look at the output against the same line graph the pipeline used
//! (`topology::build` on the same projected input), so "a corridor" is one graph edge:
//!
//! - **Q1** no gaps, no overlaps: across an edge, neighboring lines are one pitch apart
//!   (line width + spacing), ± 0.5 px. Measured on cuts across the edge every [`SAMPLE_STEP_M`],
//!   away from its ends (and outside the node fronts), where lines may leave for other edges.
//! - **Q2** no side swaps: the lines' order across an edge is the same on every cut.
//! - **Q3** smooth corners: no output line crosses itself (the join-gap part holds by
//!   construction: each output line is one continuous polyline).
//! - **Q4** stable order on zoom: each edge's order is the same at the three zooms.
//! - **Q5** stable order on edit: removing the last route changes no order on edges it is
//!   not on (zoom 14).
//! - **Q6** one line per route: as many output lines as input routes, each running the same
//!   way (checked when the input's ends are apart), with its ends within its widest slot offset +
//!   5 m of the input's ends.
//! - **Q7** close to the street: every output point within its widest slot offset + 5 m of its
//!   input route.
//! - **Q8** station bars: with the fixture's stops, or a station in the middle of every edge, each
//!   bar is square to its edge (± 5°), centered on it (± 0.5 px), and crossed by every line on it;
//!   a station with no bar counts too.

use crate::Fixture;
use crate::place::{LINE_WIDTH_PX, SPACING_PX, lab_stations, lng_lat_lines, midpoint};
use transit_braid_core::geom::Point;
use transit_braid_core::ids::{EdgeId, IdVec, LineId};
use transit_braid_core::pipeline::{Projection, Scale};
use transit_braid_core::topology::{self, Params, Topology};
use transit_braid_core::{geometry, ordering};

/// Named measurements of one fixture, sorted by name.
pub type Metrics = std::collections::BTreeMap<String, f64>;

/// The zooms the rules are checked at.
pub const ZOOMS: [u32; 3] = [11, 14, 17];
/// Distance between cuts across an edge, in meters.
pub const SAMPLE_STEP_M: f64 = 10.0;
/// Tolerance of Q1, in pixels.
pub const Q1_TOLERANCE_PX: f64 = 0.5;
/// How far an output line may be from its input beyond its slot offset (Q6, Q7), in meters.
pub const NEAR_M: f64 = 5.0;

/// One cut across an edge: where it is, its direction along the edge, and the edge's lines.
#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    /// The edge it crosses.
    pub edge: EdgeId,
    /// The point on the edge's centerline.
    pub at: Point,
    /// Unit vector along the edge (from its `from` node to its `to` node).
    pub along: Point,
}

/// Cuts across every edge with at least two lines, every `step_m` along its centerline, leaving
/// out `skip_m(lines)` at each end (where lines may turn away to other edges).
#[must_use]
pub fn cuts(topo: &Topology, step_m: f64, skip_m: impl Fn(usize) -> f64) -> Vec<Cut> {
    cuts_skipping(topo, step_m, |e| {
        let skip = skip_m(topo.edges[e].lines.len());
        (skip, skip)
    })
}

/// [`cuts`], leaving out `skip(edge)` = (meters at its `from` end, meters at its `to` end).
#[must_use]
pub fn cuts_skipping(
    topo: &Topology,
    step_m: f64,
    skip: impl Fn(EdgeId) -> (f64, f64),
) -> Vec<Cut> {
    let mut out = Vec::new();
    for (edge, e) in topo.edges.iter_enumerated() {
        if e.lines.len() < 2 || step_m.is_nan() || step_m <= 0.0 {
            continue;
        }
        let total: f64 = e.path.windows(2).map(|w| w[0].distance(w[1])).sum();
        let (skip_from, skip_to) = skip(edge);
        let mut d = skip_from;
        while d <= total - skip_to {
            if let Some((at, along)) = point_along(&e.path, d) {
                out.push(Cut { edge, at, along });
            }
            d += step_m;
        }
    }
    out
}

/// The point `d` meters along `path` and the unit direction of the segment it is on.
fn point_along(path: &[Point], d: f64) -> Option<(Point, Point)> {
    let mut left = d.max(0.0);
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        if len == 0.0 {
            continue;
        }
        let along = Point::new((w[1].x - w[0].x) / len, (w[1].y - w[0].y) / len);
        if left <= len {
            return Some((
                Point::new(w[0].x + along.x * left, w[0].y + along.y * left),
                along,
            ));
        }
        left -= len;
    }
    None
}

/// Where `line` crosses the cut, as its distance to the left of the edge's centerline (meters);
/// the crossing nearest the centerline, or None when it doesn't cross within `reach_m`.
#[must_use]
pub fn crossing(line: &[Point], cut: &Cut, reach_m: f64) -> Option<f64> {
    let (t, n) = (cut.along, Point::new(-cut.along.y, cut.along.x));
    let mut best: Option<f64> = None;
    for w in line.windows(2) {
        let da = (w[0].x - cut.at.x) * t.x + (w[0].y - cut.at.y) * t.y;
        let db = (w[1].x - cut.at.x) * t.x + (w[1].y - cut.at.y) * t.y;
        // Both ends on one side: no crossing. Both on the cut line: it runs across, not through.
        if (da > 0.0 && db > 0.0) || (da < 0.0 && db < 0.0) || (da == 0.0 && db == 0.0) {
            continue;
        }
        let k = da / (da - db);
        let p = Point::new(
            w[0].x + k * (w[1].x - w[0].x),
            w[0].y + k * (w[1].y - w[0].y),
        );
        let u = (p.x - cut.at.x) * n.x + (p.y - cut.at.y) * n.y;
        if u.abs() <= reach_m && best.is_none_or(|b: f64| u.abs() < b.abs()) {
            best = Some(u);
        }
    }
    best
}

/// The lines on a cut's edge in order across it (right to left of the edge's direction), or None
/// when one of them doesn't cross the cut within `reach_m`.
#[must_use]
pub fn order_at(
    topo: &Topology,
    out: &[Vec<Point>],
    cut: &Cut,
    reach_m: f64,
) -> Option<Vec<(LineId, f64)>> {
    let mut at = Vec::new();
    for &(line, _) in &topo.edges[cut.edge].lines {
        at.push((line, crossing(out.get(line.index())?, cut, reach_m)?));
    }
    at.sort_by(|a, b| a.1.total_cmp(&b.1));
    Some(at)
}

/// Q1: how many cuts have a gap between neighbors off the pitch by more than `tol_m` (or a line
/// missing), and the worst such difference in meters.
#[must_use]
pub fn q1_gaps(
    topo: &Topology,
    out: &[Vec<Point>],
    cuts: &[Cut],
    pitch_m: f64,
    tol_m: f64,
) -> (usize, f64) {
    let (mut bad, mut worst) = (0, 0.0_f64);
    for cut in cuts {
        let reach = pitch_m * topo.edges[cut.edge].lines.len() as f64 + NEAR_M;
        match order_at(topo, out, cut, reach) {
            None => bad += 1,
            Some(at) => {
                let off = at
                    .windows(2)
                    .map(|w| ((w[1].1 - w[0].1) - pitch_m).abs())
                    .fold(0.0, f64::max);
                worst = worst.max(off);
                if off > tol_m {
                    bad += 1;
                }
            }
        }
    }
    (bad, worst)
}

/// The order of an edge's lines on each of its cuts (ids only), by edge.
fn orders(
    topo: &Topology,
    out: &[Vec<Point>],
    cuts: &[Cut],
    pitch_m: f64,
) -> IdVec<EdgeId, Vec<Vec<LineId>>> {
    let mut by_edge: IdVec<EdgeId, Vec<Vec<LineId>>> = vec![Vec::new(); topo.edges.len()].into();
    for cut in cuts {
        let reach = pitch_m * topo.edges[cut.edge].lines.len() as f64 + NEAR_M;
        if let Some(at) = order_at(topo, out, cut, reach) {
            by_edge[cut.edge].push(at.into_iter().map(|(l, _)| l).collect());
        }
    }
    by_edge
}

/// Q2: edges whose order across them is not the same on every cut.
#[must_use]
pub fn q2_swapped_edges(topo: &Topology, out: &[Vec<Point>], cuts: &[Cut], pitch_m: f64) -> usize {
    orders(topo, out, cuts, pitch_m)
        .iter()
        .filter(|o| o.windows(2).any(|w| w[0] != w[1]))
        .count()
}

/// Each edge's order on its middle cut (None when it has no cut where every line was found).
fn middle_orders(
    topo: &Topology,
    out: &[Vec<Point>],
    cuts: &[Cut],
    pitch_m: f64,
) -> Vec<Option<Vec<LineId>>> {
    orders(topo, out, cuts, pitch_m)
        .into_iter()
        .map(|o| o.get(o.len() / 2).cloned())
        .collect()
}

/// Q3, arc form (rounded corners): points where a line's heading changes by more than 10°, over every
/// line (zero-length steps skipped).
#[must_use]
pub fn q3_sharp_turns(out: &[Vec<Point>]) -> usize {
    /// cos 10°.
    const COS_10: f64 = 0.984_807_753_012_208;
    out.iter()
        .map(|line| {
            let steps: Vec<(f64, f64)> = line
                .windows(2)
                .map(|w| (w[1].x - w[0].x, w[1].y - w[0].y))
                .filter(|&(dx, dy)| dx != 0.0 || dy != 0.0)
                .collect();
            steps
                .windows(2)
                .filter(|w| {
                    let ((ax, ay), (bx, by)) = (w[0], w[1]);
                    let dot = ax * bx + ay * by;
                    let norms = ((ax * ax + ay * ay) * (bx * bx + by * by)).sqrt();
                    dot < COS_10 * norms
                })
                .count()
        })
        .sum()
}

/// Q3: how many times output lines cross themselves (non-neighboring segments that intersect).
#[must_use]
pub fn q3_self_crossings(out: &[Vec<Point>]) -> usize {
    out.iter().map(|l| self_crossings(l)).sum()
}

fn self_crossings(line: &[Point]) -> usize {
    let segs: Vec<(Point, Point)> = line
        .windows(2)
        .filter(|w| w[0] != w[1])
        .map(|w| (w[0], w[1]))
        .collect();
    let mut n = 0;
    for i in 0..segs.len() {
        for j in i + 2..segs.len() {
            if segments_cross(segs[i], segs[j]) {
                n += 1;
            }
        }
    }
    n
}

/// True when the two segments cross at a point inside both (touching ends don't count).
fn segments_cross((a, b): (Point, Point), (c, d): (Point, Point)) -> bool {
    let side = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// The widest slot offset of each route: half the width of the busiest edge it is on.
fn widest_offset(topo: &Topology, routes: usize, pitch_m: f64) -> Vec<f64> {
    let mut busiest = vec![1_usize; routes];
    for e in &topo.edges {
        for &(l, _) in &e.lines {
            if let Some(b) = busiest.get_mut(l.index()) {
                *b = (*b).max(e.lines.len());
            }
        }
    }
    busiest
        .iter()
        .map(|&n| (n as f64 - 1.0) / 2.0 * pitch_m)
        .collect()
}

/// Q6: routes without exactly one output line running the same way with ends within their
/// widest offset + [`NEAR_M`] of the input's ends (a missing or extra line counts as one each).
#[must_use]
pub fn q6_bad_lines(input: &[Vec<Point>], out: &[Vec<Point>], widest: &[f64]) -> usize {
    q6_bad_lines_within(input, out, widest, NEAR_M)
}

/// [`q6_bad_lines`] with the room beyond the widest offset given (`room_m`: [`NEAR_M`], or one
/// pixel when that is larger; `geometry::room_at`).
#[must_use]
pub fn q6_bad_lines_within(
    input: &[Vec<Point>],
    out: &[Vec<Point>],
    widest: &[f64],
    room_m: f64,
) -> usize {
    let mut bad = input.len().abs_diff(out.len());
    for ((i, o), w) in input.iter().zip(out).zip(widest) {
        let (Some(&i0), Some(&i1), Some(&o0), Some(&o1)) =
            (i.first(), i.last(), o.first(), o.last())
        else {
            bad += 1;
            continue;
        };
        let allowed = w + room_m;
        // Which way it runs is only clear when the input's ends are apart (an out-and-back or a
        // loop ends where it starts).
        let ends_apart = i0.distance(i1) > 2.0 * allowed;
        let same_way =
            !ends_apart || o0.distance(i0) + o1.distance(i1) <= o0.distance(i1) + o1.distance(i0);
        if !same_way || o0.distance(i0) > allowed || o1.distance(i1) > allowed {
            bad += 1;
        }
    }
    bad
}

/// Q7: output points farther than their widest offset + [`NEAR_M`] from their input route,
/// and the worst excess in meters.
#[must_use]
pub fn q7_far_points(input: &[Vec<Point>], out: &[Vec<Point>], widest: &[f64]) -> (usize, f64) {
    q7_far_points_within(input, out, widest, NEAR_M)
}

/// [`q7_far_points`] with the room beyond the widest offset given (`room_m`: [`NEAR_M`], or one
/// pixel when that is larger; `geometry::room_at`).
#[must_use]
pub fn q7_far_points_within(
    input: &[Vec<Point>],
    out: &[Vec<Point>],
    widest: &[f64],
    room_m: f64,
) -> (usize, f64) {
    let (mut bad, mut worst) = (0, 0.0_f64);
    for ((i, o), w) in input.iter().zip(out).zip(widest) {
        for &p in o {
            let d = distance_to_polyline(p, i);
            let excess = d - (w + room_m);
            if excess > 0.0 {
                bad += 1;
                worst = worst.max(excess);
            }
        }
    }
    (bad, worst)
}

fn distance_to_polyline(p: Point, line: &[Point]) -> f64 {
    if let [only] = line {
        return p.distance(*only);
    }
    line.windows(2)
        .map(|w| {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let len_sq = dx * dx + dy * dy;
            let t = if len_sq == 0.0 {
                0.0
            } else {
                (((p.x - w[0].x) * dx + (p.y - w[0].y) * dy) / len_sq).clamp(0.0, 1.0)
            };
            p.distance(Point::new(w[0].x + t * dx, w[0].y + t * dy))
        })
        .fold(f64::INFINITY, f64::min)
}

/// The pipeline's output for `lines` (`[lng, lat]`) at `zoom`, in the meters of `proj`.
/// sin 5°: a bar is square to its edge when |cos| of the angle between them is at most this.
const SIN_5_DEG: f64 = 0.087_155_742_747_658_17;

/// What Q8 found: bars drawn, bars that fail (not square to their edge within 5°, not centered
/// within `half_px_m`, or not crossed by every line on the edge), and stations with no bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Q8 {
    /// Bars drawn.
    pub bars: usize,
    /// Bars that fail a check.
    pub bad: usize,
    /// Stations with no bar.
    pub missing: usize,
}

/// Q8 for stations (meters) and their bars: each bar is judged against the edge nearest its
/// station and the output lines (`out`, meters) of that edge's routes.
#[must_use]
pub fn q8_bars(
    topo: &Topology,
    out: &[Vec<Point>],
    stations: &[Point],
    bars: &[Option<(Point, Point)>],
    half_px_m: f64,
) -> Q8 {
    let mut q = Q8::default();
    for (s, bar) in stations.iter().zip(bars) {
        let Some((a, b)) = *bar else {
            q.missing += 1;
            continue;
        };
        q.bars += 1;
        let nearest = topo
            .edges
            .iter_enumerated()
            .filter(|(_, e)| !e.lines.is_empty())
            .map(|(k, e)| (distance_to_polyline(*s, &e.path), k))
            .min_by(|x, y| x.0.total_cmp(&y.0));
        let Some((_, e)) = nearest else {
            q.bad += 1;
            continue;
        };
        let edge = &topo.edges[e];
        let mid = Point::new(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
        let along = direction_near(&edge.path, mid);
        let len = a.distance(b);
        let square =
            len > 0.0 && ((b.x - a.x) * along.x + (b.y - a.y) * along.y).abs() / len <= SIN_5_DEG;
        let centered = distance_to_polyline(mid, &edge.path) <= half_px_m;
        let covers = edge.lines.iter().all(|&(l, _)| {
            out.get(l.index()).is_some_and(|line| {
                line.windows(2)
                    .any(|w| segments_touch((w[0], w[1]), (a, b)))
            })
        });
        if !(square && centered && covers) {
            q.bad += 1;
        }
    }
    q
}

/// The unit direction of the segment of `path` nearest to `p` (east for a path with none). At a
/// bend's corner point (within a millimeter), the direction halfway between its two segments:
/// the drawn bundle's corner is rounded, so there it runs along that bisector.
fn direction_near(path: &[Point], p: Point) -> Point {
    let segs: Vec<(Point, Point)> = path
        .windows(2)
        .filter(|w| w[0] != w[1])
        .map(|w| (w[0], w[1]))
        .collect();
    let unit = |(a, b): (Point, Point)| {
        let len = a.distance(b);
        Point::new((b.x - a.x) / len, (b.y - a.y) / len)
    };
    for w in segs.windows(2) {
        if w[0].1.distance(p) <= 1e-3 {
            let (d, n) = (unit(w[0]), unit(w[1]));
            let (x, y) = (d.x + n.x, d.y + n.y);
            let norm = (x * x + y * y).sqrt();
            if norm > 1e-9 {
                return Point::new(x / norm, y / norm);
            }
        }
    }
    path.windows(2)
        .filter(|w| w[0] != w[1])
        .map(|w| {
            let d = distance_to_polyline(p, w);
            let len = w[0].distance(w[1]);
            (
                d,
                Point::new((w[1].x - w[0].x) / len, (w[1].y - w[0].y) / len),
            )
        })
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map_or(Point::new(1.0, 0.0), |(_, dir)| dir)
}

/// True when the two segments meet (crossing or touching).
fn segments_touch((a, b): (Point, Point), (c, d): (Point, Point)) -> bool {
    let side = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    d1 * d2 <= 0.0 && d3 * d4 <= 0.0
}

fn output_m(lines: &[Vec<Point>], zoom: u32, proj: &Projection) -> Result<Vec<Vec<Point>>, String> {
    let out = transit_braid_core::pipeline::side_by_side(lines, &settings_at(zoom))
        .map_err(|e| e.to_string())?;
    Ok(out
        .iter()
        .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
        .collect())
}

/// Each edge of `old` matched to the edge of `new` with the same lines (`old` indices shifted by
/// `rename`) whose centerline middle is within the merge distance; None when there is none.
fn match_edges(
    old: &Topology,
    new: &Topology,
    rename: impl Fn(LineId) -> Option<LineId>,
) -> Vec<Option<usize>> {
    let middle = |p: &[Point]| p[p.len() / 2];
    old.edges
        .iter()
        .map(|e| {
            let lines: Option<Vec<LineId>> = e.lines.iter().map(|&(l, _)| rename(l)).collect();
            let lines = lines?;
            new.edges.iter().position(|f| {
                f.lines.iter().map(|&(l, _)| l).eq(lines.iter().copied())
                    && middle(&f.path).distance(middle(&e.path)) <= Params::default().merge_m
            })
        })
        .collect()
}

/// The rules for one fixture, as metrics: `q1_bad_cuts_z<zoom>`, `q1_worst_px_z<zoom>`,
/// `q2_swapped_edges_z<zoom>`, `q3_self_crossings_z<zoom>`, `q6_bad_lines_z<zoom>`,
/// `q7_far_points_z<zoom>`, `q7_worst_m_z<zoom>`, `q1_cuts_z<zoom>` (cuts measured), plus
/// `q4_changed_edges` and `q5_changed_edges`. A fixture the pipeline refuses gets
/// `quality_refused` = 1 and nothing else.
///
/// # Errors
/// Never in practice; the topology refusing input the pipeline took would be one.
pub fn measure(fixture: &Fixture) -> Result<Metrics, String> {
    let mut m = Metrics::new();
    let lines = lng_lat_lines(fixture);
    let proj = Projection::around(&lines);
    let input: Vec<Vec<Point>> = lines
        .iter()
        .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
        .collect();
    let topo = match transit_braid_core::pipeline::side_by_side(&lines, &settings_at(14)) {
        Err(_) => {
            m.insert("quality_refused".to_owned(), 1.0);
            return Ok(m);
        }
        Ok(_) => topology::build(&input, &Params::default()).map_err(|e| e.to_string())?,
    };
    let mut middles = Vec::new();
    // Q8's stations: the fixture's own stops, or one in the middle of every edge.
    let (stations_ll, groups) = if fixture.stations.is_empty() {
        let mid: Vec<Point> = topo
            .edges
            .iter()
            .filter(|e| !e.lines.is_empty())
            .map(|e| proj.inverse(midpoint(&e.path)))
            .collect();
        let none = vec![0; mid.len()];
        (mid, none)
    } else {
        lab_stations(fixture)
    };
    let stations: Vec<Point> = stations_ll.iter().map(|&p| proj.forward(p)).collect();
    for zoom in ZOOMS {
        let drawn = transit_braid_core::pipeline::side_by_side_with_named_stations(
            &lines,
            &stations_ll,
            &groups,
            &settings_at(zoom),
        )
        .map_err(|e| e.to_string())?;
        let out: Vec<Vec<Point>> = drawn
            .lines
            .iter()
            .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
            .collect();
        let bars: Vec<Option<(Point, Point)>> = drawn
            .bars
            .iter()
            .map(|b| b.map(|(a, c)| (proj.forward(a), proj.forward(c))))
            .collect();
        let px = proj.meters_per_pixel(Scale::Zoom(f64::from(zoom)));
        let pitch = (LINE_WIDTH_PX + SPACING_PX) * px;
        // Away from each edge's ends: half its width + a step, or the node's front where
        // lines turn and change slots, whichever is more.
        let fronts = geometry::node_fronts(&topo, &ordering::input_order(&topo), pitch);
        let cuts = cuts_skipping(&topo, SAMPLE_STEP_M, |e| {
            let edge = &topo.edges[e];
            let base = edge.lines.len() as f64 * pitch / 2.0 + SAMPLE_STEP_M;
            (base.max(fronts[edge.from]), base.max(fronts[edge.to]))
        });
        let (bad, worst) = q1_gaps(&topo, &out, &cuts, pitch, Q1_TOLERANCE_PX * px);
        let widest = widest_offset(&topo, input.len(), pitch);
        // The room beyond a line's slot is 5 m or one pixel, whichever is larger.
        let room = geometry::room_at(px);
        let (far, far_worst) = q7_far_points_within(&input, &out, &widest, room);
        let z = |k: &str| format!("{k}_z{zoom}");
        m.insert(z("q1_cuts"), count(cuts.len()));
        m.insert(z("q1_bad_cuts"), count(bad));
        m.insert(z("q1_worst_px"), round(worst / px));
        m.insert(
            z("q2_swapped_edges"),
            count(q2_swapped_edges(&topo, &out, &cuts, pitch)),
        );
        m.insert(z("q3_self_crossings"), count(q3_self_crossings(&out)));
        m.insert(z("q3_sharp_turns"), count(q3_sharp_turns(&out)));
        m.insert(
            z("q6_bad_lines"),
            count(q6_bad_lines_within(&input, &out, &widest, room)),
        );
        m.insert(z("q7_far_points"), count(far));
        m.insert(z("q7_worst_m"), round(far_worst));
        let q8 = q8_bars(&topo, &out, &stations, &bars, Q1_TOLERANCE_PX * px);
        m.insert(z("q8_bars"), count(q8.bars));
        m.insert(z("q8_bad_bars"), count(q8.bad));
        m.insert(z("q8_missing_bars"), count(q8.missing));
        middles.push(middle_orders(&topo, &out, &cuts, pitch));
    }
    let changed_on_zoom = q4_changed_edges(&middles);
    m.insert("q4_changed_edges".to_owned(), count(changed_on_zoom));
    insert_ordering(&mut m, &input)?;
    m.insert(
        "q5_changed_edges".to_owned(),
        count(q5_changed_edges(&lines, &proj, &topo, &input)?),
    );
    Ok(m)
}

/// Q4: edges whose middle order differs between zooms (`middles[zoom][edge]`; an edge with no
/// order at some zoom is compared among the zooms that have one).
#[must_use]
pub fn q4_changed_edges<T: PartialEq>(middles: &[Vec<Option<Vec<T>>>]) -> usize {
    let edges = middles.iter().map(Vec::len).max().unwrap_or(0);
    (0..edges)
        .filter(|&e| {
            let seen: Vec<&Vec<T>> = middles.iter().filter_map(|m| m.get(e)?.as_ref()).collect();
            seen.windows(2).any(|w| w[0] != w[1])
        })
        .count()
}

/// The ordering's numbers: crossings and separations of the order the pipeline uses
/// (`order_*`) and of the input order (`order_input_*`), and how many independent parts were
/// solved by trying every order or by greedy descent and annealing.
fn insert_ordering(m: &mut Metrics, input: &[Vec<Point>]) -> Result<(), String> {
    use transit_braid_core::ordering::{self, ExhaustiveLimit, Weights};
    let (topo, walks) =
        topology::build_with_traversals(input, &Params::default()).map_err(|e| e.to_string())?;
    let w = Weights::default();
    let s = ordering::solve(&topo, &walks, &w, ExhaustiveLimit::default());
    let before = ordering::cost(&topo, &walks, &ordering::input_order(&topo), &w);
    for (prefix, c) in [("order", s.cost), ("order_input", before)] {
        m.insert(format!("{prefix}_crossings_same"), count(c.same_segment));
        m.insert(
            format!("{prefix}_crossings_different"),
            count(c.different_segment),
        );
        m.insert(format!("{prefix}_separations"), count(c.separations));
        m.insert(format!("{prefix}_cost"), round(c.total));
    }
    m.insert(
        "order_parts_exhaustive".to_owned(),
        count(s.exhaustive_parts),
    );
    m.insert("order_parts_annealed".to_owned(), count(s.annealed_parts));
    Ok(())
}

fn settings_at(zoom: u32) -> transit_braid_core::pipeline::Settings {
    transit_braid_core::pipeline::Settings {
        scale: Scale::Zoom(f64::from(zoom)),
        line_width_px: LINE_WIDTH_PX,
        spacing_px: SPACING_PX,
    }
}

/// Q5 at zoom 14: remove the last route (through the pipeline's cache, as an edit in
/// `sideBySide()` is made) and count the edges it is not on whose order changed.
fn q5_changed_edges(
    lines: &[Vec<Point>],
    proj: &Projection,
    topo: &Topology,
    input: &[Vec<Point>],
) -> Result<usize, String> {
    let Some(removed) = lines.len().checked_sub(1) else {
        return Ok(0);
    };
    if removed == 0 {
        return Ok(0);
    }
    let px = proj.meters_per_pixel(Scale::Zoom(14.0));
    let pitch = (LINE_WIDTH_PX + SPACING_PX) * px;
    let skip = |n: usize| n as f64 * pitch / 2.0 + SAMPLE_STEP_M;
    let before = middle_orders(
        topo,
        &output_m(lines, 14, proj)?,
        &cuts(topo, SAMPLE_STEP_M, skip),
        pitch,
    );
    // The same projection for the edited input, so edges can be matched by place.
    let edited = &lines[..removed];
    let edited_m = &input[..removed];
    let new_topo = topology::build(edited_m, &Params::default()).map_err(|e| e.to_string())?;
    // The edit as `sideBySide()` makes it: through the pipeline's cache, warm-started.
    let mut cache = transit_braid_core::pipeline::Cache::new();
    transit_braid_core::pipeline::side_by_side_cached(
        &mut cache,
        lines,
        &[],
        &[],
        &settings_at(14),
    )
    .map_err(|e| e.to_string())?;
    let out = transit_braid_core::pipeline::side_by_side_cached(
        &mut cache,
        edited,
        &[],
        &[],
        &settings_at(14),
    )
    .map_err(|e| e.to_string())?
    .lines;
    // In the original meters, so cuts and edges line up with the unedited run.
    let out_m: Vec<Vec<Point>> = out
        .iter()
        .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
        .collect();
    let after = middle_orders(
        &new_topo,
        &out_m,
        &cuts(&new_topo, SAMPLE_STEP_M, skip),
        pitch,
    );
    let matched = match_edges(topo, &new_topo, |l| (l.index() != removed).then_some(l));
    Ok(topo
        .edges
        .iter()
        .enumerate()
        .filter(|(i, e)| {
            !e.lines.iter().any(|&(l, _)| l.index() == removed)
                && matched[*i].is_some_and(|j| match (&before[*i], &after[j]) {
                    (Some(a), Some(b)) => a != b,
                    _ => false,
                })
        })
        .count())
}

fn count(n: usize) -> f64 {
    f64::from(u32::try_from(n).unwrap_or(u32::MAX))
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::load;
    use transit_braid_core::ids::NodeId;
    use transit_braid_core::topology::{Dir, TopoEdge};

    use super::*;

    const P: f64 = 5.0; // pitch, meters

    /// One straight edge from (0, 0) to (200, 0) carrying lines 0, 1 and 2, the same way.
    fn three() -> Topology {
        Topology {
            nodes: vec![Point::new(0.0, 0.0), Point::new(200.0, 0.0)].into(),
            edges: vec![TopoEdge {
                from: NodeId::new(0),
                to: NodeId::new(1),
                path: vec![Point::new(0.0, 0.0), Point::new(200.0, 0.0)],
                lines: (0..3).map(|l| (LineId::new(l), Dir::Forward)).collect(),
            }]
            .into(),
        }
    }

    /// A line along y = `y` from x = 0 to 200.
    fn at(y: f64) -> Vec<Point> {
        vec![Point::new(0.0, y), Point::new(200.0, y)]
    }

    fn the_cuts(t: &Topology) -> Vec<Cut> {
        cuts(t, SAMPLE_STEP_M, |_| 20.0)
    }

    #[test]
    fn cuts_skip_the_ends_and_lone_lines() {
        let t = three();
        let c = the_cuts(&t);
        assert_eq!(c.len(), 17); // 20, 30, …, 180
        assert_eq!(c[0].at, Point::new(20.0, 0.0));
        assert_eq!(c[0].along, Point::new(1.0, 0.0));
        let mut lone = three();
        lone.edges[EdgeId::new(0)].lines.truncate(1);
        assert!(the_cuts(&lone).is_empty());
        assert!(
            cuts(&t, 10.0, |_| 150.0).is_empty(),
            "an edge shorter than its ends"
        );
        assert!(cuts(&t, 0.0, |_| 20.0).is_empty(), "no step, no cuts");
        assert!(cuts(&t, f64::NAN, |_| 20.0).is_empty());
    }

    #[test]
    fn q1_passes_one_pitch_apart_and_fails_a_gap_an_overlap_or_a_missing_line() {
        let t = three();
        let c = the_cuts(&t);
        let good = vec![at(-P), at(0.0), at(P)];
        assert_eq!(q1_gaps(&t, &good, &c, P, 0.5), (0, 0.0));
        let gap = vec![at(-P), at(0.0), at(P + 2.0)];
        let (bad, worst) = q1_gaps(&t, &gap, &c, P, 0.5);
        assert_eq!(bad, c.len());
        assert!((worst - 2.0).abs() < 1e-9);
        let overlap = vec![at(0.0), at(0.0), at(P)];
        assert_eq!(q1_gaps(&t, &overlap, &c, P, 0.5).0, c.len());
        let missing = vec![
            at(-P),
            at(0.0),
            vec![Point::new(0.0, 500.0), Point::new(200.0, 500.0)],
        ];
        assert_eq!(q1_gaps(&t, &missing, &c, P, 0.5).0, c.len());
        let too_few = vec![at(-P), at(0.0)];
        assert_eq!(q1_gaps(&t, &too_few, &c, P, 0.5).0, c.len());
    }

    #[test]
    fn q2_finds_lines_that_swap_sides_along_an_edge() {
        let t = three();
        let c = the_cuts(&t);
        assert_eq!(q2_swapped_edges(&t, &[at(-P), at(0.0), at(P)], &c, P), 0);
        // Line 0 starts on the right and ends on the left: it crosses the others.
        let crossing_over = vec![
            vec![Point::new(0.0, -P), Point::new(200.0, 2.0 * P)],
            at(0.0),
            at(P),
        ];
        assert_eq!(q2_swapped_edges(&t, &crossing_over, &c, P), 1);
    }

    #[test]
    fn q3_counts_self_crossings_but_not_touching_or_repeated_points() {
        let loop_ = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(5.0, -5.0),
        ];
        assert_eq!(q3_self_crossings(&[loop_]), 1);
        let zigzag = vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 0.0),
        ];
        assert_eq!(
            q3_self_crossings(&[zigzag, vec![], vec![Point::new(1.0, 1.0)]]),
            0
        );
    }

    #[test]
    fn q3_counts_turns_over_ten_degrees_per_point() {
        let at = |d: f64| {
            let r = d.to_radians();
            Point::new(r.cos(), r.sin())
        };
        // A quarter circle in 9° steps: no sharp turn; the same quarter in one corner: one.
        let smooth: Vec<Point> = (0..=10).map(|k| at(f64::from(k) * 9.0)).collect();
        assert_eq!(q3_sharp_turns(&[smooth]), 0);
        let corner = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
        ];
        assert_eq!(q3_sharp_turns(std::slice::from_ref(&corner)), 1);
        // Repeated points don't count as turns; empty and one-point lines have none.
        let repeated = vec![corner[0], corner[1], corner[1], corner[2]];
        assert_eq!(q3_sharp_turns(&[repeated, vec![], vec![corner[0]]]), 1);
        // A U-turn is a sharp turn.
        assert_eq!(q3_sharp_turns(&[vec![corner[0], corner[1], corner[0]]]), 1);
    }

    #[test]
    fn q4_finds_edges_whose_order_changes_with_zoom() {
        let same = vec![Some(vec![0, 1]), None];
        assert_eq!(q4_changed_edges(&[same.clone(), same.clone(), same]), 0);
        let swapped = vec![Some(vec![1, 0]), None];
        assert_eq!(
            q4_changed_edges(&[vec![Some(vec![0, 1]), None], swapped, vec![None, None]]),
            1
        );
        // An edge measured at one zoom only has nothing to compare.
        assert_eq!(
            q4_changed_edges(&[vec![Some(vec![0, 1])], vec![None], vec![]]),
            0
        );
        assert_eq!(q4_changed_edges::<usize>(&[]), 0);
    }

    /// The quality gate (in CI): every quality rule passes on every committed fixture at
    /// zooms 11, 14 and 17, except the known misses below, each with its reason and owner. A miss
    /// may shrink (then tighten this list); any new or bigger miss fails CI.
    const KNOWN_MISSES: [(&str, &str, f64, &str); 25] = [
        (
            "hairpin",
            "q1_bad_cuts_z11",
            69.0,
            "U-turn narrower than the bundle at zoom 11",
        ),
        (
            "hairpin",
            "q1_bad_cuts_z14",
            5.0,
            "the same U-turn at zoom 14",
        ),
        (
            "hairpin",
            "q8_bad_bars_z14",
            1.0,
            "the test bar sits on the U-turn's tip",
        ),
        ("hairpin", "q8_bad_bars_z17", 1.0, "the same bar at zoom 17"),
        (
            "parallel-corridor",
            "q1_bad_cuts_z11",
            5.0,
            "the corridor's jog is shorter than the offsets at zoom 11",
        ),
        (
            "stress-grid",
            "q1_bad_cuts_z14",
            40.0,
            "lanes around corners close together at zoom 14 (70-line bundles)",
        ),
        (
            "stress-grid",
            "q2_swapped_edges_z14",
            3.0,
            "three short edges inside the widest bundles swap two lines at zoom 14",
        ),
        (
            "stress-grid",
            "q3_self_crossings_z11",
            441.0,
            "junction connectors inside 70-line bundles hundreds of meters wide at zoom 11",
        ),
        (
            "stress-grid",
            "q3_self_crossings_z14",
            32.0,
            "the same junctions at zoom 14",
        ),
        (
            "stress-grid",
            "q3_self_crossings_z17",
            2.0,
            "the same junctions at zoom 17, where the widest bundles still meet",
        ),
        (
            "stress-grid",
            "q3_sharp_turns_z11",
            2204.0,
            "inner lines of bundles hundreds of meters wide fold at zoom 11 (Q7 caps arcs at 5 m)",
        ),
        (
            "stress-grid",
            "q3_sharp_turns_z14",
            1015.0,
            "the same folds at zoom 14",
        ),
        (
            "stress-grid",
            "q3_sharp_turns_z17",
            54.0,
            "sharp turns at busy junctions (connectors over 3 fronts keep the cubic)",
        ),
        (
            "out-and-back",
            "q3_sharp_turns_z11",
            1.0,
            "the out-and-back's tip is a 180° turn by design (one slot both ways)",
        ),
        ("out-and-back", "q3_sharp_turns_z14", 1.0, "the same tip"),
        ("out-and-back", "q3_sharp_turns_z17", 1.0, "the same tip"),
        (
            "hairpin",
            "q3_sharp_turns_z11",
            2.0,
            "a U-turn narrower than the bundle; arcs leave U-turns alone",
        ),
        ("hairpin", "q3_sharp_turns_z14", 1.0, "the same U-turn"),
        ("hairpin", "q3_sharp_turns_z17", 1.0, "the same U-turn"),
        (
            "stress-grid",
            "q8_bad_bars_z11",
            13.0,
            "a node's loop cut takes a line off a mid-edge bar where fronts cover most of a short edge at zoom 11",
        ),
        (
            "stress-grid",
            "q8_bad_bars_z14",
            9.0,
            "mid-edge bars on short edges at zoom 14 (the same cause)",
        ),
        (
            "stress-grid",
            "q8_bad_bars_z17",
            4.0,
            "mid-edge bars on the shortest edges at zoom 17 (the same cause)",
        ),
        (
            "stops",
            "q8_missing_bars_z11",
            1.0,
            "the fixture's far stop gets no bar, on purpose (same at every zoom)",
        ),
        (
            "stops",
            "q8_missing_bars_z14",
            1.0,
            "the far stop, on purpose",
        ),
        (
            "stops",
            "q8_missing_bars_z17",
            1.0,
            "the far stop, on purpose",
        ),
    ];

    /// Rule metrics that count misses (the others count what was measured, or a worst value).
    fn counts_misses(metric: &str) -> bool {
        metric.starts_with('q')
            && !["q1_cuts_", "q1_worst_px_", "q7_worst_m_", "q8_bars_"]
                .iter()
                .any(|p| metric.starts_with(p))
    }

    #[test]
    fn every_rule_passes_on_every_fixture_but_the_known_misses() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        let mut seen = 0;
        for folder in ["synthetic", "bus", "stress"] {
            let mut files: Vec<_> = std::fs::read_dir(root.join(folder))
                .unwrap()
                .map(|e| e.unwrap().path())
                .filter(|p| p.to_string_lossy().ends_with(".geojson"))
                .collect();
            files.sort();
            for path in files {
                let fixture = load(&path).unwrap();
                let m = measure(&fixture).unwrap();
                assert!(!m.contains_key("quality_refused"), "{}", fixture.name);
                for (metric, &value) in m.iter().filter(|(k, _)| counts_misses(k)) {
                    let allowed = KNOWN_MISSES
                        .iter()
                        .find(|(f, k, _, _)| *f == fixture.name && k == metric)
                        .map_or(0.0, |&(_, _, n, _)| n);
                    assert!(
                        value <= allowed,
                        "{} {metric} = {value}, allowed {allowed}",
                        fixture.name
                    );
                }
                seen += 1;
            }
        }
        assert_eq!(seen, 12, "every committed fixture is checked");
    }

    /// Q8 on a fixture's own stops: seven bars, all good; the far stop is the one missing.
    #[test]
    fn q8_measures_the_stops_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bus/stops.geojson");
        let m = measure(&load(&path).unwrap()).unwrap();
        for zoom in ZOOMS {
            assert_eq!(m[&format!("q8_bars_z{zoom}")], 7.0, "zoom {zoom}");
            assert_eq!(m[&format!("q8_bad_bars_z{zoom}")], 0.0, "zoom {zoom}");
            assert_eq!(m[&format!("q8_missing_bars_z{zoom}")], 1.0, "zoom {zoom}");
        }
    }

    /// On a bend's corner point the bundle runs halfway between its two segments, so Q8
    /// takes a bar square to that as square, and one square to a single segment as tilted.
    #[test]
    fn q8_measures_a_bar_on_a_corner_against_the_bend() {
        let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
        let path = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0 + 100.0 * c, 100.0 * s),
        ];
        let half = direction_near(&path, Point::new(100.0, 0.0));
        let ten = 10f64.to_radians();
        assert!((half.x - ten.cos()).abs() < 1e-12 && (half.y - ten.sin()).abs() < 1e-12);
        // Off the corner, the segment's own direction, as before.
        assert_eq!(
            direction_near(&path, Point::new(50.0, 1.0)),
            Point::new(1.0, 0.0)
        );
        // A path that turns straight back keeps a segment's direction.
        let back = vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(0.0, 0.0),
        ];
        assert_eq!(direction_near(&back, Point::new(100.0, 0.0)).y, 0.0);
    }

    #[test]
    fn q8_passes_a_square_centered_bar_across_all_lines_and_fails_broken_ones() {
        let t = three();
        let out = vec![at(-P), at(0.0), at(P)];
        let s = [Point::new(100.0, 0.0)];
        let good = (Point::new(100.0, -7.0), Point::new(100.0, 7.0));
        assert_eq!(
            q8_bars(&t, &out, &s, &[Some(good)], 0.5),
            Q8 {
                bars: 1,
                bad: 0,
                missing: 0
            }
        );
        let broken = [
            (Point::new(95.0, -7.0), Point::new(105.0, 7.0)), // tilted far past 5°
            (Point::new(100.0, -6.0), Point::new(100.0, 8.0)), // off center by 1 m
            (Point::new(100.0, -3.0), Point::new(100.0, 3.0)), // misses the outer lines
        ];
        for bar in broken {
            assert_eq!(q8_bars(&t, &out, &s, &[Some(bar)], 0.5).bad, 1, "{bar:?}");
        }
        assert_eq!(
            q8_bars(&t, &out, &s, &[None], 0.5),
            Q8 {
                bars: 0,
                bad: 0,
                missing: 1
            }
        );
        assert_eq!(
            q8_bars(&Topology::default(), &out, &s, &[Some(good)], 0.5).bad,
            1,
            "no edge"
        );
    }

    #[test]
    fn q6_finds_missing_reversed_and_far_ended_lines() {
        let input = vec![at(0.0), at(0.0)];
        let widest = [P / 2.0, P / 2.0];
        assert_eq!(q6_bad_lines(&input, &[at(-2.5), at(2.5)], &widest), 0);
        let mut reversed = at(2.5);
        reversed.reverse();
        assert_eq!(q6_bad_lines(&input, &[at(-2.5), reversed], &widest), 1);
        let short = vec![Point::new(0.0, 2.5), Point::new(150.0, 2.5)];
        assert_eq!(q6_bad_lines(&input, &[at(-2.5), short], &widest), 1);
        assert_eq!(q6_bad_lines(&input, &[at(-2.5)], &widest), 1);
        assert_eq!(q6_bad_lines(&input, &[at(-2.5), vec![]], &widest), 1);
        // An out-and-back ends where it starts: its direction can't be told from its ends.
        let out_back = vec![vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(0.0, 1.0),
        ]];
        let shifted = vec![vec![
            Point::new(0.0, 3.0),
            Point::new(100.0, 3.0),
            Point::new(0.0, -2.0),
        ]];
        assert_eq!(q6_bad_lines(&out_back, &shifted, &[P / 2.0]), 0);
    }

    #[test]
    fn q6_and_q7_allow_one_pixel_beyond_the_slot_when_that_is_more_than_5_m() {
        // At zoom 11 a pixel is tens of meters, so a point 30 m past the slot passes there.
        let input = vec![vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)]];
        let at = |y: f64| vec![Point::new(0.0, y), Point::new(100.0, y)];
        assert_eq!(
            q7_far_points_within(&input, &[at(32.0)], &[2.5], 38.0),
            (0, 0.0)
        );
        let (bad, worst) = q7_far_points_within(&input, &[at(32.0)], &[2.5], 5.0);
        assert_eq!(bad, 2);
        assert!((worst - 24.5).abs() < 1e-9, "{worst}");
        assert_eq!(q6_bad_lines_within(&input, &[at(32.0)], &[2.5], 38.0), 0);
        assert_eq!(q6_bad_lines_within(&input, &[at(32.0)], &[2.5], 5.0), 1);
        // The room the rule uses: 5 m at street zooms, one pixel far out, 5 m for a bad scale.
        assert_eq!(geometry::room_at(0.3), 5.0);
        assert_eq!(geometry::room_at(38.0), 38.0);
        for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
            assert_eq!(geometry::room_at(bad), 5.0, "{bad}");
        }
    }

    #[test]
    fn q7_finds_points_farther_than_the_slot_allows() {
        let input = vec![at(0.0)];
        assert_eq!(q7_far_points(&input, &[at(7.0)], &[2.5]), (0, 0.0));
        let (bad, worst) = q7_far_points(&input, &[at(9.0)], &[2.5]);
        assert_eq!(bad, 2);
        assert!((worst - 1.5).abs() < 1e-9);
        assert_eq!(
            q7_far_points(
                &[vec![Point::new(0.0, 0.0)]],
                &[vec![Point::new(3.0, 4.0)]],
                &[0.0]
            ),
            (0, 0.0)
        );
    }

    #[test]
    fn crossing_is_measured_left_of_the_edge_and_within_reach() {
        let t = three();
        let c = &the_cuts(&t)[0];
        assert_eq!(crossing(&at(3.0), c, 10.0), Some(3.0));
        assert_eq!(crossing(&at(-3.0), c, 10.0), Some(-3.0));
        assert_eq!(crossing(&at(30.0), c, 10.0), None);
        let short = vec![Point::new(50.0, 0.0), Point::new(60.0, 0.0)];
        assert_eq!(
            crossing(&short, c, 10.0),
            None,
            "a line that doesn't reach the cut"
        );
        assert_eq!(
            order_at(&t, &[at(1.0), at(-1.0)], c, 10.0),
            None,
            "line 2 has no output"
        );
    }

    #[test]
    fn the_harness_measures_every_rule_at_every_zoom() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/synthetic");
        let f = load(&dir.join("parallel-corridor.geojson")).unwrap();
        let m = measure(&f).unwrap();
        for zoom in ZOOMS {
            for key in [
                "q1_cuts",
                "q1_bad_cuts",
                "q1_worst_px",
                "q2_swapped_edges",
                "q3_self_crossings",
                "q6_bad_lines",
                "q7_far_points",
                "q7_worst_m",
                "q8_bars",
                "q8_bad_bars",
                "q8_missing_bars",
            ] {
                assert!(m.contains_key(&format!("{key}_z{zoom}")), "{key} at {zoom}");
            }
        }
        assert!(m["q1_cuts_z14"] > 0.0, "{m:?}");
        assert!(m.contains_key("q4_changed_edges") && m.contains_key("q5_changed_edges"));
        assert_eq!(m["q6_bad_lines_z14"], 0.0, "{m:?}");
        let mut refused = f.clone();
        refused.lines[0].points.truncate(1);
        assert_eq!(
            measure(&refused).unwrap(),
            Metrics::from([("quality_refused".to_owned(), 1.0)])
        );
    }
}
