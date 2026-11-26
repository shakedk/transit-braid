//! The cache: a zoom change runs geometry only, an edit rebuilds incrementally, a refused call
//! changes nothing, and the work budget.

use proptest::prelude::*;

use super::cache::Prepared;
use super::test_support::*;
use super::*;
use crate::budget::{Bounds, Work};
use crate::ids::LineId;
use crate::topology::{self, Params};

/// A zoom change reruns geometry only (topology 0, ordering 0), and gives the
/// same lines and bars as a call without the cache. New routes rebuild everything.
#[test]
fn a_zoom_change_runs_geometry_only() {
    let a = east(10.013, 40.69, 400.0);
    let lines = vec![a.clone(), a];
    let stop = [Point::new(10.015, 40.690_01)];
    let mut cache = Cache::new();
    let first = side_by_side_cached(&mut cache, &lines, &stop, &[], &at_zoom(14.0)).unwrap();
    assert_eq!(
        cache.runs(),
        StageRuns {
            topology: 1,
            ordering: 1,
            geometry: 1
        }
    );
    assert_eq!(
        first,
        side_by_side_with_stations(&lines, &stop, &at_zoom(14.0)).unwrap()
    );
    for z in [17.0, 11.5, 14.0] {
        let again = side_by_side_cached(&mut cache, &lines, &stop, &[], &at_zoom(z)).unwrap();
        assert_eq!(
            again,
            side_by_side_with_stations(&lines, &stop, &at_zoom(z)).unwrap()
        );
    }
    assert_eq!(
        cache.runs(),
        StageRuns {
            topology: 1,
            ordering: 1,
            geometry: 4
        }
    );
    // Other widths or stations: still the same graph.
    let wide = Settings {
        line_width_px: 9.0,
        ..at_zoom(16.0)
    };
    side_by_side_cached(&mut cache, &lines, &[], &[], &wide).unwrap();
    assert_eq!(cache.runs().topology, 1);
    // A moved point: everything again.
    let mut moved = lines.clone();
    moved[1][2].y += 0.000_01;
    side_by_side_cached(&mut cache, &moved, &stop, &[], &at_zoom(16.0)).unwrap();
    assert_eq!(
        cache.runs(),
        StageRuns {
            topology: 2,
            ordering: 2,
            geometry: 6
        }
    );
}

/// An edit through the cache rebuilds the topology incrementally, computing only the
/// keys near the changed route, and gives exactly what a fresh call gives.
#[test]
fn an_edit_rebuilds_only_the_topology_near_it() {
    // Six streets 0.001° (about 85 m) apart, two routes on each.
    let lines: Vec<Vec<Point>> = (0..12)
        .map(|k| east(10.01, 40.69 + 0.001 * f64::from(k / 2), 900.0))
        .collect();
    let mut cache = Cache::new();
    side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(16.0)).unwrap();
    let all = cache.topology_keys();
    assert_eq!(all.computed, all.total);
    // A point in the middle of route 0 moves 1 m: same bounding box, same projection.
    let mut moved = lines.clone();
    moved[0][1].y += 0.000_01;
    let got = side_by_side_cached(&mut cache, &moved, &[], &[], &at_zoom(16.0)).unwrap();
    assert_eq!(
        got,
        side_by_side_with_stations(&moved, &[], &at_zoom(16.0)).unwrap()
    );
    let some = cache.topology_keys();
    assert!(
        some.computed > 0 && some.computed * 3 < some.total,
        "{some:?}"
    );
    assert_eq!(cache.runs().topology, 2);
}

