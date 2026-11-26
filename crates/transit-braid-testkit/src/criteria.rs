//! Clean-graph criteria: is a line graph a good topology for its fixture?
//!
//! - **C1 drift:** every input point, and a point every [`DRIFT_SAMPLE_M`] along each route
//!   (Q6), within [`DRIFT_MAX_M`] of an edge carrying its route; 95% within [`DRIFT_P95_M`].
//!   Spike tips that the topology removes on purpose (
//!   `despike` at [`SPIKE_M`]) are skipped and counted in [`Report::spike_tips_removed`].
//! - **C2 connected:** each route's edges form one walk (each `forward`/`backward` edge used
//!   once in its direction, each `both` edge once each way) whose length is within
//!   [`LENGTH_TOLERANCE`] of the input length. Given the route's own walk (the topology's, see
//!   [`check_walked`]), C2 follows it instead, so a street the route drives twice counts twice
//!   too.
//! - **C3 no flicker:** no edge shorter than [`FLICKER_M`] whose route set changes at both
//!   ends, along its lines: a line continues onto an edge with a different set, or ends there
//!   (a crossing street that shares no line is not a change).
//! - **C4 same topology** ([`same_topology`]): node count, edge count and per-edge route sets
//!   equal the expected graph.
//! - **C5 no artifacts:** no edge shorter than [`ARTIFACT_M`], no two nodes closer than that.
//! - C6 (deterministic) is about the algorithm that builds graphs; it is tested there.
//!
//! The expected graphs in `fixtures/` must pass; the topology spike's output is checked the same
//! way.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::despike::despike;
use crate::place::{from_meters, reference_latitude, to_meters};
use crate::{Direction, Fixture, LineGraph};
use transit_braid_core::geom::Point;

/// C1: largest allowed distance from an input point to its route's edges.
pub const DRIFT_MAX_M: f64 = 5.0;
/// C1: the 95th percentile must be within this.
pub const DRIFT_P95_M: f64 = 3.0;
/// C1 (Q6): besides its own points, each route is checked every this many meters along it, so
/// a long straight segment that leaves its edges between two points is caught too.
pub const DRIFT_SAMPLE_M: f64 = 5.0;
/// The spike size the topology removes before building (`topology::despike`); C1 and C2
/// measure the input after the same removal.
pub const SPIKE_M: f64 = 5.0;
/// C2: allowed relative difference between walk length and input length.
pub const LENGTH_TOLERANCE: f64 = 0.02;
/// C3: edges shorter than this between two set changes are flicker.
pub const FLICKER_M: f64 = 20.0;
/// C5: shortest allowed edge and closest allowed nodes.
pub const ARTIFACT_M: f64 = 5.0;

/// What the criteria found for one graph.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// C1: largest point-to-edge distance.
    pub drift_max_m: f64,
    /// C1: 95th-percentile point-to-edge distance.
    pub drift_p95_m: f64,
    /// C2: routes that don't form one walk of the right length, with the reason.
    pub disconnected: Vec<String>,
    /// C3: ids of flicker edges.
    pub flicker_edges: Vec<String>,
    /// C5: ids of edges shorter than [`ARTIFACT_M`].
    pub short_edges: Vec<String>,
    /// C5: pairs of node ids closer than [`ARTIFACT_M`].
    pub close_nodes: Vec<(String, String)>,
    /// Input points removed as spike tips before measuring (not a failure).
    pub spike_tips_removed: usize,
    /// C1 per input point (spike tips removed), plus each in-between sample (Q6) over
    /// [`DRIFT_P95_M`], to keep the viewer's file small: `(route id, point in the fixture's units,
    /// meters to the nearest edge carrying it)`, for a viewer to show. Routes on no edge are
    /// left out (C2 reports them).
    pub drift: Vec<(String, Point, f64)>,
}

