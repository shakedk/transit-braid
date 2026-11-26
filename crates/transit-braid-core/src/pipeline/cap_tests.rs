//! The lane cap: which routes it drops, and that the kept ones are drawn as on their own.

use proptest::prelude::*;

use super::test_support::*;
use super::*;
use crate::ids::LineId;
use crate::topology::{self, Params};

/// A street running north from `(lng, lat)` for about `m` meters, as `[lng, lat]` points.
fn north(lng: f64, lat: f64, m: f64) -> Vec<Point> {
    let proj = Projection::at(Point::new(lng, lat));
    (0..=4)
        .map(|k| proj.inverse(Point::new(0.0, m * f64::from(k) / 4.0)))
        .collect()
}

/// The kept routes of `lines` (all but `dropped`), in order.
fn kept_of(lines: &[Vec<Point>], dropped: &[usize]) -> Vec<Vec<Point>> {
    (0..lines.len())
        .filter(|i| !dropped.contains(i))
        .map(|i| lines[i].clone())
        .collect()
}

/// The most distinct routes on one edge of `lines`' own graph.
fn widest_edge(lines: &[Vec<Point>]) -> usize {
    let proj = Projection::around(lines);
    let meters: Vec<Vec<Point>> = lines
        .iter()
        .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
        .collect();
    let (topo, _) = topology::build_with_traversals(&meters, &Params::default()).unwrap();
    topo.edges
        .iter()
        .map(|e| {
            let mut ls: Vec<LineId> = e.lines.iter().map(|&(l, _)| l).collect();
            ls.sort_unstable();
            ls.dedup();
            ls.len()
        })
        .max()
        .unwrap_or(0)
}

/// Three routes on one street with a cap of 2: the third is dropped whole, and the two
/// kept are drawn exactly as they are without it.
#[test]
fn a_full_street_drops_the_later_routes() {
    let a = east(10.013, 40.69, 400.0);
    let mut b = a.clone();
    b.reverse();
    let lines = vec![a.clone(), b, a];
    let s = at_zoom(15.0);
    let capped = side_by_side_capped(&mut Cache::new(), &[], &lines, &[], &[], &s, 2).unwrap();
    assert_eq!(capped.dropped, vec![2]);
    assert_eq!(capped.drawn.lines.len(), 2);
    let alone = side_by_side_with_stations(&lines[..2], &[], &s).unwrap();
    assert_eq!(capped.drawn, alone);
}

/// Input order decides, street by street. Cap 1: A fills street X, so B (X, then Y) is
/// dropped, and C (Y only) is kept: with B gone, Y has room.
#[test]
fn the_cap_goes_in_input_order() {
    let x = east(34.78, 32.07, 400.0);
    let corner = x[4];
    let y = north(corner.x, corner.y, 400.0);
    let mut b = x.clone();
    b.extend_from_slice(&y[1..]);
    let lines = vec![x, b, y];
    let s = at_zoom(16.0);
    let capped = side_by_side_capped(&mut Cache::new(), &[], &lines, &[], &[], &s, 1).unwrap();
    assert_eq!(capped.dropped, vec![1]);
    assert_eq!(
        capped.drawn,
        side_by_side_with_stations(&kept_of(&lines, &[1]), &[], &s).unwrap()
    );
    // Cap 2: room for everyone.
    let roomy = side_by_side_capped(&mut Cache::new(), &[], &lines, &[], &[], &s, 2).unwrap();
    assert!(roomy.dropped.is_empty());
}