/// With ids, the same routes listed in another order compute no topology key, are
/// ordered warm with every order kept, and each route's line stays exactly where it was.
/// Without ids the same reorder rebuilds everything (by place, every route moved).
#[test]
fn a_reorder_matched_by_id_keeps_every_line_where_it_was() {
    let lines: Vec<Vec<Point>> = (0..12)
        .map(|k| east(10.01, 40.69 + 0.001 * f64::from(k / 2), 900.0))
        .collect();
    let ids: Vec<u32> = (0..12).map(|k| 7 * k + 1).collect();
    let s = at_zoom(16.0);
    let mut cache = Cache::new();
    let before = side_by_side_cached_with_ids(&mut cache, &ids, &lines, &[], &[], &s).unwrap();
    let order: Vec<usize> = (0..12).rev().collect();
    let rev_lines: Vec<Vec<Point>> = order.iter().map(|&i| lines[i].clone()).collect();
    let rev_ids: Vec<u32> = order.iter().map(|&i| ids[i]).collect();
    let after =
        side_by_side_cached_with_ids(&mut cache, &rev_ids, &rev_lines, &[], &[], &s).unwrap();
    assert_eq!(cache.topology_keys().computed, 0);
    assert!(cache.warm_parts().is_some(), "ordered warm");
    for (n, &o) in order.iter().enumerate() {
        assert_eq!(after.lines[n], before.lines[o], "route {} moved", ids[o]);
    }
    // Same routes, all ids new: new routes, drawn as a fresh call draws them.
    let renamed: Vec<u32> = rev_ids.iter().map(|id| id + 1).collect();
    let again =
        side_by_side_cached_with_ids(&mut cache, &renamed, &rev_lines, &[], &[], &s).unwrap();
    assert_eq!(
        again,
        side_by_side_with_stations(&rev_lines, &[], &s).unwrap()
    );
    // Two copies of one route trading places (equal routes, ids swapped): each keeps its
    // own line.
    let twins = vec![lines[0].clone(), lines[0].clone()];
    let mut c = Cache::new();
    let a = side_by_side_cached_with_ids(&mut c, &[1, 2], &twins, &[], &[], &s).unwrap();
    let b = side_by_side_cached_with_ids(&mut c, &[2, 1], &twins, &[], &[], &s).unwrap();
    assert_eq!(
        (b.lines[0].clone(), b.lines[1].clone()),
        (a.lines[1].clone(), a.lines[0].clone())
    );
    // Without ids, by place: everything again.
    let mut plain = Cache::new();
    side_by_side_cached(&mut plain, &lines, &[], &[], &s).unwrap();
    side_by_side_cached(&mut plain, &rev_lines, &[], &[], &s).unwrap();
    assert_eq!(plain.topology_keys().computed, plain.topology_keys().total);
    // Ids that can't be used (repeated, too few) behave like no ids.
    for bad in [vec![1; 12], vec![1, 2]] {
        let mut c = Cache::new();
        let a = side_by_side_cached_with_ids(&mut c, &bad, &lines, &[], &[], &s).unwrap();
        assert_eq!(a, side_by_side_with_stations(&lines, &[], &s).unwrap());
    }
}

#[test]
fn a_refused_call_leaves_the_cache_as_it_was() {
    let lines = vec![east(0.0, 0.0, 100.0)];
    let mut cache = Cache::new();
    side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(15.0)).unwrap();
    let before = cache.runs();
    let bad_zoom = side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(f64::NAN));
    assert!(matches!(bad_zoom, Err(PipelineError::InvalidSettings(_))));
    let bad_line = vec![vec![Point::new(f64::NAN, 0.0), Point::new(1.0, 0.0)]];
    assert!(side_by_side_cached(&mut cache, &bad_line, &[], &[], &at_zoom(15.0)).is_err());
    assert_eq!(cache.runs(), before);
    side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(16.0)).unwrap();
    assert_eq!(cache.runs().topology, before.topology, "still reused");
    // No routes: nothing runs, nothing is kept or lost.
    let none = side_by_side_cached(&mut cache, &[], &[], &[], &at_zoom(15.0)).unwrap();
    assert!(none.lines.is_empty());
    assert_eq!(cache.runs().topology, before.topology);
}

/// `n` points going round a circle of `r` meters around `(lng, lat)`, 20 a lap: route points
/// piled in one place.
fn knot(lng: f64, lat: f64, r: f64, n: usize) -> Vec<Point> {
    let proj = Projection::at(Point::new(lng, lat));
    (0..n)
        .map(|k| {
            let a = k as f64 * std::f64::consts::TAU / 20.0;
            proj.inverse(Point::new(r * a.cos(), r * a.sin()))
        })
        .collect()
}

