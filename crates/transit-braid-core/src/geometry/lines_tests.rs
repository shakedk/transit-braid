//! Lines in their slots along their walks: offsets, node fronts, connectors, loop lines.

use proptest::prelude::*;

use super::test_support::*;
use super::walk::{CONNECTOR_STEPS, connector, sub_path};
use super::*;
use crate::ids::{EdgeId, LineId, NodeId};
use crate::topology::TopoEdge;

#[test]
fn slots_are_centered_one_pitch_apart() {
    assert_eq!(slot_offset(0, 1), 0.0);
    assert_eq!((slot_offset(0, 2), slot_offset(1, 2)), (-0.5, 0.5));
    assert_eq!(
        (slot_offset(0, 3), slot_offset(1, 3), slot_offset(2, 3)),
        (-1.0, 0.0, 1.0)
    );
    assert_eq!(slot_offset(0, 0), 0.0, "no lines: nothing to center");
}

/// Neighbors one pitch apart, on a straight corridor: three lines on one straight edge sit exactly one
/// pitch apart, edge to edge `spacing`, in the order's slots (slot 0 on the right).
#[test]
fn a_straight_corridor_has_its_lines_one_pitch_apart_in_order() {
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(300.0, 0.0)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), (300.0, 0.0)], &[7, 8, 9])].into(),
    };
    let order = order_of(vec![vec![9, 7, 8]]);
    let p = edge_pieces(&t, &order, 5.0);
    let at: Vec<(usize, f64)> = p[EdgeId::new(0)]
        .iter()
        .map(|(l, pts)| (l.index(), pts[0].y))
        .collect();
    assert_eq!(at, vec![(9, -5.0), (7, 0.0), (8, 5.0)]);
    for (_, pts) in &p[EdgeId::new(0)] {
        assert!(
            pts.iter().all(|q| (q.y - pts[0].y).abs() < 1e-9),
            "straight and parallel"
        );
    }
}

/// A loop line that starts and ends on a street it shares, through the real topology
/// and ordering. Its drawn line closes exactly (its end on its start) instead of overlapping
/// itself by a few centimeters, and it stays in one slot around the closure.
#[test]
fn a_loop_line_closes_on_itself() {
    let p =
        |pts: &[(f64, f64)]| -> Vec<Point> { pts.iter().map(|&(x, y)| Point::new(x, y)).collect() };
    let inputs = vec![
        p(&[
            (0.0, -200.0),
            (300.0, -200.0),
            (300.0, 200.0),
            (-300.0, 200.0),
            (-300.0, -200.0),
            (0.0, -200.0),
        ]),
        p(&[
            (-600.0, -202.0),
            (-300.0, -202.0),
            (300.0, -201.5),
            (600.0, -201.5),
        ]),
    ];
    let (t, walks) =
        crate::topology::build_with_traversals(&inputs, &crate::topology::Params::default())
            .unwrap();
    let order = crate::ordering::solve(
        &t,
        &walks,
        &crate::ordering::Weights::default(),
        crate::ordering::ExhaustiveLimit::default(),
    )
    .order;
    let out = lines(&t, &walks, &order, 12.0, &inputs);
    assert_eq!(out[0].first(), out[0].last(), "closed");
    assert!(
        (out[0][0].y - -202.0).abs() > 5.0,
        "in its slot, off the centerline"
    );
    assert_ne!(
        out[1].first(),
        out[1].last(),
        "the straight line stays open"
    );
}

#[test]
fn a_loop_line_in_two_slots_stays_open() {
    // Ends a whole slot apart are not one place: no closing jump across lanes.
    let input = vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 0.0),
    ];
    let mut line = vec![
        Point::new(0.0, 6.0),
        Point::new(10.0, 6.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, -6.0),
    ];
    let before = line.clone();
    close_like(&input, &mut line, 12.0);
    assert_eq!(line, before);
    close_like(&input[..3], &mut line, 12.0); // not closed: unchanged
    assert_eq!(line, before);
    close_like(&input, &mut line[..2], 12.0); // too short to close: unchanged
    assert_eq!(line, before);
}