/// A cap of 0 is refused, and so is any input the uncapped call refuses, even in a
/// route the cap would drop; a refused call changes nothing in the cache.
#[test]
fn a_zero_cap_and_bad_input_are_refused() {
    let a = east(0.0, 0.0, 100.0);
    let lines = vec![a.clone(), a.clone(), a];
    let s = at_zoom(15.0);
    let mut cache = Cache::new();
    side_by_side_capped(&mut cache, &[], &lines, &[], &[], &s, 1).unwrap();
    let before = cache.runs();
    let zero = side_by_side_capped(&mut cache, &[], &lines, &[], &[], &s, 0).unwrap_err();
    assert!(matches!(&zero, PipelineError::InvalidSettings(why) if why.contains("1 or more")));
    let mut bad = lines.clone();
    bad[2][1] = Point::new(f64::NAN, 0.0);
    assert_eq!(
        side_by_side_capped(&mut cache, &[], &bad, &[], &[], &s, 1).unwrap_err(),
        PipelineError::BadCoordinate { route: 2, point: 1 }
    );
    let too_many = vec![lines[0].clone(); MAX_ROUTES + 1];
    assert_eq!(
        side_by_side_capped(&mut cache, &[], &too_many, &[], &[], &s, 1).unwrap_err(),
        PipelineError::TooMany { count: 71 }
    );
    let stop = [Point::new(0.0, f64::INFINITY)];
    assert_eq!(
        side_by_side_capped(&mut cache, &[], &lines, &stop, &[], &s, 1).unwrap_err(),
        PipelineError::BadStation { station: 0 }
    );
    assert!(side_by_side_capped(&mut cache, &[], &lines, &[], &[], &at_zoom(-1.0), 1).is_err());
    assert_eq!(cache.runs(), before);
    // No routes: nothing dropped, nothing drawn, one empty bar slot per station.
    let none =
        side_by_side_capped(&mut cache, &[], &[], &[Point::new(0.0, 0.0)], &[], &s, 1).unwrap();
    assert!(none.dropped.is_empty() && none.drawn.lines.is_empty());
    assert_eq!(none.drawn.bars, vec![None]);
}

/// Seven near-parallel routes, a few meters apart, around 10° E 40° N (found by a random
/// search): with a cap of 3 the kept routes' own graph still has an edge over the cap, so the
/// cap takes a second pass, which builds the graph and the order of fewer routes as an edit.
fn two_pass_lines() -> Vec<Vec<Point>> {
    let proj = Projection::at(Point::new(10.0, 40.0));
    let meters: [&[(f64, f64)]; 7] = [
        &[(42.0, 5.3), (121.8, 7.7), (201.5, 3.8), (281.2, 3.1)],
        &[(233.1, -5.4), (146.7, -2.1), (60.3, -2.5), (-26.1, -3.5)],
        &[(219.5, -0.9), (142.5, -1.3), (65.5, 3.9), (-11.5, 0.5)],
        &[
            (-60.6, -1.1),
            (8.7, 2.3),
            (78.0, -2.7),
            (147.3, -2.4),
            (216.6, 0.6),
        ],
        &[(12.5, 7.1), (130.2, 9.2), (248.0, 7.8)],
        &[(223.6, 4.3), (85.1, 5.6), (-53.5, 8.3)],
        &[(-80.2, -0.9), (11.7, -0.7), (103.5, -1.1), (195.3, 0.9)],
    ];
    meters
        .iter()
        .map(|l| {
            l.iter()
                .map(|&(x, y)| proj.inverse(Point::new(x, y)))
                .collect()
        })
        .collect()
}

/// A zoom change with the same routes and cap runs geometry only, even when the cap took
/// two passes, and draws exactly what a fresh call draws at that zoom: the second call starts
/// from the routes the first one dropped in the end, not from its first pass (which would
/// rebuild the graph twice and order it warm from the other routes' order).
#[test]
fn a_zoom_change_after_a_two_pass_cap_runs_geometry_only() {
    let lines = two_pass_lines();
    let mut cache = Cache::new();
    let first = side_by_side_capped(&mut cache, &[], &lines, &[], &[], &at_zoom(15.0), 3).unwrap();
    let after_first = cache.runs();
    assert!(
        after_first.topology > 1,
        "the cap took one pass: {after_first:?}"
    );
    for z in [17.0, 11.0, 15.0] {
        let again = side_by_side_capped(&mut cache, &[], &lines, &[], &[], &at_zoom(z), 3).unwrap();
        let fresh =
            side_by_side_capped(&mut Cache::new(), &[], &lines, &[], &[], &at_zoom(z), 3).unwrap();
        assert_eq!(again, fresh, "zoom {z}");
        assert_eq!(again.dropped, first.dropped);
    }
    let runs = cache.runs();
    assert_eq!(runs.topology, after_first.topology);
    assert_eq!(runs.ordering, after_first.ordering);
    assert_eq!(runs.geometry, after_first.geometry + 3);
}

