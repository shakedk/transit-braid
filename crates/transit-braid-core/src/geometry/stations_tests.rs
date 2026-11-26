//! Station bars across the bundle, and same-name stations merged.

use proptest::prelude::*;

use super::stations::point_and_direction;
use super::test_support::*;
use super::*;
use crate::ids::EdgeId;

#[test]
fn a_bar_crosses_the_whole_bundle_square_to_the_street() {
    let t = street(&[0, 1, 2]);
    let order = order_of(vec![vec![0, 1, 2]]);
    // Three lines 5 m apart, 4 m wide: from 1 pitch + half a width right of the centerline to
    // the same left of it, at the station's place along the street.
    let bars = station_bars(&t, &order, 5.0, 4.0, &[Point::new(100.0, 3.0)]);
    assert_eq!(
        bars,
        vec![Some((Point::new(100.0, -7.0), Point::new(100.0, 7.0)))]
    );
    // One line: just its own width.
    let one = station_bars(
        &street(&[0]),
        &order_of(vec![vec![0]]),
        5.0,
        4.0,
        &[Point::new(50.0, -1.0)],
    );
    assert_eq!(
        one,
        vec![Some((Point::new(50.0, -2.0), Point::new(50.0, 2.0)))]
    );
}

/// Found as 25 tilted bars in Dublin at zoom 17: a stop on the outside of a bend snaps onto
/// the bend's corner point; its bar is square to the direction halfway between the two
/// segments, where the rounded bundle runs. Before, it was square to one segment only, off by
/// the whole bend from the other.
#[test]
fn a_bar_on_a_bend_is_square_to_the_bend() {
    // East 100 m, then 20° to the north for 100 m; two lines 5 m apart.
    let (c, s) = (20f64.to_radians().cos(), 20f64.to_radians().sin());
    let corner = (100.0, 0.0);
    let end = (100.0 + 100.0 * c, 100.0 * s);
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(end.0, end.1)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), corner, end], &[0, 1])].into(),
    };
    let order = order_of(vec![vec![0, 1]]);
    // On the outside of the bend (south-east of the corner): its nearest point is the corner.
    let stop = Point::new(102.0, -6.0);
    let (a, b) = station_bars(&t, &order, 5.0, 4.0, &[stop])[0].unwrap();
    let mid = Point::new(f64::midpoint(a.x, b.x), f64::midpoint(a.y, b.y));
    assert!(
        mid.distance(Point::new(corner.0, corner.1)) < 1e-6,
        "{mid:?}"
    );
    // Halfway between east and 20° north: 10° north of east.
    let half = (10f64.to_radians().cos(), 10f64.to_radians().sin());
    let bar = ((b.x - a.x) / a.distance(b), (b.y - a.y) / a.distance(b));
    assert!((bar.0 * half.0 + bar.1 * half.1).abs() < 1e-9, "{bar:?}");
    // Same length as on a straight street: one pitch plus a line width.
    assert!((a.distance(b) - 9.0).abs() < 1e-9);
    // A hair either side of the corner point (rounding), the same direction.
    for along in [100.0 - 1e-9, 100.0 + 1e-9] {
        let (_, d) = point_and_direction(&t.edges[EdgeId::new(0)].path, along).unwrap();
        assert!(
            (d.x * half.0 + d.y * half.1 - 1.0).abs() < 1e-9,
            "{along}: {d:?}"
        );
    }
    // Beside a segment, away from the corner, it is square to that segment as before.
    let (a, b) = station_bars(&t, &order, 5.0, 4.0, &[Point::new(50.0, 3.0)])[0].unwrap();
    assert!(
        (a.x - 50.0).abs() < 1e-9 && (b.x - 50.0).abs() < 1e-9,
        "{a:?} {b:?}"
    );
}

