//! Step 3b on hand-made runs: where a run that only touches an edge sits on it.

use std::collections::BTreeSet;

use super::Dir;
use super::assemble::{Proto, Run};
use super::keys::Sample;
use super::split::{Span, split_at_run_ends};
use crate::geom::Point;
use crate::ids::{IdVec, LineId};

/// Found in Edmonton's Westmount and Dublin's Adamstown: a run of a few meters where a route
/// meets a long shared edge right at its end (here as it starts a small loop) is a point at
/// that end. Before, it took the whole edge, so one of its ends was the edge's far end, 700 m
/// away; continuity joined the route's next run there and put a node off the street, and
/// its edges cut straight across the blocks.
#[test]
fn a_run_at_one_end_of_an_edge_is_a_point_there_not_the_whole_edge() {
    let at = sample_arc;
    let step = 2.0;
    // Route 0 runs west along y = 0 from x = 700 to 0: the shared edge's path.
    let line0: Vec<Sample> = (0..=350)
        .map(|i| at(700.0 - step * f64::from(i), 0.0, step * f64::from(i)))
        .collect();
    // Route 1 runs east along it to x = 698, meets it for two samples at the very end,
    // then turns north onto another edge.
    let mut line1: Vec<Sample> = (0..=349)
        .map(|i| at(step * f64::from(i), 0.0, step * f64::from(i)))
        .collect();
    line1.extend([at(699.0, 0.0, 699.0), at(700.0, 0.0, 700.0)]);
    line1.extend((1..=150).map(|i| at(700.0, step * f64::from(i), 700.0 + step * f64::from(i))));
    let shared = BTreeSet::from([LineId::new(0), LineId::new(1)]);
    let runs = vec![
        Run {
            line: LineId::new(0),
            key: (shared.clone(), false),
            first: 0,
            last: 350,
        },
        Run {
            line: LineId::new(1),
            key: (shared.clone(), false),
            first: 0,
            last: 349,
        },
        Run {
            line: LineId::new(1),
            key: (shared, true),
            first: 350,
            last: 351,
        },
        Run {
            line: LineId::new(1),
            key: (BTreeSet::from([LineId::new(1)]), false),
            first: 352,
            last: 501,
        },
    ];
    // Route 1 runs against the shared path (west); its last run is on edge 1 (north).
    let run_edge = vec![(0, true), (0, false), (0, false), (1, true)];
    let protos = vec![
        Proto {
            path: vec![Point::new(700.0, 0.0), Point::new(0.0, 0.0)],
            lines: vec![
                (LineId::new(0), Dir::Forward),
                (LineId::new(1), Dir::Backward),
            ],
        },
        Proto {
            path: vec![Point::new(700.0, 0.0), Point::new(700.0, 300.0)],
            lines: vec![(LineId::new(1), Dir::Forward)],
        },
    ];
    let samples: IdVec<LineId, &[Sample]> = IdVec::from(vec![&line0[..], &line1[..]]);
    let (pieces, spans) = split_at_run_ends(protos, &runs, &run_edge, &samples, 5.0);
    let ends: Vec<Point> = pieces
        .iter()
        .flat_map(|p| [p.path[0], p.path[p.path.len() - 1]])
        .collect();
    let (from, to) = start_end(spans[2]);
    let near = Point::new(700.0, 0.0);
    assert!(
        ends[from].distance(near) <= 5.0,
        "starts at {:?}",
        ends[from]
    );
    assert!(ends[to].distance(near) <= 5.0, "ends at {:?}", ends[to]);
    // The edge still carries both routes: route 1's long run covers it.
    assert_eq!(
        pieces[0].lines,
        vec![
            (LineId::new(0), Dir::Forward),
            (LineId::new(1), Dir::Backward)
        ]
    );
    // The other runs are as before: along the edge, and north on edge 1.
    assert_eq!(start_end(spans[1]), (1, 0));
    assert_eq!(start_end(spans[3]), (2, 3));
}

/// A sample at `(x, y)`, `arc` meters along its route (heading unused here).
fn sample_arc(x: f64, y: f64, arc: f64) -> Sample {
    Sample {
        at: Point::new(x, y),
        dir: (1.0, 0.0),
        arc,
    }
}

