//! The whole build on small inputs: edges, nodes and walks, bad input, and the graph's
//! properties on random routes.

use std::collections::BTreeSet;

use proptest::prelude::*;

use super::assemble::runs_along;
use super::keys::Sample;
use super::test_support::routes;
use super::*;
use crate::ids::{EdgeId, IdVec, LineId, NodeId};

fn lines(input: &[&[(f64, f64)]]) -> Vec<Vec<Point>> {
    input
        .iter()
        .map(|l| l.iter().map(|&(x, y)| Point::new(x, y)).collect())
        .collect()
}

fn ok(input: &[&[(f64, f64)]]) -> Topology {
    build(&lines(input), &Params::default()).unwrap()
}

fn walked(input: &[&[(f64, f64)]]) -> (Topology, IdVec<LineId, Traversal>) {
    build_with_traversals(&lines(input), &Params::default()).unwrap()
}

/// Where step `s` of a walk starts and ends (nodes), given the way it runs.
fn ends_of(t: &Topology, (e, dir): (EdgeId, Dir)) -> (NodeId, NodeId) {
    let edge = &t.edges[e];
    if dir == Dir::Backward {
        (edge.to, edge.from)
    } else {
        (edge.from, edge.to)
    }
}

/// Each step starts at the node where the one before it ends.
fn connected(t: &Topology, walk: &Traversal) -> bool {
    walk.windows(2)
        .all(|w| ends_of(t, w[0]).1 == ends_of(t, w[1]).0)
}

#[test]
fn a_y_split_walks_the_trunk_then_its_own_branch() {
    // A goes straight on, B turns north after 300 m, C turns south.
    let (t, walks) = walked(&[
        &[(0.0, 0.0), (300.0, 0.0), (600.0, 0.0)],
        &[(0.0, 1.0), (300.0, 1.0), (500.0, 200.0)],
        &[(0.0, -1.0), (300.0, -1.0), (500.0, -200.0)],
    ]);
    assert_eq!(t.edges.len(), 4, "{t:?}");
    let trunk = t
        .edges
        .iter()
        .position(|e| e.lines.len() == 3)
        .map(EdgeId::new)
        .unwrap();
    for (i, walk) in walks.iter_enumerated() {
        assert_eq!(walk.len(), 2, "route {i}: {walk:?}");
        assert_eq!(walk[0].0, trunk);
        assert_ne!(walk[1].0, trunk);
        assert!(t.edges[walk[1].0].lines.iter().any(|&(l, _)| l == i));
        assert!(connected(&t, walk), "route {i}: {walk:?}");
    }
    // Each branch is a different edge.
    let branches: BTreeSet<EdgeId> = walks.iter().map(|w| w[1].0).collect();
    assert_eq!(branches.len(), 3);
}

#[test]
fn crossing_lines_each_walk_their_own_single_edge() {
    // No node at a plain crossing: each line is one edge, walked once.
    let (t, walks) = walked(&[
        &[(0.0, 0.0), (400.0, 0.0)],
        &[(200.0, -200.0), (200.0, 200.0)],
    ]);
    assert_eq!(t.edges.len(), 2);
    for (i, walk) in walks.iter_enumerated() {
        assert_eq!(walk.len(), 1, "{walk:?}");
        assert_eq!(t.edges[walk[0].0].lines, vec![(i, walk[0].1)]);
    }
}

#[test]
fn an_out_and_back_walks_its_shared_street_twice_but_takes_one_slot() {
    // Out 300 m east and back along the same street, then 200 m north from the start.
    let (t, walks) = walked(&[&[(0.0, 0.0), (300.0, 0.0), (0.0, 0.5), (0.0, 200.0)]]);
    let shared = t
        .edges
        .iter()
        .position(|e| e.lines == vec![(LineId::new(0), Dir::Both)])
        .map(EdgeId::new)
        .expect("the street both ways is one edge listing the route once");
    let walk = &walks[LineId::new(0)];
    let on_shared: Vec<Dir> = walk.iter().filter(|s| s.0 == shared).map(|s| s.1).collect();
    assert_eq!(on_shared.len(), 2, "{walk:?}");
    assert_ne!(on_shared[0], on_shared[1], "once each way");
    assert!(walk.iter().all(|s| s.1 != Dir::Both));
    assert!(connected(&t, walk), "{walk:?}");
}