impl Report {
    /// One readable line per failed criterion; empty if the graph is clean.
    #[must_use]
    pub fn failures(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.drift_max_m > DRIFT_MAX_M || self.drift_p95_m > DRIFT_P95_M {
            out.push(format!(
                "C1 drift: max {:.2} m (limit {DRIFT_MAX_M}), p95 {:.2} m (limit {DRIFT_P95_M})",
                self.drift_max_m, self.drift_p95_m
            ));
        }
        if !self.disconnected.is_empty() {
            out.push(format!("C2 connected: {}", self.disconnected.join("; ")));
        }
        if !self.flicker_edges.is_empty() {
            out.push(format!(
                "C3 flicker edges: {}",
                self.flicker_edges.join(", ")
            ));
        }
        if !self.short_edges.is_empty() {
            out.push(format!(
                "C5 edges under {ARTIFACT_M} m: {}",
                self.short_edges.join(", ")
            ));
        }
        if !self.close_nodes.is_empty() {
            let pairs: Vec<String> = self
                .close_nodes
                .iter()
                .map(|(a, b)| format!("{a}–{b}"))
                .collect();
            out.push(format!(
                "C5 nodes under {ARTIFACT_M} m apart: {}",
                pairs.join(", ")
            ));
        }
        out
    }
}

/// One route's walk through a [`LineGraph`]: `(edge index, Forward or Backward)` per step, in
/// order (the topology's traversal, [`crate::built::line_graph_walked`]).
pub type Walk = Vec<(usize, Direction)>;

/// Checks C1, C2, C3 and C5 for `graph` against `fixture` (which it must have been validated
/// against, see `crate::parse_graph`).
#[must_use]
pub fn check(fixture: &Fixture, graph: &LineGraph) -> Report {
    check_walked(fixture, graph, None)
}