#[test]
fn jittered_inputs_still_share_one_centerline() {
    // The lines' own inputs are meters apart, but they are offset from the edge's centerline.
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), (100.0, 0.0)], &[0, 1])].into(),
    };
    let walks = walks_of(vec![vec![(0, F)], vec![(0, F)]]);
    let inputs = vec![
        vec![Point::new(0.0, 3.0), Point::new(100.0, 3.0)],
        vec![Point::new(0.0, -2.0), Point::new(100.0, -2.0)],
    ];
    let out = lines(&t, &walks, &order_of(vec![vec![0, 1]]), 4.0, &inputs);
    assert_eq!(out[0], vec![Point::new(0.0, -2.0), Point::new(100.0, -2.0)]);
    assert_eq!(out[1], vec![Point::new(0.0, 2.0), Point::new(100.0, 2.0)]);
}

#[test]
fn a_walk_runs_its_pieces_in_order_reversing_edges_it_runs_against() {
    // A: 0 → 1 along edge 0, then 1 → 2 against edge 1 (drawn 2 → 1).
    let t = Topology {
        nodes: vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0),
        ]
        .into(),
        edges: vec![
            edge(0, 1, &[(0.0, 0.0), (100.0, 0.0)], &[0]),
            edge(2, 1, &[(100.0, 100.0), (100.0, 0.0)], &[0]),
        ]
        .into(),
    };
    let walks = walks_of(vec![vec![(0, F), (1, Dir::Backward)]]);
    // The pieces, joined straight: through the corner.
    let pieces = edge_pieces(&t, &order_of(vec![vec![0], vec![0]]), 4.0);
    assert_eq!(
        assemble(&walks[LineId::new(0)], &pieces, LineId::new(0)),
        Some(vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0)
        ]),
        "one line, no repeated point at the joint"
    );
    // With fronts and connectors: cut back 2 m at the corner (half the bundle: 1 line × 4 m / 2; a right angle), and a
    // smooth connector from (98, 0) heading east to (100, 2) heading north.
    let out = lines(
        &t,
        &walks,
        &order_of(vec![vec![0], vec![0]]),
        4.0,
        &[vec![]],
    );
    let line = &out[0];
    assert_eq!(
        line.first(),
        Some(&Point::new(0.0, 0.0)),
        "the walk's start is not cut"
    );
    assert_eq!(line.last(), Some(&Point::new(100.0, 100.0)), "nor its end");
    assert!(line.contains(&Point::new(98.0, 0.0)) && line.contains(&Point::new(100.0, 2.0)));
    for p in line.iter().filter(|p| p.x > 98.0 && p.y < 2.0) {
        assert!(
            p.x < 100.0 && p.y > 0.0,
            "the connector stays inside the corner: {p:?}"
        );
    }
    assert!(
        line.windows(2)
            .all(|w| w[1].x >= w[0].x - 1e-9 && w[1].y >= w[0].y - 1e-9),
        "east, then north"
    );
}

#[test]
fn node_fronts_make_room_by_angle_within_their_limits() {
    let p = |x, y| Point::new(x, y);
    // A right angle with one line: W/2 = 2, tan 45° = 1 → 2. A route's end: 0.
    let corner = Topology {
        nodes: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)].into(),
        edges: vec![
            edge(0, 1, &[(0.0, 0.0), (100.0, 0.0)], &[0]),
            edge(1, 2, &[(100.0, 0.0), (100.0, 100.0)], &[0]),
        ]
        .into(),
    };
    let f = node_fronts(&corner, &order_of(vec![vec![0], vec![0]]), 4.0);
    assert_eq!(f[NodeId::new(0)], 0.0);
    assert!((f[NodeId::new(1)] - 2.0).abs() < 1e-12, "{f:?}");
    assert_eq!(f[NodeId::new(2)], 0.0);
    // Straight on, three lines: at least half the bundle (3 × 4 / 2 = 6).
    let straight = Topology {
        nodes: vec![p(0.0, 0.0), p(100.0, 0.0), p(200.0, 0.0)].into(),
        edges: vec![
            edge(0, 1, &[(0.0, 0.0), (100.0, 0.0)], &[0, 1, 2]),
            edge(1, 2, &[(100.0, 0.0), (200.0, 0.0)], &[0, 1, 2]),
        ]
        .into(),
    };
    let order = order_of(vec![vec![0, 1, 2], vec![0, 1, 2]]);
    assert!((node_fronts(&straight, &order, 4.0)[NodeId::new(1)] - 6.0).abs() < 1e-12);
    // A sharp turn on short edges: capped at 45% of the shortest edge (10 m → 4.5).
    let sharp = Topology {
        nodes: vec![p(0.0, 0.0), p(10.0, 0.0), p(0.0, 1.0)].into(),
        edges: vec![
            edge(0, 1, &[(0.0, 0.0), (10.0, 0.0)], &[0, 1, 2]),
            edge(1, 2, &[(10.0, 0.0), (0.0, 1.0)], &[0, 1, 2]),
        ]
        .into(),
    };
    assert!((node_fronts(&sharp, &order, 4.0)[NodeId::new(1)] - 4.5).abs() < 1e-12);
}