/// A call past a work bound is refused with the work and its bound, and leaves the cache
/// as it was: the next call with the last routes reuses their graph.
#[test]
fn a_call_past_a_work_bound_is_refused_and_leaves_the_cache() {
    let street = east(-6.26, 53.35, 400.0);
    let lines = vec![street.clone(), street.clone()];
    let bounds = Bounds {
        nearby_checks: 50_000,
        graph_steps: 50_000,
        ..Bounds::default()
    };
    let mut cache = Cache::with_bounds(bounds);
    let first = side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(15.0)).unwrap();
    let (runs, keys) = (cache.runs(), cache.topology_keys());

    // Three routes going round a 10 m circle 10 times each.
    let knots: Vec<Vec<Point>> = (0..3).map(|_| knot(-6.26, 53.35, 10.0, 200)).collect();
    let crowded = vec![
        street.clone(),
        knots[0].clone(),
        knots[1].clone(),
        knots[2].clone(),
    ];
    let err = side_by_side_cached(&mut cache, &crowded, &[], &[], &at_zoom(15.0)).unwrap_err();
    assert_eq!(
        err,
        PipelineError::TooComplex {
            work: Work::NearbyChecks,
            max: 50_000
        }
    );
    assert_eq!(
        err.to_string(),
        "too complex: too many points lie close together (more than 50000 nearby-segment \
         checks)"
    );
    // Weaving across a street: flicker in every few meters.
    let weaving: Vec<Point> = {
        let proj = Projection::at(Point::new(-6.26, 53.35));
        (0..=16)
            .map(|k| {
                proj.inverse(Point::new(
                    f64::from(k) * 25.0,
                    if k % 2 == 0 { 6.0 } else { -6.0 },
                ))
            })
            .collect()
    };
    let weave = vec![street.clone(), weaving];
    let mut tight = Cache::with_bounds(Bounds {
        graph_steps: 1_000,
        ..Bounds::default()
    });
    assert_eq!(
        side_by_side_cached(&mut tight, &weave, &[], &[], &at_zoom(15.0)),
        Err(PipelineError::TooComplex {
            work: Work::GraphSteps,
            max: 1_000
        })
    );
    assert!(side_by_side_cached(&mut Cache::new(), &weave, &[], &[], &at_zoom(15.0)).is_ok());

    // Nothing changed: the last routes reuse their graph.
    assert_eq!((cache.runs(), cache.topology_keys()), (runs, keys));
    assert_eq!(
        side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(15.0)).unwrap(),
        first
    );
    assert_eq!(cache.runs().topology, runs.topology);
    // The capped call refuses the same way, whichever graph it reaches the bound in.
    let capped = side_by_side_capped(&mut cache, &[], &crowded, &[], &[], &at_zoom(15.0), 1);
    assert!(matches!(
        capped,
        Err(PipelineError::TooComplex {
            work: Work::NearbyChecks,
            ..
        })
    ));
}

/// Stations × the graph's centerline segments may be at most 50 million, checked before
/// any station is snapped (each one reads every segment, at every zoom) and before a new
/// graph is kept. A refusal leaves the cache as it was, on a new graph or a zoom change.
#[test]
fn too_many_station_checks_are_refused() {
    // A 72 km route zigzagging 1 m left and right every 6 m: about 12 000 centerline
    // segments, 5000 stations along it (60 million checks).
    let proj = Projection::at(Point::new(-6.26, 53.35));
    let zigzag: Vec<Point> = (0..12_000)
        .map(|k| {
            proj.inverse(Point::new(
                f64::from(k) * 6.0,
                if k % 2 == 0 { 1.0 } else { -1.0 },
            ))
        })
        .collect();
    let stops: Vec<Point> = (0..MAX_STATIONS)
        .map(|k| proj.inverse(Point::new(k as f64 * 14.0, 3.0)))
        .collect();
    let mut cache = Cache::new();
    let refused = Err(PipelineError::TooComplex {
        work: Work::StationChecks,
        max: 50_000_000,
    });
    let lines = vec![zigzag];
    assert_eq!(
        side_by_side_cached(&mut cache, &lines, &stops, &[], &at_zoom(15.0)),
        refused
    );
    assert_eq!(
        cache.runs(),
        StageRuns::default(),
        "the new graph was not kept"
    );
    // Without the stations the graph is kept; the stations then are refused at a zoom
    // change, and the next call still reuses the graph.
    side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(15.0)).unwrap();
    let runs = cache.runs();
    assert_eq!(
        side_by_side_cached(&mut cache, &lines, &stops, &[], &at_zoom(16.0)),
        refused
    );
    assert_eq!(cache.runs(), runs);
    let few = &stops[..10];
    assert!(side_by_side_cached(&mut cache, &lines, few, &[], &at_zoom(16.0)).is_ok());
    assert_eq!(cache.runs().topology, runs.topology);
    assert!(
        PipelineError::TooComplex {
            work: Work::StationChecks,
            max: 50_000_000
        }
        .to_string()
        .contains("stations × centerline segments"),
    );

    // At the bound is fine, one more is not: two streets on one edge, three stations.
    let street = east(-6.26, 53.35, 400.0);
    let two = vec![street.clone(), street];
    let three = [two[0][1], two[0][2], two[0][3]];
    let mut probe = Cache::new();
    side_by_side_cached(&mut probe, &two, &[], &[], &at_zoom(15.0)).unwrap();
    let segments: u64 = probe
        .last
        .as_ref()
        .map(|p| p.topo.edges.iter().map(|e| e.path.len() as u64 - 1).sum())
        .unwrap_or_default();
    let within = |station_checks| {
        Cache::with_bounds(Bounds {
            station_checks,
            ..Bounds::default()
        })
    };
    assert!(
        side_by_side_cached(&mut within(3 * segments), &two, &three, &[], &at_zoom(15.0)).is_ok()
    );
    assert_eq!(
        side_by_side_cached(
            &mut within(3 * segments - 1),
            &two,
            &three,
            &[],
            &at_zoom(15.0)
        ),
        Err(PipelineError::TooComplex {
            work: Work::StationChecks,
            max: 3 * segments - 1
        })
    );
}