/// Samples every 2 m from `a` to `b` (both included), arcs going on from `arc`.
fn samples_from(a: (f64, f64), b: (f64, f64), arc: f64) -> Vec<Sample> {
    let len = (b.0 - a.0).abs().max((b.1 - a.1).abs());
    let n = (len / 2.0).round() as u32;
    (0..=n)
        .map(|i| {
            let t = f64::from(i) / f64::from(n);
            sample_arc(
                a.0 + (b.0 - a.0) * t,
                a.1 + (b.1 - a.1) * t,
                arc + 2.0 * f64::from(i),
            )
        })
        .collect()
}

/// Where a run starts and ends (piece ends: 2 × piece, 2 × piece + 1), as step 4 reads it.
fn start_end(s: Span) -> (usize, usize) {
    match (s.at, s.forward) {
        (Some(e), _) => (e, e),
        (None, true) => (2 * s.first, 2 * s.last + 1),
        (None, false) => (2 * s.last + 1, 2 * s.first),
    }
}

/// Found on Dublin's 15-0 at a turnaround: a run of a few meters where a route crosses a
/// shared edge at a cut, reaching more than `merge_m` past it but to no piece's middle, is a
/// point at that cut. Before, it took the whole piece beyond the cut, so its end was that
/// piece's far end, 500 m away; continuity joined the route's next street there, and the
/// node between them sat halfway, off the street.
#[test]
fn a_short_run_beside_a_cut_is_a_point_at_the_cut_not_the_piece_beyond() {
    // Route 0 runs east along y = 0 from x = 0 to 1000: the shared edge's path.
    let line0 = samples_from((0.0, 0.0), (1000.0, 0.0), 0.0);
    // Route 1 comes south down x = 500, touches the edge for 16 m (out and back), and goes
    // on south.
    let mut line1 = samples_from((500.0, 300.0), (500.0, 2.0), 0.0);
    let p_last = line1.len() - 1;
    line1.extend(samples_from((500.0, 0.0), (516.0, 0.0), 300.0));
    line1.extend(samples_from((514.0, 0.0), (500.0, 0.0), 316.0));
    let q_last = line1.len() - 1;
    line1.extend(samples_from((500.0, -2.0), (500.0, -300.0), 332.0));
    let r_last = line1.len() - 1;
    let shared = BTreeSet::from([LineId::new(0), LineId::new(1)]);
    let alone = BTreeSet::from([LineId::new(1)]);
    let runs = vec![
        Run {
            line: LineId::new(0),
            key: (shared.clone(), false),
            first: 0,
            last: line0.len() - 1,
        },
        Run {
            line: LineId::new(1),
            key: (alone.clone(), false),
            first: 0,
            last: p_last,
        },
        Run {
            line: LineId::new(1),
            key: (shared, true),
            first: p_last + 1,
            last: q_last,
        },
        Run {
            line: LineId::new(1),
            key: (alone, false),
            first: q_last + 1,
            last: r_last,
        },
    ];
    let run_edge = vec![(0, true), (1, true), (0, true), (2, true)];
    let protos = vec![
        Proto {
            path: vec![Point::new(0.0, 0.0), Point::new(1000.0, 0.0)],
            lines: vec![
                (LineId::new(0), Dir::Forward),
                (LineId::new(1), Dir::Forward),
            ],
        },
        Proto {
            path: vec![Point::new(500.0, 300.0), Point::new(500.0, 0.0)],
            lines: vec![(LineId::new(1), Dir::Forward)],
        },
        Proto {
            path: vec![Point::new(500.0, 0.0), Point::new(500.0, -300.0)],
            lines: vec![(LineId::new(1), Dir::Forward)],
        },
    ];
    let samples: IdVec<LineId, &[Sample]> = IdVec::from(vec![&line0[..], &line1[..]]);
    let (pieces, spans) = split_at_run_ends(protos, &runs, &run_edge, &samples, 5.0);
    let ends: Vec<Point> = pieces
        .iter()
        .flat_map(|p| [p.path[0], p.path[p.path.len() - 1]])
        .collect();
    // The shared edge is cut where route 1 meets it: two pieces, then the two streets.
    assert_eq!(pieces.len(), 4);
    let (from, to) = start_end(spans[2]);
    let cut = Point::new(500.0, 0.0);
    assert!(
        ends[from].distance(cut) <= 5.0,
        "starts at {:?}",
        ends[from]
    );
    assert!(ends[to].distance(cut) <= 5.0, "ends at {:?}", ends[to]);
    // Neither piece carries route 1: it only crosses there.
    assert_eq!(pieces[0].lines, vec![(LineId::new(0), Dir::Forward)]);
    assert_eq!(pieces[1].lines, vec![(LineId::new(0), Dir::Forward)]);
    // The streets before and after it end and start at the cut too.
    assert!(ends[start_end(spans[1]).1].distance(cut) <= 5.0);
    assert!(ends[start_end(spans[3]).0].distance(cut) <= 5.0);
}