#[test]
fn connectors_leave_and_arrive_along_their_tangents() {
    let (p0, p3) = (Point::new(0.0, 0.0), Point::new(10.0, 10.0));
    let c = connector(
        p0,
        Point::new(1.0, 0.0),
        p3,
        Point::new(0.0, 1.0),
        f64::INFINITY,
    );
    assert_eq!(c.last(), Some(&p3));
    // A biarc: east then north, 10 m each way: a quarter circle around (0, 10).
    assert!(
        c.iter()
            .all(|q| (q.distance(Point::new(0.0, 10.0)) - 10.0).abs() < 1e-9),
        "{c:?}"
    );
    // Wider than three fronts (5 m each): the capped cubic.
    let cubic = connector(p0, Point::new(1.0, 0.0), p3, Point::new(0.0, 1.0), 4.0);
    assert_eq!(cubic.len(), CONNECTOR_STEPS);
    assert!(c[0].y.abs() < c[0].x, "leaves mostly east: {:?}", c[0]);
    let before_end = c[c.len() - 2];
    assert!(
        (10.0 - before_end.x) < (10.0 - before_end.y),
        "arrives mostly northward: {before_end:?}"
    );
    assert!(
        connector(p0, Point::new(1.0, 0.0), p0, Point::new(0.0, 1.0), 5.0).is_empty(),
        "no gap, nothing"
    );
    // Handles capped (a jump across a small node): nearly straight, inside the gap's band.
    let far = Point::new(300.0, 0.0);
    let capped = connector(p0, Point::new(0.0, 1.0), far, Point::new(0.0, 1.0), 2.0);
    assert!(capped.iter().all(|p| p.y.abs() <= 2.0), "{capped:?}");
}

#[test]
fn sub_path_cuts_by_distance_along() {
    let path = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
    ];
    assert_eq!(
        sub_path(&path, 5.0, 15.0),
        vec![
            Point::new(5.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 5.0)
        ]
    );
    assert_eq!(sub_path(&path, 0.0, 20.0), path.to_vec());
    assert_eq!(
        sub_path(&path, 2.0, 4.0),
        vec![Point::new(2.0, 0.0), Point::new(4.0, 0.0)]
    );
}

#[test]
fn an_out_and_back_folds_onto_itself_in_one_slot() {
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)].into(),
        edges: vec![TopoEdge {
            from: NodeId::new(0),
            to: NodeId::new(1),
            path: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)],
            lines: vec![(LineId::new(0), Dir::Both), (LineId::new(1), F)],
        }]
        .into(),
    };
    let walks = walks_of(vec![vec![(0, F), (0, Dir::Backward)], vec![(0, F)]]);
    let out = lines(
        &t,
        &walks,
        &order_of(vec![vec![0, 1]]),
        4.0,
        &[vec![], vec![]],
    );
    // Out along y = -2 and back along y = -2: the same slot both ways.
    assert!(
        out[0].iter().all(|p| (p.y + 2.0).abs() < 1e-9),
        "{:?}",
        out[0]
    );
    assert_eq!(out[0].first(), out[0].last());
    assert!(out[1].iter().all(|p| (p.y - 2.0).abs() < 1e-9));
}