#[test]
fn walks_match_build_and_cover_every_edge_of_their_route() {
    let input = lines(&[
        &[(0.0, 0.0), (300.0, 0.0)],
        &[(0.0, 2.0), (300.0, 2.0), (300.0, 300.0)],
    ]);
    let (t, walks) = build_with_traversals(&input, &Params::default()).unwrap();
    assert_eq!(t, build(&input, &Params::default()).unwrap());
    assert_eq!(walks.len(), 2);
    assert!(
        build_with_traversals(&[], &Params::default())
            .unwrap()
            .1
            .is_empty()
    );
    let bad = Params {
        merge_m: -1.0,
        ..Params::default()
    };
    assert!(build_with_traversals(&input, &bad).is_err());
}

#[test]
fn shared_stretch_splits_into_a_bundle_and_branches() {
    // A and B share 200 m, then B turns away: 3 nodes, 2 edges... plus B's own branch.
    let t = ok(&[
        &[(0.0, 0.0), (400.0, 0.0)],
        &[(0.0, 2.0), (200.0, 2.0), (300.0, 200.0)],
    ]);
    assert_eq!((t.nodes.len(), t.edges.len()), (4, 3));
    let shared = t.edges.iter().find(|e| e.lines.len() == 2).unwrap();
    assert_eq!(
        shared.lines,
        vec![
            (LineId::new(0), Dir::Forward),
            (LineId::new(1), Dir::Forward)
        ]
    );
}

#[test]
fn opposite_directions_share_an_edge() {
    let t = ok(&[&[(0.0, 0.0), (300.0, 0.0)], &[(300.0, 2.0), (0.0, 2.0)]]);
    assert_eq!(t.edges.len(), 1);
    let dirs: Vec<Dir> = t.edges[EdgeId::new(0)].lines.iter().map(|l| l.1).collect();
    assert!(
        dirs.contains(&Dir::Forward) && dirs.contains(&Dir::Backward),
        "{dirs:?}"
    );
}

#[test]
fn out_and_back_takes_one_slot_both_ways() {
    // Line 0 goes out and comes back 3 m away; line 1 runs alongside and carries on.
    let t = ok(&[
        &[
            (0.0, 0.0),
            (600.0, 0.0),
            (602.0, 1.5),
            (600.0, 3.0),
            (0.0, 3.0),
        ],
        &[(0.0, -2.0), (600.0, -2.0), (900.0, -2.0)],
    ]);
    let shared = t.edges.iter().find(|e| e.lines.len() == 2).unwrap();
    assert_eq!(shared.lines[0], (LineId::new(0), Dir::Both));
}