/// Edges of `now` with an exact twin in `before` (same path, same renumbered lines) that
/// carry no changed route, whose order differs from the twin's (renumbered): an edit must
/// not move them.
fn untouched_orders_changed(
    before: &Prepared,
    now: &Prepared,
    edit: &topology::RouteEdit,
) -> usize {
    let to_new = |l: LineId| edit.old_to_new.get(l).copied().flatten();
    now.topo
        .edges
        .iter_enumerated()
        .filter(|(_, e)| !e.lines.iter().any(|(l, _)| edit.changed.contains(l)))
        .filter(|&(e, edge)| {
            before.topo.edges.iter_enumerated().any(|(k, old)| {
                let lines: Option<Vec<(LineId, topology::Dir)>> = old
                    .lines
                    .iter()
                    .map(|&(l, d)| to_new(l).map(|n| (n, d)))
                    .collect();
                let order: Option<Vec<LineId>> =
                    before.order[k].iter().map(|&l| to_new(l)).collect();
                old.path == edge.path
                    && lines.as_ref() == Some(&edge.lines)
                    && order.as_ref() != Some(&now.order[e])
            })
        })
        .count()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Through a cache, any sequence of zooms gives what a fresh call gives at each zoom.
    #[test]
    fn the_cache_changes_nothing(lines in city_lines(), zooms in prop::collection::vec(10.0f64..19.0, 1..4)) {
        let valid = lines.iter().all(|l| l.iter().any(|p| p != &l[0]));
        prop_assume!(valid);
        let mut cache = Cache::new();
        for &z in &zooms {
            let cached = side_by_side_cached(&mut cache, &lines, &[], &[], &at_zoom(z)).unwrap();
            prop_assert_eq!(cached, side_by_side_with_stations(&lines, &[], &at_zoom(z)).unwrap());
        }
        prop_assert_eq!(cache.runs().topology, u64::from(!lines.is_empty()));
    }

    /// Edits through a cache (a route dropped, then a point moved). The line graph and
    /// walks are the ones a fresh call builds; every input gets one finite line; and an edge
    /// the edit did not touch (it has an exact twin from before and carries no changed
    /// route) keeps its order.
    #[test]
    fn edits_through_the_cache(
        lines in city_lines(),
        drop in any::<usize>(),
        nudge in (any::<usize>(), -0.0002f64..0.0002),
    ) {
        let valid = lines.iter().all(|l| l.iter().any(|p| p != &l[0]));
        prop_assume!(valid && !lines.is_empty());
        let mut cache = Cache::new();
        let z = at_zoom(15.0);
        side_by_side_cached(&mut cache, &lines, &[], &[], &z).unwrap();
        let mut fewer = lines.clone();
        if fewer.len() > 1 {
            fewer.remove(drop % lines.len());
        }
        let mut moved = fewer.clone();
        let k = nudge.0 % moved.len();
        let m = moved[k].len() / 2;
        moved[k][m].x += nudge.1;
        let still_valid = moved[k].iter().any(|p| p != &moved[k][0]);
        prop_assume!(still_valid);
        for edited in [fewer, moved] {
            let before = cache.last.clone().unwrap();
            let out = side_by_side_cached(&mut cache, &edited, &[], &[], &z).unwrap();
            prop_assert_eq!(out.lines.len(), edited.len());
            prop_assert!(out.lines.iter().flatten().all(|q| q.x.is_finite() && q.y.is_finite()));
            let now = cache.last.clone().unwrap();
            let proj = Projection::around(&edited);
            let meters: Vec<Vec<Point>> = edited
                .iter()
                .map(|l| l.iter().map(|&q| proj.forward(q)).collect())
                .collect();
            let fresh = topology::build_with_traversals(&meters, &Params::default()).unwrap();
            prop_assert_eq!(&(now.topo.clone(), now.walks.clone()), &fresh);
            let edit = topology::route_edit(&before.lines, &edited);
            prop_assert_eq!(untouched_orders_changed(&before, &now, &edit), 0);
        }
    }
}