/// A stop past the end of a street (a terminus) gets its bar a line width inside the
/// edge, not on the node itself, where a line that ends or turns there would only graze it.
#[test]
fn a_bar_stays_a_line_width_inside_its_edge() {
    let t = street(&[0, 1]);
    let order = order_of(vec![vec![0, 1]]);
    // 4 m wide lines: past the west end, and past the east end (the street is 300 m).
    let bars = station_bars(
        &t,
        &order,
        5.0,
        4.0,
        &[Point::new(-3.0, 1.0), Point::new(304.0, -1.0)],
    );
    let x = |i: usize| bars[i].map(|(a, _)| a.x);
    assert_eq!(x(0), Some(4.0));
    assert_eq!(x(1), Some(296.0));
    // A street shorter than two line widths: its middle.
    let short = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(6.0, 0.0)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), (6.0, 0.0)], &[0])].into(),
    };
    let bar = station_bars(
        &short,
        &order_of(vec![vec![0]]),
        5.0,
        4.0,
        &[Point::new(7.0, 0.0)],
    );
    assert_eq!(bar[0].map(|(a, _)| a.x), Some(3.0));
}

/// A path that turns straight back: no halfway direction, the bar keeps the first segment's.
#[test]
fn a_bar_at_a_full_u_turn_keeps_its_segment() {
    let t = Topology {
        nodes: vec![Point::new(0.0, 0.0), Point::new(0.0, 1.0)].into(),
        edges: vec![edge(0, 1, &[(0.0, 0.0), (100.0, 0.0), (0.0, 1e-12)], &[0])].into(),
    };
    let (_, d) = point_and_direction(&t.edges[EdgeId::new(0)].path, 100.0).unwrap();
    assert!((d.x - 1.0).abs() < 1e-9 && d.y.abs() < 1e-9, "{d:?}");
}

/// A bar per side of a split street: a boulevard with two carriageways 24 m apart, eastbound lines 0
/// and 1 on one, westbound 2 and 3 on the other. A same-name stop on each side (as in Tel
/// Aviv) keeps its own bar across its own bundle; a pair on both sides of one bundle shares
/// one bar.
#[test]
fn a_same_name_pair_on_two_carriageways_keeps_a_bar_each() {
    let t = Topology {
        nodes: vec![
            Point::new(0.0, 0.0),
            Point::new(300.0, 0.0),
            Point::new(0.0, 24.0),
            Point::new(300.0, 24.0),
        ]
        .into(),
        edges: vec![
            edge(0, 1, &[(0.0, 0.0), (300.0, 0.0)], &[0, 1]),
            edge(3, 2, &[(300.0, 24.0), (0.0, 24.0)], &[2, 3]),
        ]
        .into(),
    };
    let order = order_of(vec![vec![0, 1], vec![2, 3]]);
    // Stop A at the curb south of the eastbound bundle, stop B north of the westbound one,
    // 30 m apart along the street: 39 m apart in all, within the 50 m merge distance.
    let stops = pts(&[(100.0, -4.0), (130.0, 28.0)]);
    let placed = merge_named_stations_on_edges(&t, &order, &stops, &[7, 7]);
    assert_eq!(placed, stops, "two carriageways: nothing merged");
    let bars = station_bars(&t, &order, 5.0, 4.0, &placed);
    assert_eq!(
        bars,
        vec![
            Some((Point::new(100.0, -4.5), Point::new(100.0, 4.5))),
            Some((Point::new(130.0, 28.5), Point::new(130.0, 19.5))),
        ]
    );
    // The old merge put both at their average, (115, 12): one bar on one bundle only.
    assert_eq!(
        merge_named_stations(&stops, &[7, 7]),
        pts(&[(115.0, 12.0), (115.0, 12.0)])
    );
    // Both sides of the eastbound bundle: one street piece, so they share one bar.
    let across = pts(&[(100.0, -4.0), (110.0, 4.0)]);
    assert_eq!(
        merge_named_stations_on_edges(&t, &order, &across, &[7, 7]),
        pts(&[(105.0, 0.0), (105.0, 0.0)])
    );
}

