//! Cutting a connector's loop and offsetting's folds, and what stays.

use proptest::prelude::*;

use super::loops::{
    LOOP_SLACK_M, NODE_LOOP_REACH_FRONTS, crosses_itself, cut_node_loop, without_offset_loops,
};
use super::test_support::*;
use super::walk::connector;
use super::*;
use crate::polyline::offset_polyline_within;
use crate::topology::distance_to_segment;

/// Tel Aviv's line 61-0 at zoom 17: it arrives heading north-east, but its
/// next slot starts 4 m behind and to its left, heading east-south-east, so the connector
/// loops round and the line crosses its own way in. The node's loop is cut to the crossing.
#[test]
fn a_connector_that_turns_a_line_back_over_itself_is_cut_at_the_crossing() {
    let p = |x: f64, y: f64| Point::new(x, y);
    let p0 = p(-1_012.831_475_740_120_3, 90.864_290_080_990_61);
    let p3 = p(-1_016.543_375_894_734_7, 92.925_205_648_788_07);
    let (t0, t1) = (
        p(0.578_269_690_721_905_6, 0.815_845_674_617_688_6),
        p(0.864_489_904_062_067, -0.502_650_182_308_489_9),
    );
    let front = 3.041_446_343_319_463_6;
    let mut line = vec![p(-1_079.132_6, -2.249_9), p(-1_076.978_7, 0.362_9), p0];
    let from = line.len() - 1;
    line.extend(connector(p0, t0, p3, t1, front));
    let to = line.len();
    line.extend([p(-951.513_8, 55.114_3), p(-951.249_2, 54.953_7)]);
    assert!(
        crosses_itself(&line),
        "the case: the uncut line crosses itself"
    );
    let (first, last) = (line[0], *line.last().unwrap());
    let reach = NODE_LOOP_REACH_FRONTS * front + LOOP_SLACK_M;
    let mut cut = line.clone();
    cut_node_loop(&mut cut, from, to, reach);
    assert!(!crosses_itself(&cut), "{cut:?}");
    assert!(cut.len() < line.len());
    assert_eq!((cut[0], *cut.last().unwrap()), (first, last));
    let again = cut.clone();
    let n = cut.len();
    cut_node_loop(&mut cut, from.min(n - 1), (from + 2).min(n), reach);
    assert_eq!(cut, again, "cutting again changes nothing");
}

/// A loop farther from the node than the reach is the route's own (it goes round a block and
/// comes back): it stays. Bad ranges and reaches change nothing.
#[test]
fn a_route_loop_beyond_the_reach_and_bad_input_stay() {
    let p = |x: f64, y: f64| Point::new(x, y);
    // A connector at (0, 0)–(1, 0); the line goes round a 100 m block and crosses its way in
    // 60 m away from the connector.
    let line = vec![
        p(-200.0, 0.0),
        p(0.0, 0.0),
        p(1.0, 0.0),
        p(100.0, 0.0),
        p(100.0, 100.0),
        p(60.0, 100.0),
        p(60.0, -50.0),
    ];
    assert!(crosses_itself(&line));
    let mut kept = line.clone();
    cut_node_loop(&mut kept, 1, 3, 5.0);
    assert_eq!(kept, line);
    for (from, to, reach) in [
        (1, 3, 0.0),
        (1, 3, f64::NAN),
        (1, 3, -1.0),
        (3, 3, 5.0),
        (4, 2, 5.0),
        (7, 9, 5.0),
        (1, 99, 5.0),
    ] {
        let mut same = line.clone();
        cut_node_loop(&mut same, from, to, reach);
        assert_eq!(same, line, "{from} {to} {reach}");
    }
    let mut empty: Vec<Point> = Vec::new();
    cut_node_loop(&mut empty, 0, 1, 5.0);
    assert!(empty.is_empty());
}

/// Two segments that don't share a point cross: the self-crossing check, on one line.
fn has_self_crossing(line: &[Point]) -> bool {
    crosses_itself(line)
}

#[test]
fn a_small_fold_is_cut_at_its_crossing() {
    // Segment 0 runs east; segment 2 comes back across it at (7.5, 0): a 5 m fold.
    let line = pts(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (5.0, -5.0),
        (20.0, -5.0),
    ]);
    let center = pts(&[(0.0, 0.0), (20.0, 0.0)]);
    assert_eq!(
        without_offset_loops(line, &center, 3.0),
        pts(&[(0.0, 0.0), (7.5, 0.0), (5.0, -5.0), (20.0, -5.0)])
    );
}

#[test]
fn longer_loops_and_the_routes_own_loops_stay() {
    let line = pts(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (5.0, -5.0),
        (20.0, -5.0),
    ]);
    let straight = pts(&[(0.0, 0.0), (20.0, 0.0)]);
    // At 0.5 m a full turn is 2π · 0.5 + 1 = 4.1 m, shorter than the 5 m fold.
    assert_eq!(without_offset_loops(line.clone(), &straight, 0.5), line);
    // A centerline that crosses itself: every loop may be the route's own.
    let looping = pts(&[(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (5.0, -5.0)]);
    assert_eq!(without_offset_loops(line.clone(), &looping, 3.0), line);
}