#[test]
fn a_spike_on_a_shared_stretch_keeps_the_stretch() {
    // Dublin route 27: two ~10 m spikes on a street shared with the other direction. Before
    // `despike`, the stretch landed on one edge whose ends became one node: dropped.
    let spiky: &[(f64, f64)] = &[
        (0.0, 0.0),
        (100.0, 0.0),
        (108.0, 6.0),
        (100.0, 0.0),
        (400.0, 0.0),
        (395.0, 9.0),
        (400.0, 0.0),
        (1000.0, 0.0),
    ];
    let t = ok(&[spiky, &[(1000.0, 2.0), (0.0, 2.0)]]);
    let shared = t.edges.iter().filter(|e| e.lines.len() == 2);
    let length: f64 = shared
        .map(|e| e.path.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
        .sum();
    assert!(length > 990.0, "shared length {length}: {t:?}");
}

#[test]
fn a_circular_route_keeps_its_loop() {
    // A loop whose end comes back within the merge distance of its start: both ends become
    // one node. It used to be dropped as a collapsed edge; now it is split in two.
    let t = ok(&[&[(0.0, 0.0), (200.0, 0.0), (100.0, 150.0), (2.0, 1.0)]]);
    assert_eq!(t.edges.len(), 2, "{t:?}");
    assert!(
        t.edges
            .iter()
            .all(|e| e.from != e.to && e.lines == vec![(LineId::new(0), Dir::Forward)])
    );
    let length: f64 = t
        .edges
        .iter()
        .map(|e| e.path.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
        .sum();
    assert!(length > 500.0, "loop length {length}");
}

#[test]
fn lines_farther_apart_than_the_merge_distance_stay_separate() {
    let input = lines(&[&[(0.0, 0.0), (100.0, 0.0)], &[(0.0, 6.0), (100.0, 6.0)]]);
    let t = build(&input, &Params::default()).unwrap();
    assert_eq!((t.nodes.len(), t.edges.len()), (4, 2));
    let wide = Params {
        merge_m: 7.0,
        ..Params::default()
    };
    let t = build(&input, &wide).unwrap();
    assert_eq!((t.nodes.len(), t.edges.len()), (2, 1));
    assert_eq!(t.edges[EdgeId::new(0)].lines.len(), 2);
}

#[test]
fn empty_and_single_inputs() {
    assert_eq!(ok(&[]), Topology::default());
    let one = ok(&[&[(0.0, 0.0), (50.0, 0.0)]]);
    assert_eq!((one.nodes.len(), one.edges.len()), (2, 1));
    assert_eq!(
        one.edges[EdgeId::new(0)].lines,
        vec![(LineId::new(0), Dir::Forward)]
    );
    assert_eq!(
        one.edges[EdgeId::new(0)].path.first(),
        Some(&one.nodes[one.edges[EdgeId::new(0)].from])
    );
}

#[test]
fn bad_routes_are_errors_not_panics() {
    let p = Params::default();
    let nan = vec![vec![Point::new(0.0, 0.0), Point::new(f64::NAN, 1.0)]];
    assert_eq!(build(&nan, &p), Err(TopologyError::NotFinite { route: 0 }));
    let inf = lines(&[
        &[(0.0, 0.0), (1.0, 0.0)],
        &[(0.0, 0.0), (f64::INFINITY, 0.0)],
    ]);
    assert_eq!(build(&inf, &p), Err(TopologyError::NotFinite { route: 1 }));
    for short in [
        lines(&[&[]]),
        lines(&[&[(1.0, 1.0)]]),
        lines(&[&[(3.0, 3.0), (3.0, 3.0)]]),
    ] {
        assert_eq!(build(&short, &p), Err(TopologyError::TooShort { route: 0 }));
    }
    let msg = TopologyError::TooShort { route: 2 }.to_string();
    assert_eq!(msg, "route 2: needs at least two distinct points");
}

#[test]
fn bad_settings_are_rejected() {
    let input = lines(&[&[(0.0, 0.0), (1.0, 0.0)]]);
    let d = Params::default();
    for (params, setting) in [
        (Params { merge_m: 0.0, ..d }, "merge_m"),
        (
            Params {
                merge_m: f64::NAN,
                ..d
            },
            "merge_m",
        ),
        (Params { step_m: -1.0, ..d }, "step_m"),
        (
            Params {
                step_m: f64::INFINITY,
                ..d
            },
            "step_m",
        ),
        (
            Params {
                angle_deg: 0.0,
                ..d
            },
            "angle_deg",
        ),
        (
            Params {
                angle_deg: 90.0,
                ..d
            },
            "angle_deg",
        ),
    ] {
        let err = build(&input, &params).unwrap_err();
        assert!(err.to_string().contains(setting), "{err}");
    }
}

fn length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

#[test]
fn a_route_that_loops_off_and_rejoins_gets_a_node_where_it_leaves() {
    // Dublin's routes 4/9/140/155: B leaves the street A runs on at (300, 0), goes around a
    // block and comes back to the same point, then carries on with A. A never changes there,
    // so without the edge split the shared edge had no node at (300, 0) and continuity glued
    // its two far ends to that point: one node at their average, off the street.
    let street: &[(f64, f64)] = &[(0.0, 0.0), (900.0, 0.0)];
    let looping: &[(f64, f64)] = &[
        (0.0, 2.0),
        (300.0, 2.0),
        (300.0, -100.0),
        (200.0, -100.0),
        (200.0, -40.0),
        (300.0, 2.0),
        (900.0, 2.0),
    ];
    let t = ok(&[street, looping]);
    assert!(
        t.nodes
            .iter()
            .any(|n| n.distance(Point::new(300.0, 1.0)) <= 5.0),
        "a node where B leaves and rejoins: {t:?}"
    );
    // A's edges stay on its street and add up to its length.
    let on_a: Vec<&TopoEdge> = t
        .edges
        .iter()
        .filter(|e| e.lines.iter().any(|l| l.0 == LineId::new(0)))
        .collect();
    assert!(
        on_a.iter().all(|e| e.path.iter().all(|p| p.y.abs() <= 5.0)),
        "{t:?}"
    );
    let a_len: f64 = on_a.iter().map(|e| length(&e.path)).sum();
    assert!((a_len - 900.0).abs() < 18.0, "A's edges: {a_len} m");
    // B's edges add up to its own length (within 2%): no detour to a far node.
    let b_input = length(&lines(&[looping])[0]);
    let b_len: f64 = t
        .edges
        .iter()
        .filter(|e| e.lines.iter().any(|l| l.0 == LineId::new(1)))
        .map(|e| length(&e.path))
        .sum();
    assert!(
        (b_len - b_input).abs() <= 0.02 * b_input,
        "B's edges: {b_len} m of {b_input} m"
    );
}

#[test]
fn a_loop_line_is_cut_where_it_starts_and_ends_on_a_shared_street() {
    // The `loop-line` fixture: A goes around a block, starting and ending at (0, -200)
    // on the street B runs along. Without the cut, each of A's two runs there claimed the
    // whole shared edge, so A's walk ran it twice and its line jumped across the block.
    let looping: &[(f64, f64)] = &[
        (0.0, -200.0),
        (300.0, -200.0),
        (300.0, 200.0),
        (-300.0, 200.0),
        (-300.0, -200.0),
        (0.0, -200.0),
    ];
    let street: &[(f64, f64)] = &[(-600.0, -202.0), (600.0, -202.0)];
    let (t, walks) = build_with_traversals(&lines(&[looping, street]), &Params::default()).unwrap();
    assert_eq!((t.nodes.len(), t.edges.len()), (5, 5), "{t:?}");
    assert!(
        t.nodes
            .iter()
            .any(|n| n.distance(Point::new(0.0, -201.0)) <= 5.0),
        "a node where A starts and ends: {t:?}"
    );
    // A walks each piece once, three steps in all, adding up to its length.
    assert_eq!(walks[LineId::new(0)].len(), 3, "{walks:?}");
    let walked: f64 = walks[LineId::new(0)]
        .iter()
        .map(|&(e, _)| length(&t.edges[e].path))
        .sum();
    assert!(
        (walked - 2000.0).abs() <= 40.0,
        "A walks {walked} m of 2000 m"
    );
    // B is unchanged: one walk along the street.
    assert_eq!(walks[LineId::new(1)].len(), 4, "{walks:?}");
}

#[test]
fn a_loop_line_on_its_own_keeps_its_loop() {
    // The same loop with nothing beside it: its two ends meet at one node, one edge around.
    let looping: &[(f64, f64)] = &[
        (0.0, -200.0),
        (300.0, -200.0),
        (300.0, 200.0),
        (-300.0, 200.0),
        (-300.0, -200.0),
        (0.0, -200.0),
    ];
    let (t, walks) = build_with_traversals(&lines(&[looping]), &Params::default()).unwrap();
    let walked: f64 = walks[LineId::new(0)]
        .iter()
        .map(|&(e, _)| length(&t.edges[e].path))
        .sum();
    assert!(
        (walked - 2000.0).abs() <= 40.0,
        "walks {walked} m of 2000 m: {t:?}"
    );
    assert!(
        t.edges.iter().all(|e| e.from != e.to),
        "no self-loop: {t:?}"
    );
}

fn pts(xy: &[(f64, f64)]) -> Vec<Point> {
    xy.iter().map(|&(x, y)| Point::new(x, y)).collect()
}

/// How far `p` is from the nearest of `lines`.
fn off_input(p: Point, lines: &[Vec<Point>]) -> f64 {
    lines
        .iter()
        .flat_map(|l| l.windows(2))
        .map(|w| distance_to_segment(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

fn sample_at(x: f64, y: f64, dir: (f64, f64)) -> Sample {
    Sample {
        at: Point::new(x, y),
        dir,
        arc: 0.0,
    }
}

#[test]
fn a_run_through_a_sharp_corner_reads_by_its_ends() {
    // A V: 100 m west, then back east-north-east. The middle sample sits on the corner with
    // the second leg's direction, while the nearest leg there is the first: the old reading
    // (direction at the middle) said backward.
    let path = pts(&[(100.0, 0.0), (0.0, 0.0), (100.0, 5.0)]);
    let leg2 = (100.0 / 100.125_f64, 5.0 / 100.125_f64);
    let run = [
        sample_at(100.0, 0.0, (-1.0, 0.0)),
        sample_at(0.0, 0.0, leg2),
        sample_at(100.0, 5.0, leg2),
    ];
    assert!(runs_along(&path, &run, 2.0));
    let mut back = run;
    back.reverse();
    assert!(!runs_along(&path, &back, 2.0));
}

#[test]
fn a_run_about_one_sample_long_reads_by_its_direction() {
    let path = pts(&[(0.0, 0.0), (10.0, 0.0)]);
    assert!(runs_along(&path, &[sample_at(5.0, 0.0, (1.0, 0.0))], 2.0));
    assert!(!runs_along(&path, &[sample_at(5.0, 0.0, (-1.0, 0.0))], 2.0));
    let close = [
        sample_at(5.0, 0.0, (-1.0, 0.0)),
        sample_at(6.0, 0.0, (-1.0, 0.0)),
    ];
    assert!(
        !runs_along(&path, &close, 2.0),
        "1 m apart: within one step"
    );
    assert!(
        runs_along(&path, &[], 2.0),
        "an empty run counts as forward"
    );
}

/// Found by the pipeline's property test: two lines crossing at a shallow angle,
/// opposite ways, the second turning sharply after the shared stretch. A node landed 82 m from
/// one line and 69 m from the other.
#[test]
fn a_shallow_crossing_keeps_its_nodes_on_the_lines() {
    let lines = vec![
        pts(&[(106.9, 35.2), (257.2, 251.6)]),
        pts(&[(273.3, 191.9), (-68.3, -34.4), (-54.2, 235.4)]),
    ];
    let t = build(&lines, &Params::default()).unwrap();
    for n in &t.nodes {
        let d = off_input(*n, &lines);
        assert!(d < 5.0, "node {n:?} is {d} m off the lines");
    }
}

/// A minimal case from the property test below: a short shared stretch, then the second line
/// zigzags back across the first. A node landed 61 m off both lines.
#[test]
fn a_zigzag_after_a_shared_stretch_keeps_its_nodes_on_the_lines() {
    let lines = vec![
        pts(&[
            (0.0, 251.920_979_834_751_9),
            (215.767_776_797_866_28, 255.170_107_611_620_3),
        ]),
        pts(&[
            (205.203_546_277_283_66, 252.688_030_604_952_22),
            (250.787_831_112_613_3, 234.797_945_110_671_3),
            (57.850_275_723_490_48, 197.253_032_474_772_23),
            (257.454_138_070_141_25, 69.522_877_211_709_31),
        ]),
    ];
    let t = build(&lines, &Params::default()).unwrap();
    for n in &t.nodes {
        let d = off_input(*n, &lines);
        assert!(d < 5.0, "node {n:?} is {d} m off the lines");
    }
}

proptest! {
    /// Every node is within two merge distances of the input (a node joins edge ends
    /// closer than one, each on its own route).
    #[test]
    fn every_node_is_near_the_input(input in routes()) {
        let params = Params::default();
        if let Ok(t) = build(&input, &params) {
            for n in &t.nodes {
                let d = off_input(*n, &input);
                prop_assert!(d <= 2.0 * params.merge_m, "node {:?} is {} m off", n, d);
            }
        }
    }

    /// Walks agree with the edges: every step is on an edge that lists its route, every
    /// edge listing a route is on its walk, no step is `Both`, and no step repeats at once.
    #[test]
    fn walks_agree_with_the_edges(input in routes()) {
        let (t, walks) = build_with_traversals(&input, &Params::default()).unwrap();
        prop_assert_eq!(walks.len(), input.len());
        for (i, walk) in walks.iter_enumerated() {
            for &(e, dir) in walk {
                prop_assert!(t.edges.get(e).is_some());
                prop_assert!(dir != Dir::Both);
                prop_assert!(t.edges[e].lines.iter().any(|&(l, _)| l == i), "step {:?} of {}", (e, dir), i);
            }
            prop_assert!(walk.windows(2).all(|w| w[0] != w[1]), "{:?}", walk);
            for (e, edge) in t.edges.iter_enumerated() {
                if edge.lines.iter().any(|&(l, _)| l == i) {
                    prop_assert!(walk.iter().any(|s| s.0 == e), "edge {} not walked by {}", e, i);
                }
            }
        }
    }

    /// Any valid input gives a well-formed graph, the same one every time.
    #[test]
    fn any_input_gives_a_well_formed_deterministic_graph(input in routes()) {
        let t = build(&input, &Params::default()).unwrap();
        prop_assert_eq!(&t, &build(&input, &Params::default()).unwrap());
        for e in &t.edges {
            prop_assert!(t.nodes.get(e.from).is_some() && t.nodes.get(e.to).is_some() && e.from != e.to);
            prop_assert!(e.path.len() >= 2);
            prop_assert_eq!(e.path[0], t.nodes[e.from]);
            prop_assert_eq!(e.path[e.path.len() - 1], t.nodes[e.to]);
            prop_assert!(!e.lines.is_empty());
            prop_assert!(e.lines.windows(2).all(|w| w[0].0 < w[1].0), "sorted, each once");
            prop_assert!(e.lines.iter().all(|&(l, _)| l.index() < input.len()));
        }
        prop_assert!(t.nodes.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
        // No route is lost: each one longer than the merge distance is on some edge.
        for (i, l) in input.iter().enumerate() {
            let length: f64 = l.windows(2).map(|w| w[0].distance(w[1])).sum();
            if length > Params::default().merge_m {
                prop_assert!(
                    t.edges.iter().any(|e| e.lines.iter().any(|&(k, _)| k.index() == i)),
                    "route {} is on no edge", i
                );
            }
        }
    }
}