/// Bad and empty input for the edge-aware merge: stations off every edge, not finite, without
/// names, or no edges at all merge nothing and keep their places.
#[test]
fn the_edge_aware_merge_leaves_bad_and_unnamed_stations_alone() {
    let t = street(&[0, 1]);
    let order = order_of(vec![vec![0, 1]]);
    let far = pts(&[(100.0, 60.0), (110.0, 60.0)]); // 60 m from the street
    assert_eq!(
        merge_named_stations_on_edges(&t, &order, &far, &[1, 1]),
        far
    );
    let odd = pts(&[(f64::NAN, 0.0), (100.0, 2.0), (104.0, -2.0)]);
    let out = merge_named_stations_on_edges(&t, &order, &odd, &[4, 4, 4]);
    assert!(out[0].x.is_nan());
    assert_eq!(
        (out[1], out[2]),
        (Point::new(102.0, 0.0), Point::new(102.0, 0.0))
    );
    let named_none = pts(&[(100.0, 2.0), (104.0, -2.0)]);
    assert_eq!(
        merge_named_stations_on_edges(&t, &order, &named_none, &[]),
        named_none
    );
    assert_eq!(
        merge_named_stations_on_edges(&t, &order, &named_none, &[0, 0]),
        named_none
    );
    let empty = Topology {
        nodes: vec![].into(),
        edges: vec![].into(),
    };
    assert_eq!(
        merge_named_stations_on_edges(&empty, &Order::new(), &named_none, &[1, 1]),
        named_none
    );
    assert!(merge_named_stations_on_edges(&t, &order, &[], &[1, 1]).is_empty());
    // An edge with no lines in the order is no street to snap to.
    assert_eq!(
        merge_named_stations_on_edges(&t, &order_of(vec![vec![]]), &named_none, &[1, 1]),
        named_none
    );
}

#[test]
fn stations_far_from_every_route_or_not_finite_get_no_bar() {
    let t = street(&[0, 1]);
    let order = order_of(vec![vec![0, 1]]);
    let stations = [
        Point::new(100.0, STATION_SNAP_M + 0.1),
        Point::new(100.0, f64::NAN),
        Point::new(f64::INFINITY, 0.0),
        Point::new(100.0, STATION_SNAP_M),
    ];
    let bars = station_bars(&t, &order, 5.0, 4.0, &stations);
    assert_eq!(bars[..3], [None, None, None]);
    assert!(bars[3].is_some(), "exactly at the limit still counts");
    assert!(
        station_bars(&t, &order_of(vec![vec![]]), 5.0, 4.0, &stations)
            .iter()
            .all(Option::is_none),
        "an edge without lines has no bundle"
    );
    assert!(
        station_bars(&Topology::default(), &Order::new(), 5.0, 4.0, &stations)
            .iter()
            .all(Option::is_none)
    );
    assert!(station_bars(&t, &order, 5.0, 4.0, &[]).is_empty());
}

#[test]
fn a_stop_gets_a_bar_within_25_m_of_a_route_and_none_beyond() {
    // The documented distance, in plain numbers (a test written against the constant passes
    // whatever its value): curbside stops 24 m off the street's centerline get a bar, 26 m don't.
    let t = street(&[0, 1]);
    let order = order_of(vec![vec![0, 1]]);
    let stations = [
        Point::new(100.0, 24.0),
        Point::new(100.0, -24.0),
        Point::new(100.0, 26.0),
        Point::new(100.0, -26.0),
    ];
    let bars = station_bars(&t, &order, 5.0, 4.0, &stations);
    assert!(
        bars[0].is_some() && bars[1].is_some(),
        "24 m either side: a bar"
    );
    assert_eq!(bars[2..], [None, None], "26 m either side: no bar");
}