/// Another cap, or other routes, after a two-pass cap: the cap runs from its first pass
/// again and drops what a fresh call drops; the same cap afterwards resumes from that call.
#[test]
fn another_cap_after_a_two_pass_cap_starts_over() {
    let lines = two_pass_lines();
    let z = at_zoom(15.0);
    let mut cache = Cache::new();
    side_by_side_capped(&mut cache, &[], &lines, &[], &[], &z, 3).unwrap();
    for cap in [2, 4, 3] {
        let cached = side_by_side_capped(&mut cache, &[], &lines, &[], &[], &z, cap).unwrap();
        let fresh = side_by_side_capped(&mut Cache::new(), &[], &lines, &[], &[], &z, cap).unwrap();
        assert_eq!(cached.dropped, fresh.dropped, "cap {cap}");
    }
    let fewer = &lines[1..];
    let cached = side_by_side_capped(&mut cache, &[], fewer, &[], &[], &z, 3).unwrap();
    let fresh = side_by_side_capped(&mut Cache::new(), &[], fewer, &[], &[], &z, 3).unwrap();
    assert_eq!(cached.dropped, fresh.dropped);
    // A refused call (a station that is not a number) leaves what the next call resumes from:
    // the same routes and cap again run geometry only and draw what the cache drew.
    let off = [Point::new(f64::NAN, 0.0)];
    assert!(side_by_side_capped(&mut cache, &[], &lines, &off, &[], &z, 3).is_err());
    let before = cache.runs();
    let again = side_by_side_capped(&mut cache, &[], fewer, &[], &[], &z, 3).unwrap();
    assert_eq!(again, cached);
    assert_eq!(cache.runs().topology, before.topology);
    assert_eq!(cache.runs().ordering, before.ordering);
}

/// A cap at or above the number of routes draws exactly what no cap draws.
#[test]
fn a_cap_with_room_for_all_changes_nothing() {
    let a = east(-6.26, 53.35, 300.0);
    let lines = vec![a.clone(), a.clone(), a];
    let stop = [Point::new(-6.259, 53.350_01)];
    let s = at_zoom(17.0);
    for cap in [3, 4, MAX_ROUTES, usize::MAX] {
        let capped =
            side_by_side_capped(&mut Cache::new(), &[], &lines, &stop, &[], &s, cap).unwrap();
        assert!(capped.dropped.is_empty());
        assert_eq!(
            capped.drawn,
            side_by_side_with_stations(&lines, &stop, &s).unwrap()
        );
    }
}