/// [`check`], with each route's walk when the graph's builder gives them (`walks[i]` is
/// fixture line `i`'s). C2 then checks that walk: its steps join end to start, it covers
/// every edge listing the route (a `both` edge each way) and nothing else, and its length,
/// an edge walked twice counted twice, is within [`LENGTH_TOLERANCE`] of the input. A graph
/// without walks (a hand-built expected one) is checked as by [`check`].
#[must_use]
pub fn check_walked(fixture: &Fixture, graph: &LineGraph, walks: Option<&[Walk]>) -> Report {
    let lat0 = reference_latitude(fixture);
    let nodes: Vec<Point> = graph
        .nodes
        .iter()
        .map(|n| to_meters(&[n.at], fixture.units, lat0)[0])
        .collect();
    let centerlines: Vec<Vec<Point>> = graph
        .edges
        .iter()
        .map(|e| to_meters(&e.centerline(&graph.nodes), fixture.units, lat0))
        .collect();
    let lengths: Vec<f64> = centerlines.iter().map(|c| path_length(c)).collect();
    // The input as the topology sees it, spike tips removed.
    let inputs: Vec<Vec<Point>> = fixture
        .lines
        .iter()
        .map(|l| despike(&to_meters(&l.points, fixture.units, lat0), SPIKE_M))
        .collect();
    let spike_tips_removed = fixture
        .lines
        .iter()
        .zip(&inputs)
        .map(|(l, kept)| l.points.len() - kept.len())
        .sum();

    // C1: each input point, and every DRIFT_SAMPLE_M along the route (Q6), against the edges
    // carrying its route.
    let mut drift = Vec::new();
    let mut drift_at = Vec::new();
    for (line, input) in fixture.lines.iter().zip(&inputs) {
        let own: Vec<&Vec<Point>> = graph
            .edges
            .iter()
            .zip(&centerlines)
            .filter(|(e, _)| e.lines.iter().any(|l| l.line == line.id))
            .map(|(_, c)| c)
            .collect();
        if own.is_empty() {
            continue; // on no edge at all: C2 reports it
        }
        for (p, own_point) in samples_along(input, DRIFT_SAMPLE_M) {
            let d = own
                .iter()
                .flat_map(|c| c.windows(2).map(|w| distance_to_segment(p, w[0], w[1])))
                .fold(f64::INFINITY, f64::min);
            drift.push(d);
            if own_point || d > DRIFT_P95_M {
                drift_at.push((line.id.clone(), from_meters(p, fixture.units, lat0), d));
            }
        }
    }
    let drift_max_m = drift.iter().copied().fold(0.0, f64::max);
    let drift_p95_m = percentile(&drift, 0.95);

    // C2: one walk per route, of the right length.
    let mut disconnected = Vec::new();
    for (i, (line, input)) in fixture.lines.iter().zip(&inputs).enumerate() {
        let input_len = path_length(input);
        if let Some(walk) = walks.and_then(|w| w.get(i)) {
            match walk_length(graph, &lengths, &line.id, walk) {
                None => disconnected.push(format!("{}: edges don't form one walk", line.id)),
                Some(walk_len) if (walk_len - input_len).abs() > LENGTH_TOLERANCE * input_len => {
                    disconnected.push(format!(
                        "{}: walk is {walk_len:.1} m, input {input_len:.1} m",
                        line.id
                    ));
                }
                Some(_) => {}
            }
            continue;
        }
        let mut arcs = Vec::new();
        let mut walk_len = 0.0;
        for (e, len) in graph.edges.iter().zip(&lengths) {
            for l in e.lines.iter().filter(|l| l.line == line.id) {
                match l.dir {
                    Direction::Forward => arcs.push((e.from, e.to)),
                    Direction::Backward => arcs.push((e.to, e.from)),
                    Direction::Both => {
                        arcs.push((e.from, e.to));
                        arcs.push((e.to, e.from));
                    }
                }
                walk_len += len * if l.dir == Direction::Both { 2.0 } else { 1.0 };
            }
        }
        if !is_one_walk(&arcs) {
            disconnected.push(format!("{}: edges don't form one walk", line.id));
        } else if (walk_len - input_len).abs() > LENGTH_TOLERANCE * input_len {
            disconnected.push(format!(
                "{}: walk is {walk_len:.1} m, input {input_len:.1} m",
                line.id
            ));
        }
    }

    // C3: short edges whose route set changes at both ends.
    let set = |i: usize| -> BTreeSet<&str> {
        graph.edges[i]
            .lines
            .iter()
            .map(|l| l.line.as_str())
            .collect()
    };
    let mut at_node: HashMap<usize, Vec<usize>> = HashMap::new();
    for (i, e) in graph.edges.iter().enumerate() {
        at_node.entry(e.from).or_default().push(i);
        at_node.entry(e.to).or_default().push(i);
    }
    // The set changes *along the lines* at a node: one of the edge's lines continues onto an
    // edge with a different set, or ends there. Edges that only cross (no shared line) don't
    // count.
    let changes_at = |edge: usize, node: usize| {
        let mine = set(edge);
        let others: Vec<usize> = at_node[&node]
            .iter()
            .copied()
            .filter(|&o| o != edge)
            .collect();
        let continues_differently = others
            .iter()
            .any(|&o| set(o) != mine && !set(o).is_disjoint(&mine));
        let a_line_ends = mine
            .iter()
            .any(|l| others.iter().all(|&o| !set(o).contains(l)));
        continues_differently || a_line_ends
    };
    let flicker_edges = graph
        .edges
        .iter()
        .enumerate()
        .filter(|&(i, e)| lengths[i] < FLICKER_M && changes_at(i, e.from) && changes_at(i, e.to))
        .map(|(_, e)| e.id.clone())
        .collect();

    // C5: tiny edges and nodes on top of each other.
    let short_edges = graph
        .edges
        .iter()
        .zip(&lengths)
        .filter(|&(_, &len)| len < ARTIFACT_M)
        .map(|(e, _)| e.id.clone())
        .collect();
    let mut close_nodes = Vec::new();
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            if nodes[i].distance(nodes[j]) < ARTIFACT_M {
                close_nodes.push((graph.nodes[i].id.clone(), graph.nodes[j].id.clone()));
            }
        }
    }

    Report {
        drift_max_m,
        drift_p95_m,
        disconnected,
        flicker_edges,
        short_edges,
        close_nodes,
        spike_tips_removed,
        drift: drift_at,
    }
}

/// The `q`-quantile (0 < q ≤ 1) by nearest rank; 0 for no values.
#[must_use]
pub fn percentile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let rank = ((q * v.len() as f64).ceil() as usize).clamp(1, v.len());
    v[rank - 1]
}

/// C4: same node count, edge count and multiset of per-edge route sets (directions and ids
/// of nodes and edges are ignored, so an equivalent graph built by an algorithm matches).
///
/// # Errors
/// Describes the first difference.
pub fn same_topology(expected: &LineGraph, actual: &LineGraph) -> Result<(), String> {
    if expected.nodes.len() != actual.nodes.len() {
        return Err(format!(
            "C4: {} nodes, expected {}",
            actual.nodes.len(),
            expected.nodes.len()
        ));
    }
    if expected.edges.len() != actual.edges.len() {
        return Err(format!(
            "C4: {} edges, expected {}",
            actual.edges.len(),
            expected.edges.len()
        ));
    }
    let sets = |g: &LineGraph| {
        let mut count: BTreeMap<Vec<String>, usize> = BTreeMap::new();
        for e in &g.edges {
            let mut ids: Vec<String> = e.lines.iter().map(|l| l.line.clone()).collect();
            ids.sort();
            *count.entry(ids).or_default() += 1;
        }
        count
    };
    let (want, got) = (sets(expected), sets(actual));
    if want != got {
        return Err(format!("C4: edge route sets {got:?}, expected {want:?}"));
    }
    Ok(())
}