#[test]
fn no_offset_bad_offsets_and_short_lines_are_left_alone() {
    let line = pts(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (5.0, -5.0),
        (20.0, -5.0),
    ]);
    let center = pts(&[(0.0, 0.0), (20.0, 0.0)]);
    for offset in [0.0, -0.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            without_offset_loops(line.clone(), &center, offset),
            line,
            "{offset}"
        );
    }
    for short in [vec![], pts(&[(1.0, 1.0)]), pts(&[(0.0, 0.0), (5.0, 0.0)])] {
        assert_eq!(without_offset_loops(short.clone(), &center, 3.0), short);
        assert_eq!(without_offset_loops(short.clone(), &[], 3.0), short);
    }
    // Segments that touch, share an end or overlap don't cross.
    let touching = pts(&[
        (0.0, 0.0),
        (10.0, 0.0),
        (10.0, 5.0),
        (5.0, 0.0),
        (5.0, -5.0),
    ]);
    assert_eq!(
        without_offset_loops(touching.clone(), &center, 3.0),
        touching
    );
    let overlapping = pts(&[(0.0, 0.0), (10.0, 0.0), (4.0, 0.0), (12.0, 0.0)]);
    assert_eq!(
        without_offset_loops(overlapping.clone(), &center, 3.0),
        overlapping
    );
}

/// A bundle round a corner cut by a short 2.8 m diagonal (a chamfer, as
/// GTFS shapes often draw a turn). Its inner lines, offset more than the diagonal is long, used
/// to fold into loops there; now no line crosses itself, and each still runs end to end.
#[test]
fn lines_round_a_chamfered_corner_do_not_cross_themselves() {
    let path = [(0.0, 0.0), (100.0, 0.0), (102.0, 2.0), (102.0, 100.0)];
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(102.0, 100.0)].into(),
        edges: vec![edge(0, 1, &path, &[0, 1, 2, 3, 4, 5])].into(),
    };
    let walks = walks_of((0..6).map(|_| vec![(0, F)]).collect());
    let order = order_of(vec![(0..6).collect::<Vec<_>>()]);
    let inputs = vec![Vec::new(); 6];
    let out = lines(&t, &walks, &order, 3.0, &inputs);
    let center = pts(&path);
    for (slot, line) in out.iter().enumerate() {
        let offset = slot_offset(slot, 6) * 3.0;
        let raw =
            offset_polyline_within(&center, offset, DEFAULT_MITER_LIMIT, MAX_CORNER_OVERSHOOT_M)
                .unwrap();
        assert!(
            !has_self_crossing(line),
            "slot {slot} ({offset} m) crosses itself: {line:?}"
        );
        // The same ends (the rounded corner turns the last leg by a hair: 1e-14 m).
        assert!(
            line[0].distance(raw[0]) < 1e-9
                && line[line.len() - 1].distance(raw[raw.len() - 1]) < 1e-9,
            "slot {slot}"
        );
        if offset > 3.0 {
            assert!(
                has_self_crossing(&raw),
                "slot {slot} folds without the loop cut (the test's point)"
            );
        }
    }
}

proptest! {
    /// A node's loop cut keeps the line's first and last points and never adds a point,
    /// whatever the line, connector range and reach.
    #[test]
    fn node_loop_cuts_keep_the_ends_and_are_stable(
        pts in prop::collection::vec((-30.0..30.0_f64, -30.0..30.0_f64), 2..14),
        from in 0_usize..14,
        len in 1_usize..6,
        reach in 0.5..20.0_f64,
    ) {
        let line: Vec<Point> = pts.iter().map(|&(x, y)| Point::new(x, y)).collect();
        let from = from % line.len();
        let to = (from + len).min(line.len());
        prop_assume!(from < to);
        let mut cut = line.clone();
        cut_node_loop(&mut cut, from, to, reach);
        prop_assert!(cut.len() <= line.len());
        prop_assert_eq!(cut[0], line[0]);
        prop_assert_eq!(cut.last(), line.last());
    }
    /// Cutting loops keeps both ends, only drops points or adds crossing points on the line,
    /// leaves no crossing it would cut, and doing it again changes nothing.
    #[test]
    fn cutting_loops_is_safe_and_done_once(
        xy in proptest::collection::vec((-20.0..20.0_f64, -20.0..20.0_f64), 0..12),
        offset in -8.0..8.0_f64,
    ) {
        let line = pts(&xy);
        let center = pts(&[(-50.0, 0.0), (50.0, 0.0)]);
        let out = without_offset_loops(line.clone(), &center, offset);
        prop_assert!(out.len() <= line.len());
        if let (Some(a), Some(b)) = (line.first(), out.first()) {
            prop_assert_eq!(a, b);
            prop_assert_eq!(line.last(), out.last());
        }
        for p in &out {
            let on_line = line.windows(2).any(|w| distance_to_segment(*p, w[0], w[1]) < 1e-9)
                || line.contains(p);
            prop_assert!(on_line, "{:?} is not on the input", p);
        }
        prop_assert_eq!(without_offset_loops(out.clone(), &center, offset), out);
    }
}