/// 2–8 routes, each a stretch of one of three crossing streets (100 m between points), some
/// reversed, around a city center anywhere on the map: crowded streets for the lane cap.
fn crowded_lines() -> impl Strategy<Value = Vec<Vec<Point>>> {
    let center = (-170.0f64..170.0, -80.0f64..80.0).prop_map(|(x, y)| Point::new(x, y));
    let route = (0usize..3, 0usize..4, 1usize..5, any::<bool>());
    (center, prop::collection::vec(route, 2..9)).prop_map(|(c, rs)| {
        let proj = Projection::at(c);
        let street = |s: usize, k: usize| {
            let t = (k as f64 - 2.0) * 100.0;
            match s {
                0 => Point::new(t, 0.0),
                1 => Point::new(0.0, t),
                _ => Point::new(t, t),
            }
        };
        rs.into_iter()
            .map(|(s, from, len, back)| {
                let to = (from + len).min(4);
                let from = from.min(to - 1);
                let mut l: Vec<Point> = (from..=to).map(|k| proj.inverse(street(s, k))).collect();
                if back {
                    l.reverse();
                }
                l
            })
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// On crowded streets, with any cap: the dropped routes are a sorted set of input
    /// indices; the kept ones are drawn exactly as on their own; no edge of their own graph
    /// has more than the cap; each dropped one runs along an edge the cap filled on the graph
    /// of all; and the same ones are dropped at every zoom, through the cache or fresh.
    #[test]
    fn the_cap_holds_on_crowded_streets(lines in crowded_lines(), cap in 1usize..4) {
        let mut cache = Cache::new();
        let first = side_by_side_capped(&mut cache, &[], &lines, &[], &[], &at_zoom(15.0), cap).unwrap();
        let dropped = first.dropped.clone();
        prop_assert!(dropped.windows(2).all(|w| w[0] < w[1]));
        prop_assert!(dropped.iter().all(|&i| i < lines.len()));
        let kept = kept_of(&lines, &dropped);
        prop_assert_eq!(first.drawn.lines.len(), kept.len());
        prop_assert_eq!(&first.drawn, &side_by_side_with_stations(&kept, &[], &at_zoom(15.0)).unwrap());
        prop_assert!(widest_edge(&kept) <= cap, "an edge is over the cap of {}", cap);
        if !dropped.is_empty() {
            let proj = Projection::around(&lines);
            let meters: Vec<Vec<Point>> = lines
                .iter()
                .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
                .collect();
            let (all, _) = topology::build_with_traversals(&meters, &Params::default()).unwrap();
            for &d in &dropped {
                let full = all.edges.iter().any(|e| {
                    let mut on: Vec<usize> = e.lines.iter().map(|&(l, _)| l.index()).collect();
                    on.sort_unstable();
                    on.dedup();
                    on.contains(&d) && on.iter().filter(|l| dropped.binary_search(l).is_err()).count() >= cap
                });
                prop_assert!(full, "route {} was dropped with room on every edge", d);
            }
        }
        for z in [11.0, 17.5] {
            let again = side_by_side_capped(&mut cache, &[], &lines, &[], &[], &at_zoom(z), cap).unwrap();
            prop_assert_eq!(&again.dropped, &dropped);
            let fresh = side_by_side_capped(&mut Cache::new(), &[], &lines, &[], &[], &at_zoom(z), cap).unwrap();
            prop_assert_eq!(again, fresh);
        }
    }

    /// The cap through the cache: an edit (one route left out, the rest given ids), then the
    /// cap changed, then no cap. Each drops what a fresh call drops and builds the same graph
    /// of the kept routes; the order may differ, since an edit keeps the old order where it
    /// can.
    #[test]
    fn the_cap_through_the_cache(lines in crowded_lines(), cap in 1usize..4, gone in any::<usize>()) {
        let z = at_zoom(15.0);
        let mut cache = Cache::new();
        side_by_side_capped(&mut cache, &[], &lines, &[], &[], &z, cap).unwrap();
        let mut fewer = lines.clone();
        fewer.remove(gone % lines.len());
        let ids: Vec<u32> = (0..fewer.len()).map(|i| 100 + u32::try_from(i).unwrap()).collect();
        let same_graph = |cache: &Cache, kept: &[Vec<Point>]| {
            let proj = Projection::around(kept);
            let meters: Vec<Vec<Point>> = kept
                .iter()
                .map(|l| l.iter().map(|&q| proj.forward(q)).collect())
                .collect();
            let fresh = topology::build_with_traversals(&meters, &Params::default()).unwrap();
            cache.last.as_ref().is_some_and(|p| (p.topo.clone(), p.walks.clone()) == fresh)
        };
        for (c, ids) in [(cap, &ids[..]), (cap + 1, &ids[..]), (cap, &[][..])] {
            let cached = side_by_side_capped(&mut cache, ids, &fewer, &[], &[], &z, c).unwrap();
            let fresh = side_by_side_capped(&mut Cache::new(), &[], &fewer, &[], &[], &z, c).unwrap();
            prop_assert_eq!(&cached.dropped, &fresh.dropped);
            prop_assert_eq!(cached.drawn.lines.len(), fresh.drawn.lines.len());
            prop_assert!(cached.drawn.lines.iter().flatten().all(|q| q.x.is_finite() && q.y.is_finite()));
            prop_assert!(same_graph(&cache, &kept_of(&fewer, &cached.dropped)));
        }
        let uncapped = side_by_side_cached(&mut cache, &fewer, &[], &[], &z).unwrap();
        prop_assert_eq!(uncapped.lines.len(), fewer.len());
        prop_assert!(same_graph(&cache, &fewer));
    }
}