#[test]
fn a_station_in_a_node_front_moves_to_where_the_lines_are_parallel() {
    // A right-angle corner: the front cuts 2 m from the corner on each edge (one line); a
    // bar also stays a line width (4 m) inside its edge, the larger of the two.
    let corner = Topology {
        nodes: vec![
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            Point::new(100.0, 100.0),
        ]
        .into(),
        edges: vec![
            edge(0, 1, &[(0.0, 0.0), (100.0, 0.0)], &[0]),
            edge(1, 2, &[(100.0, 0.0), (100.0, 100.0)], &[0]),
        ]
        .into(),
    };
    let bars = station_bars(
        &corner,
        &order_of(vec![vec![0], vec![0]]),
        4.0,
        4.0,
        &[Point::new(99.5, 0.5)],
    );
    let (a, b) = bars[0].unwrap();
    let mid = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    assert!(
        mid.distance(Point::new(96.0, 0.0)) < 1e-9 || mid.distance(Point::new(100.0, 4.0)) < 1e-9,
        "{mid:?}"
    );
}

#[test]
fn same_name_stations_close_together_move_to_their_average() {
    let s = pts(&[
        (0.0, 8.0),
        (20.0, -8.0),
        (500.0, 0.0),
        (10.0, 30.0),
        (12.0, 2.0),
    ]);
    // 1 and 1 within 50 m: merged; the third 1 is 490 m away; 2 is close but another name; 0
    // has no name.
    let out = merge_named_stations(&s, &[1, 1, 1, 2, 0]);
    assert_eq!(out[0], Point::new(10.0, 0.0));
    assert_eq!(out[1], Point::new(10.0, 0.0));
    assert_eq!(&out[2..], &s[2..]);
}

#[test]
fn stations_chain_within_the_merge_distance() {
    // 40 m apart in a row: the ends are 80 m apart but join through the middle.
    let s = pts(&[(0.0, 0.0), (40.0, 0.0), (80.0, 0.0)]);
    let out = merge_named_stations(&s, &[3, 3, 3]);
    assert!(out.iter().all(|&p| p == Point::new(40.0, 0.0)), "{out:?}");
}

#[test]
fn missing_groups_and_odd_positions_merge_nothing() {
    let s = pts(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)]);
    assert_eq!(merge_named_stations(&s, &[]), s);
    assert_eq!(merge_named_stations(&s, &[0, 0, 0]), s);
    // Groups shorter than the stations: the rest have no name. Longer: the extra are ignored.
    assert_eq!(merge_named_stations(&s, &[5]), s);
    assert_eq!(merge_named_stations(&s, &[0, 0, 0, 9, 9]), s);
    assert!(merge_named_stations(&[], &[1, 1]).is_empty());
    // A NaN station joins nothing and leaves the others alone.
    let odd = pts(&[(f64::NAN, 0.0), (0.0, 0.0), (10.0, 0.0)]);
    let out = merge_named_stations(&odd, &[4, 4, 4]);
    assert!(out[0].x.is_nan());
    assert_eq!(
        (out[1], out[2]),
        (Point::new(5.0, 0.0), Point::new(5.0, 0.0))
    );
}

proptest! {
    /// Merging keeps one place per station; a station moves only when another of its name is
    /// within the merge distance, and then to a finite place no farther than the cluster spans.
    #[test]
    fn merged_stations_stay_with_their_kin(
        xy in proptest::collection::vec((-200.0..200.0_f64, -200.0..200.0_f64), 0..30),
        groups in proptest::collection::vec(0_u32..4, 0..30),
    ) {
        let s = pts(&xy);
        let out = merge_named_stations(&s, &groups);
        prop_assert_eq!(out.len(), s.len());
        let g = |i: usize| groups.get(i).copied().unwrap_or(0);
        for (i, (&before, &after)) in s.iter().zip(&out).enumerate() {
            if before == after {
                continue;
            }
            prop_assert!(g(i) != 0, "a station without a name moved");
            let kin = s.iter().enumerate().any(|(j, &q)| {
                j != i && g(j) == g(i) && q.distance(before) <= STATION_MERGE_M
            });
            prop_assert!(kin, "station {} moved with no namesake within reach", i);
            prop_assert!(after.x.is_finite() && after.y.is_finite());
            let span = STATION_MERGE_M * s.len() as f64;
            prop_assert!(after.distance(before) <= span);
        }
    }
}