#[test]
fn a_route_on_no_edge_keeps_its_input_and_bad_walks_are_skipped() {
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(100.0, 0.0)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), (100.0, 0.0)], &[0])].into(),
    };
    let short = vec![Point::new(5.0, 5.0), Point::new(6.0, 5.0)];
    // Route 1 walks nothing; route 2 walks an edge that doesn't exist and one it isn't on.
    let walks = walks_of(vec![vec![(0, F)], vec![], vec![(9, F), (0, F)]]);
    let out = lines(
        &t,
        &walks,
        &order_of(vec![vec![0]]),
        4.0,
        &[vec![], short.clone(), short.clone()],
    );
    assert_eq!(out[1], short);
    assert_eq!(out[2], short, "no piece of its own: its input");
    assert_eq!(out.len(), 3);
}

proptest! {
    /// Lines through a junction (a Y with arms at random angles) are continuous: no jump
    /// longer than the arm, every point finite, starting and ending where the walk does.
    #[test]
    fn lines_through_a_junction_are_continuous_and_reach_their_ends(
        a in 0.3..2.8_f64,
        b in 3.4..6.0_f64,
        pitch in 1.0..10.0_f64,
    ) {
        let arm = |angle: f64| Point::new(100.0 + 200.0 * angle.cos(), 200.0 * angle.sin());
        let nodes = vec![Point::new(-200.0, 0.0), Point::new(100.0, 0.0), arm(a), arm(b)];
        let t = Topology {
            nodes: nodes.clone().into(),
            edges: vec![
                TopoEdge { from: NodeId::new(0), to: NodeId::new(1), path: vec![nodes[0], nodes[1]], lines: vec![(LineId::new(0), F), (LineId::new(1), F)] },
                TopoEdge { from: NodeId::new(1), to: NodeId::new(2), path: vec![nodes[1], nodes[2]], lines: vec![(LineId::new(0), F)] },
                TopoEdge { from: NodeId::new(1), to: NodeId::new(3), path: vec![nodes[1], nodes[3]], lines: vec![(LineId::new(1), F)] },
            ].into(),
        };
        let walks = walks_of(vec![vec![(0, F), (1, F)], vec![(0, F), (2, F)]]);
        let out = lines(&t, &walks, &order_of(vec![vec![1, 0], vec![0], vec![1]]), pitch, &[vec![], vec![]]);
        for (i, line) in out.iter().enumerate() {
            prop_assert!(line.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
            prop_assert!(line.windows(2).all(|w| w[0].distance(w[1]) <= 300.0 + 1e-6), "a jump in {}", i);
            prop_assert!(line[0].distance(nodes[0]) <= pitch, "line {} starts at the trunk's start", i);
            let end = nodes[2 + i];
            prop_assert!(line[line.len() - 1].distance(end) < 1e-6, "line {} ends at its arm's end", i);
        }
    }
    /// Every line on an edge lies exactly its slot's distance from the centerline (a straight
    /// edge in any direction), so neighbors are one pitch apart.
    #[test]
    fn straight_edges_put_every_slot_at_its_distance(
        angle in 0.0..std::f64::consts::TAU,
        n in 1_usize..8,
        pitch in 0.5..20.0_f64,
    ) {
        let end = Point::new(200.0 * angle.cos(), 200.0 * angle.sin());
        let t = Topology {
            nodes: vec![Point::new(0.0, 0.0), end].into(),
            edges: vec![TopoEdge { from: NodeId::new(0), to: NodeId::new(1), path: vec![Point::new(0.0, 0.0), end], lines: (0..n).map(|l| (LineId::new(l), F)).collect() }].into(),
        };
        let order = order_of(vec![(0..n).collect::<Vec<_>>()]);
        let pieces = edge_pieces(&t, &order, pitch);
        let left = Point::new(-angle.sin(), angle.cos());
        for (slot, (_, pts)) in pieces[EdgeId::new(0)].iter().enumerate() {
            for p in pts {
                let d = p.x * left.x + p.y * left.y;
                prop_assert!((d - slot_offset(slot, n) * pitch).abs() < 1e-6);
            }
        }
    }
}