/// Found on Dublin's 27-1 at its terminal loop: a run that is a point at an edge's end adds no
/// direction to the edge. Before, the edge kept the direction it had with that run (both
/// ways), so the route's walk counted the edge twice and its length did not add up.
#[test]
fn a_point_run_adds_no_direction_to_its_edge() {
    // Route 0 runs east along the 100 m edge, north up a street, back down a diagonal
    // to the edge's east end, touches the edge for 2 m going west, and goes on south.
    let mut line = samples_from((0.0, 0.0), (100.0, 0.0), 0.0);
    let a_last = line.len() - 1;
    line.extend(samples_from((100.0, 2.0), (100.0, 60.0), 100.0));
    let b_last = line.len() - 1;
    line.extend(samples_from((130.0, 60.0), (102.0, 2.0), 160.0));
    let c_last = line.len() - 1;
    line.extend([sample_arc(99.0, 0.0, 230.0), sample_arc(97.0, 0.0, 232.0)]);
    let d_last = line.len() - 1;
    line.extend(samples_from((97.0, -2.0), (97.0, -60.0), 234.0));
    let e_last = line.len() - 1;
    let key = (BTreeSet::from([LineId::new(0)]), false);
    let bounds = [(0, a_last), (a_last + 1, b_last), (b_last + 1, c_last)];
    let mut runs: Vec<Run> = bounds
        .iter()
        .map(|&(first, last)| Run {
            line: LineId::new(0),
            key: key.clone(),
            first,
            last,
        })
        .collect();
    runs.extend(
        [(c_last + 1, d_last), (d_last + 1, e_last)].map(|(first, last)| Run {
            line: LineId::new(0),
            key: key.clone(),
            first,
            last,
        }),
    );
    // Edge 0 the street along y = 0; its second run goes against it (west).
    let run_edge = vec![(0, true), (1, true), (2, true), (0, false), (3, true)];
    let protos = vec![
        Proto {
            path: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
            // As step 3 gives it: one run each way.
            lines: vec![(LineId::new(0), Dir::Both)],
        },
        Proto {
            path: vec![Point::new(100.0, 0.0), Point::new(100.0, 60.0)],
            lines: vec![(LineId::new(0), Dir::Forward)],
        },
        Proto {
            path: vec![Point::new(130.0, 60.0), Point::new(100.0, 0.0)],
            lines: vec![(LineId::new(0), Dir::Forward)],
        },
        Proto {
            path: vec![Point::new(97.0, 0.0), Point::new(97.0, -60.0)],
            lines: vec![(LineId::new(0), Dir::Forward)],
        },
    ];
    let samples: IdVec<LineId, &[Sample]> = IdVec::from(vec![&line[..]]);
    let (pieces, spans) = split_at_run_ends(protos, &runs, &run_edge, &samples, 5.0);
    assert_eq!(pieces.len(), 4);
    // The 2 m run is a point at the edge's east end, and the edge goes one way only.
    assert_eq!(start_end(spans[3]), (1, 1));
    assert_eq!(pieces[0].lines, vec![(LineId::new(0), Dir::Forward)]);
    assert_eq!(start_end(spans[0]), (0, 1));
}