/// The length of `line`'s `walk` through `graph` (edge lengths `lengths`), an edge walked twice
/// counted twice; `None` if it isn't one walk of that line: empty, a step that doesn't start
/// where the one before it ends, a step on an edge that doesn't list the line (or the other way
/// round), or an edge listing the line that the walk misses (a `both` edge must be walked each
/// way).
fn walk_length(graph: &LineGraph, lengths: &[f64], line: &str, walk: &Walk) -> Option<f64> {
    let ends = |(e, dir): (usize, Direction)| {
        let edge = graph.edges.get(e)?;
        match dir {
            Direction::Forward => Some((edge.from, edge.to)),
            Direction::Backward => Some((edge.to, edge.from)),
            Direction::Both => None, // a step goes one way
        }
    };
    let listed = |e: usize| {
        graph.edges[e]
            .lines
            .iter()
            .find(|l| l.line == line)
            .map(|l| l.dir)
    };
    let mut stepped: BTreeSet<(usize, bool)> = BTreeSet::new();
    let mut len = 0.0;
    let mut at = None;
    for &step in walk {
        let (from, to) = ends(step)?;
        let forward = step.1 == Direction::Forward;
        match listed(step.0)? {
            Direction::Both => {}
            d if (d == Direction::Forward) != forward => return None,
            _ => {}
        }
        if at.is_some_and(|n| n != from) {
            return None;
        }
        at = Some(to);
        stepped.insert((step.0, forward));
        len += lengths[step.0];
    }
    let covered = graph.edges.iter().enumerate().all(|(e, edge)| {
        edge.lines
            .iter()
            .filter(|l| l.line == line)
            .all(|l| match l.dir {
                Direction::Forward => stepped.contains(&(e, true)),
                Direction::Backward => stepped.contains(&(e, false)),
                Direction::Both => stepped.contains(&(e, true)) && stepped.contains(&(e, false)),
            })
    });
    (!walk.is_empty() && covered).then_some(len)
}

/// Whether the arcs (from, to) can all be walked in one go: weakly connected, and every node
/// balanced except at most one start (one more out than in) and one end (one more in than out).
fn is_one_walk(arcs: &[(usize, usize)]) -> bool {
    if arcs.is_empty() {
        return false;
    }
    let mut balance: HashMap<usize, i64> = HashMap::new();
    let mut neighbors: HashMap<usize, Vec<usize>> = HashMap::new();
    for &(a, b) in arcs {
        *balance.entry(a).or_default() += 1;
        *balance.entry(b).or_default() -= 1;
        neighbors.entry(a).or_default().push(b);
        neighbors.entry(b).or_default().push(a);
    }
    let starts = balance.values().filter(|&&v| v == 1).count();
    let ends = balance.values().filter(|&&v| v == -1).count();
    let balanced = balance.values().all(|v| v.abs() <= 1) && starts == ends && starts <= 1;

    let first = arcs[0].0;
    let mut seen = BTreeSet::from([first]);
    let mut stack = vec![first];
    while let Some(n) = stack.pop() {
        for &m in &neighbors[&n] {
            if seen.insert(m) {
                stack.push(m);
            }
        }
    }
    balanced && seen.len() == neighbors.len()
}

/// `points` in order, each flagged `true`, with points flagged `false` every `step` meters along
/// each segment in between (measured from the segment's start, never on its end). A `step` that
/// is not finite or not above zero, and a segment whose length is not finite, add nothing.
fn samples_along(points: &[Point], step: f64) -> Vec<(Point, bool)> {
    let mut out = Vec::with_capacity(points.len());
    let Some(&first) = points.first() else {
        return out;
    };
    out.push((first, true));
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if step.is_finite() && step > 0.0 && len.is_finite() {
            let mut k = 1.0;
            while k * step < len {
                let t = k * step / len;
                out.push((
                    Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)),
                    false,
                ));
                k += 1.0;
            }
        }
        out.push((b, true));
    }
    out
}

fn path_length(points: &[Point]) -> f64 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::{load, load_graph, parse_graph};

    use super::*;

    const ALL: [(&str, &str); 10] = [
        ("synthetic", "dog-bone"),
        ("synthetic", "hairpin"),
        ("synthetic", "opposite-direction"),
        ("synthetic", "parallel-corridor"),
        ("synthetic", "x-crossing"),
        ("synthetic", "y-split"),
        ("bus", "out-and-back"),
        ("bus", "one-way-pair"),
        ("bus", "stops"),
        ("bus", "loop-line"),
    ];

    fn dir(folder: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(folder)
    }

    fn pair(folder: &str, name: &str) -> (Fixture, LineGraph) {
        let f = load(&dir(folder).join(format!("{name}.geojson"))).unwrap();
        let g = load_graph(&dir(folder).join(format!("{name}.graph.json")), &f).unwrap();
        (f, g)
    }

    fn node(g: &LineGraph, id: &str) -> usize {
        g.nodes.iter().position(|n| n.id == id).unwrap()
    }

    fn edge<'a>(g: &'a mut LineGraph, id: &str) -> &'a mut crate::Edge {
        g.edges.iter_mut().find(|e| e.id == id).unwrap()
    }

    #[test]
    fn spike_tips_are_skipped_and_counted() {
        // A 10 m stop pull-in at x = 50. The graph is the street without it.
        let f = Fixture {
            name: "t".to_owned(),
            description: String::new(),
            units: crate::Units::Meters,
            license: String::new(),
            attribution: String::new(),
            expect: String::new(),
            stations: Vec::new(),
            lines: vec![crate::FixtureLine {
                id: "A".to_owned(),
                color: "#000000".to_owned(),
                points: [
                    (0.0, 0.0),
                    (50.0, 0.0),
                    (50.0, 10.0),
                    (50.0, 0.0),
                    (100.0, 0.0),
                ]
                .into_iter()
                .map(|(x, y)| Point::new(x, y))
                .collect(),
                sign: None,
                direction: None,
            }],
        };
        let g = parse_graph(
            r#"{"fixture": "t",
            "nodes": [{"id": "a", "at": [0, 0]}, {"id": "b", "at": [100, 0]}],
            "edges": [{"id": "e", "from": "a", "to": "b", "lines": [{"line": "A", "dir": "forward"}]}]}"#,
            &f,
        )
        .unwrap();
        let r = check(&f, &g);
        assert_eq!(r.spike_tips_removed, 1);
        assert!(r.drift_max_m < 1e-9, "{r:?}");
        assert!(
            r.disconnected.is_empty(),
            "C2 uses the despiked length too: {r:?}"
        );
        assert!(r.failures().is_empty(), "{:?}", r.failures());
    }

    fn pts(xy: &[(f64, f64)]) -> Vec<Point> {
        xy.iter().map(|&(x, y)| Point::new(x, y)).collect()
    }

    #[test]
    fn samples_along_adds_a_point_every_step() {
        let flat = |v: Vec<(Point, bool)>| {
            v.into_iter()
                .map(|(p, o)| (p.x, p.y, o))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            flat(samples_along(&pts(&[(0.0, 0.0), (12.0, 0.0)]), 5.0)),
            [
                (0.0, 0.0, true),
                (5.0, 0.0, false),
                (10.0, 0.0, false),
                (12.0, 0.0, true)
            ]
        );
        // A segment that is an exact multiple of the step: no sample on top of its end.
        assert_eq!(
            flat(samples_along(
                &pts(&[(0.0, 0.0), (0.0, 10.0), (3.0, 10.0)]),
                5.0
            )),
            [
                (0.0, 0.0, true),
                (0.0, 5.0, false),
                (0.0, 10.0, true),
                (3.0, 10.0, true)
            ]
        );
    }

    #[test]
    fn samples_along_keeps_the_points_for_empty_short_and_bad_input() {
        assert!(samples_along(&[], 5.0).is_empty());
        let one = pts(&[(1.0, 2.0)]);
        assert_eq!(samples_along(&one, 5.0), [(one[0], true)]);
        let line = pts(&[(0.0, 0.0), (0.0, 0.0), (20.0, 0.0)]);
        for step in [0.0, -5.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let got = samples_along(&line, step);
            assert_eq!(got.len(), 3, "step {step}");
            assert!(got.iter().all(|&(_, own)| own), "step {step}");
        }
        // A segment of length zero, NaN or infinity adds nothing (and doesn't loop forever).
        for bad in [f64::NAN, f64::INFINITY] {
            let got = samples_along(&pts(&[(0.0, 0.0), (bad, 0.0), (10.0, 0.0)]), 5.0);
            assert_eq!(got.len(), 3, "{bad}");
        }
        assert_eq!(
            samples_along(&line, 5.0).len(),
            6,
            "0, 0, then 5, 10, 15, 20"
        );
    }

    #[test]
    fn a_straight_cut_across_a_bend_fails_drift_between_its_points() {
        // Q6: A's two points sit on its edges, but its straight segment runs 18.6 m from the
        // bend at m. Checking only the points saw 0 m.
        let f = Fixture {
            name: "t".to_owned(),
            description: String::new(),
            units: crate::Units::Meters,
            license: String::new(),
            attribution: String::new(),
            expect: String::new(),
            stations: Vec::new(),
            lines: vec![crate::FixtureLine {
                id: "A".to_owned(),
                color: "#000000".to_owned(),
                points: pts(&[(0.0, 0.0), (100.0, 0.0)]),
                sign: None,
                direction: None,
            }],
        };
        let g = parse_graph(
            r#"{"fixture": "t",
            "nodes": [{"id": "a", "at": [0, 0]}, {"id": "m", "at": [50, 20]}, {"id": "b", "at": [100, 0]}],
            "edges": [{"id": "e1", "from": "a", "to": "m", "lines": [{"line": "A", "dir": "forward"}]},
                      {"id": "e2", "from": "m", "to": "b", "lines": [{"line": "A", "dir": "forward"}]}]}"#,
            &f,
        )
        .unwrap();
        let r = check(&f, &g);
        let at_bend = 20.0 * 50.0 / (50.0_f64 * 50.0 + 20.0 * 20.0).sqrt();
        assert!((r.drift_max_m - at_bend).abs() < 1e-9, "{r:?}");
        assert!(
            r.failures().iter().any(|m| m.starts_with("C1 drift")),
            "{:?}",
            r.failures()
        );
        // Both own points are listed; of the 19 samples between, those over 3 m are too.
        let own = r.drift.iter().filter(|d| d.2 < 1e-9).count();
        assert_eq!(own, 2, "{:?}", r.drift);
        assert!(
            r.drift.iter().all(|d| d.2 < 1e-9 || d.2 > DRIFT_P95_M),
            "{:?}",
            r.drift
        );
        assert!(r.drift.len() > 2, "{:?}", r.drift);
    }

    #[test]
    fn every_expected_graph_is_clean_and_matches_itself() {
        for (folder, name) in ALL {
            let (f, g) = pair(folder, name);
            let r = check(&f, &g);
            assert!(r.failures().is_empty(), "{name}: {:?}", r.failures());
            assert_eq!(same_topology(&g, &g), Ok(()));
        }
    }

    #[test]
    fn a_moved_node_breaks_drift_and_length() {
        // y-split's split node moved 40 m away: the edges no longer follow the lines.
        let (f, mut g) = pair("synthetic", "y-split");
        let split = node(&g, "split");
        g.nodes[split].at = Point::new(540.0, 40.0);
        let fails = check(&f, &g).failures();
        assert!(fails.iter().any(|m| m.starts_with("C1 drift")), "{fails:?}");
        assert!(
            fails
                .iter()
                .any(|m| m.starts_with("C2 connected") && m.contains("walk is")),
            "{fails:?}"
        );
    }

    #[test]
    fn a_missing_edge_breaks_the_walk() {
        // dog-bone: line A loses the shared middle edge, so its two ends are separate pieces.
        let (f, mut g) = pair("synthetic", "dog-bone");
        edge(&mut g, "shared").lines.retain(|l| l.line != "A");
        assert_eq!(check(&f, &g).disconnected, ["A: edges don't form one walk"]);
    }

    #[test]
    fn a_reversed_edge_breaks_the_walk() {
        // y-split: branch-a reversed, so A arrives at the split twice and never leaves.
        let (f, mut g) = pair("synthetic", "y-split");
        let e = edge(&mut g, "branch-a");
        std::mem::swap(&mut e.from, &mut e.to);
        assert_eq!(check(&f, &g).disconnected, ["A: edges don't form one walk"]);
    }

    /// The topology's graph of `loop-twice` and its walks, and route A's index.
    fn loop_twice() -> (Fixture, LineGraph, Vec<Walk>) {
        let f = load(&dir("bus").join("loop-twice.geojson")).unwrap();
        let (g, walks) =
            crate::built::line_graph_walked(&f, &transit_braid_core::topology::Params::default())
                .unwrap();
        (f, g, walks)
    }

    #[test]
    fn a_street_walked_twice_passes_c2_by_its_walk() {
        // Route A drives the block's south side twice the same way; the edge lists A once.
        let (f, g, walks) = loop_twice();
        let twice = walks[0]
            .iter()
            .enumerate()
            .any(|(k, s)| walks[0][k + 1..].contains(s));
        assert!(twice, "A's walk repeats a step: {:?}", walks[0]);
        assert!(check_walked(&f, &g, Some(&walks)).disconnected.is_empty());
        // Without the walk, the edges alone can't be walked in one go.
        assert_eq!(check(&f, &g).disconnected, ["A: edges don't form one walk"]);
    }

    #[test]
    fn a_broken_walk_fails_c2() {
        let (f, g, walks) = loop_twice();
        let c2 = |w: &[Walk]| check_walked(&f, &g, Some(w)).disconnected;
        let broken = "A: edges don't form one walk";
        // A step left out of the middle: the walk jumps.
        let mut gap = walks.clone();
        let middle = gap[0].len() / 2;
        gap[0].remove(middle);
        assert_eq!(c2(&gap), [broken]);
        // No steps at all.
        let mut empty = walks.clone();
        empty[0].clear();
        assert_eq!(c2(&empty), [broken]);
        // A step the wrong way along a one-way edge.
        let mut reversed = walks.clone();
        let last = reversed[0].len() - 1;
        reversed[0][last].1 = Direction::Backward;
        assert_eq!(c2(&reversed), [broken]);
        // A step on an edge that doesn't carry A (B's own edge), or on no edge at all.
        let b_only = g
            .edges
            .iter()
            .position(|e| e.lines.iter().all(|l| l.line == "B"))
            .unwrap();
        let mut foreign = walks.clone();
        foreign[0].push((b_only, Direction::Forward));
        assert_eq!(c2(&foreign), [broken]);
        let mut nowhere = walks.clone();
        nowhere[0].push((g.edges.len(), Direction::Forward));
        assert_eq!(c2(&nowhere), [broken]);
        // A step that is neither way.
        let mut both = walks.clone();
        both[0][0].1 = Direction::Both;
        assert_eq!(c2(&both), [broken]);
        // The last step dropped: the walk is whole but misses an edge that lists A.
        let mut short = walks.clone();
        short[0].pop();
        assert_eq!(c2(&short), [broken]);
    }

    #[test]
    fn a_walk_of_the_wrong_length_fails_c2() {
        // The repeated street walked a third time: the walk is longer than A's input.
        let (f, g, mut walks) = loop_twice();
        let repeat: Vec<(usize, Direction)> = walks[0]
            .iter()
            .enumerate()
            .find(|(k, s)| walks[0][k + 1..].contains(s))
            .map(|(k, _)| walks[0][k..].to_vec())
            .unwrap();
        // Around the block again from the first pass of the repeated street.
        let at = walks[0].iter().position(|s| *s == repeat[0]).unwrap();
        let lap: Vec<(usize, Direction)> = walks[0][at..]
            .iter()
            .copied()
            .take_while({
                let mut seen = 0;
                move |s| {
                    if *s == repeat[0] {
                        seen += 1;
                    }
                    seen < 2
                }
            })
            .collect();
        walks[0].splice(at..at, lap);
        let fails = check_walked(&f, &g, Some(&walks)).disconnected;
        assert_eq!(fails.len(), 1);
        assert!(fails[0].starts_with("A: walk is "), "{fails:?}");
    }

    #[test]
    fn walks_for_some_lines_only_check_the_rest_by_their_edges() {
        // Only A's walk is given: B is checked by its edges as before, and passes.
        let (f, g, walks) = loop_twice();
        assert!(
            check_walked(&f, &g, Some(&walks[..1]))
                .disconnected
                .is_empty()
        );
    }

    #[test]
    fn out_and_back_needs_both_directions() {
        // A runs the street out and back; marked forward only, its walk is half as long.
        let (f, mut g) = pair("bus", "out-and-back");
        for l in &mut edge(&mut g, "street").lines {
            if l.line == "A" {
                l.dir = Direction::Forward;
            }
        }
        let r = check(&f, &g);
        assert_eq!(r.disconnected.len(), 1, "{r:?}");
        assert!(r.disconnected[0].starts_with("A: walk is 600.0 m"), "{r:?}");
    }

    #[test]
    fn tiny_edges_and_close_nodes_are_artifacts_and_flicker() {
        let f = crate::parse(
            r##"{"type": "FeatureCollection",
            "braid": {"name": "t", "description": "d", "units": "meters", "license": "MIT",
                      "attribution": "a", "expect": "e"},
            "features": [
              {"type": "Feature", "properties": {"line": "A", "color": "#000000"},
               "geometry": {"type": "LineString", "coordinates": [[0, 0], [200, 0]]}},
              {"type": "Feature", "properties": {"line": "B", "color": "#000000"},
               "geometry": {"type": "LineString", "coordinates": [[0, 2], [97, 2]]}},
              {"type": "Feature", "properties": {"line": "C", "color": "#000000"},
               "geometry": {"type": "LineString", "coordinates": [[100, -2], [200, -2]]}}
            ]}"##,
        )
        .unwrap();
        // B leaves at 97 m and C joins at 100 m: a 3 m stretch where only A runs.
        let g = parse_graph(
            r#"{"fixture": "t",
              "nodes": [{"id": "w", "at": [0, 0]}, {"id": "p", "at": [97, 0]},
                        {"id": "q", "at": [100, 0]}, {"id": "e", "at": [200, 0]}],
              "edges": [
                {"id": "ab", "from": "w", "to": "p", "lines": [{"line": "A", "dir": "forward"}, {"line": "B", "dir": "forward"}]},
                {"id": "gap", "from": "p", "to": "q", "lines": [{"line": "A", "dir": "forward"}]},
                {"id": "ac", "from": "q", "to": "e", "lines": [{"line": "A", "dir": "forward"}, {"line": "C", "dir": "forward"}]}
              ]}"#,
            &f,
        )
        .unwrap();
        let r = check(&f, &g);
        assert_eq!(r.flicker_edges, ["gap"]);
        assert_eq!(r.short_edges, ["gap"]);
        assert_eq!(r.close_nodes, [("p".to_owned(), "q".to_owned())]);
        let fails = r.failures();
        assert_eq!(fails.len(), 3, "{fails:?}");
    }

    #[test]
    fn same_topology_spots_count_and_set_differences() {
        let (_, y) = pair("synthetic", "y-split");
        let (_, x) = pair("synthetic", "x-crossing");
        let (_, d) = pair("synthetic", "dog-bone");
        assert!(
            same_topology(&y, &x)
                .unwrap_err()
                .contains("4 nodes, expected 5")
        );
        assert!(
            same_topology(&y, &d)
                .unwrap_err()
                .contains("nodes, expected")
        );
        // Same counts, one fewer line on the trunk.
        let mut broken = y.clone();
        edge(&mut broken, "trunk").lines.retain(|l| l.line != "C");
        assert!(
            same_topology(&y, &broken)
                .unwrap_err()
                .contains("edge route sets")
        );
        let mut fewer_edges = y.clone();
        fewer_edges.edges.pop();
        assert!(
            same_topology(&y, &fewer_edges)
                .unwrap_err()
                .contains("3 edges, expected 4")
        );
    }

    #[test]
    fn walks_need_connection_and_balance() {
        assert!(is_one_walk(&[(0, 1), (1, 2)]));
        assert!(is_one_walk(&[(0, 1), (1, 0)])); // out and back
        assert!(!is_one_walk(&[]));
        assert!(!is_one_walk(&[(0, 1), (2, 3)])); // two pieces
        assert!(!is_one_walk(&[(0, 1), (2, 1)])); // two arcs into one node
        assert!(!is_one_walk(&[(0, 1), (0, 2), (0, 3)])); // three starts
    }
}
