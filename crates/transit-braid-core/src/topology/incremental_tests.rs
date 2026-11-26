//! The incremental build after edits, matched by place or by id: always exactly the full
//! build, and the work budget counted as a full build counts it.

use proptest::prelude::*;

use super::incremental::Pending;
use super::*;

/// Routes along a 100 m street grid, jittered under 2 m (so they share streets, cross,
/// join and part, like routes).
fn street_walks() -> impl Strategy<Value = Vec<Vec<Point>>> {
    let walk = (
        (0..4_i32, 0..4_i32),
        prop::collection::vec(0..4_usize, 1..6),
        (-1.5..1.5_f64, -1.5..1.5_f64),
    )
        .prop_map(|((x, y), moves, (jx, jy))| {
            let mut at = (x, y);
            let mut out = vec![Point::new(
                f64::from(x) * 100.0 + jx,
                f64::from(y) * 100.0 + jy,
            )];
            for m in moves {
                let (dx, dy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][m];
                at = (at.0 + dx, at.1 + dy);
                out.push(Point::new(
                    f64::from(at.0) * 100.0 + jx,
                    f64::from(at.1) * 100.0 + jy,
                ));
            }
            out
        });
    prop::collection::vec(walk, 1..8)
}

/// One edit of a list of routes: replace, remove or add one, move one point, or nothing.
#[derive(Debug, Clone)]
enum Edit {
    Replace(usize, Vec<Point>),
    Remove(usize),
    Insert(usize, Vec<Point>),
    Nudge(usize, f64),
    Nothing,
}

fn edits() -> impl Strategy<Value = Edit> {
    let one = || street_walks().prop_map(|mut v| v.swap_remove(0));
    prop_oneof![
        (any::<usize>(), one()).prop_map(|(k, l)| Edit::Replace(k, l)),
        any::<usize>().prop_map(Edit::Remove),
        (any::<usize>(), one()).prop_map(|(k, l)| Edit::Insert(k, l)),
        (any::<usize>(), -30.0..30.0_f64).prop_map(|(k, d)| Edit::Nudge(k, d)),
        Just(Edit::Nothing),
    ]
}

fn apply(lines: &[Vec<Point>], edit: &Edit) -> Vec<Vec<Point>> {
    let mut out = lines.to_vec();
    let n = out.len();
    match edit {
        Edit::Replace(k, l) if n > 0 => out[k % n].clone_from(l),
        Edit::Remove(k) if n > 1 => {
            out.remove(k % n);
        }
        Edit::Insert(k, l) => out.insert(k % (n + 1), l.clone()),
        Edit::Nudge(k, d) if n > 0 => {
            let line = &mut out[k % n];
            let m = line.len() / 2;
            line[m].x += d;
        }
        _ => {}
    }
    out
}

#[test]
fn an_edit_recomputes_only_the_keys_near_it() {
    // Ten parallel streets 100 m apart; one route on the first street changes.
    let lines: Vec<Vec<Point>> = (0..10)
        .map(|k| {
            vec![
                Point::new(0.0, f64::from(k) * 100.0),
                Point::new(1000.0, f64::from(k) * 100.0),
            ]
        })
        .collect();
    let mut state = Incremental::new();
    let full = build_incremental(&mut state, &lines, &Params::default()).unwrap();
    assert_eq!(
        full,
        build_with_traversals(&lines, &Params::default()).unwrap()
    );
    let all = state.last_keys();
    assert_eq!(all.computed, all.total);
    // Nothing changed: no key is computed.
    build_incremental(&mut state, &lines, &Params::default()).unwrap();
    assert_eq!(
        state.last_keys(),
        KeyCounts {
            computed: 0,
            total: all.total
        }
    );
    // One street's route bends: its own keys and none of the far streets'.
    let mut edited = lines.clone();
    edited[0].insert(1, Point::new(500.0, 20.0));
    let got = build_incremental(&mut state, &edited, &Params::default()).unwrap();
    assert_eq!(
        got,
        build_with_traversals(&edited, &Params::default()).unwrap()
    );
    let some = state.last_keys();
    assert!(
        some.computed > 0 && some.computed * 5 < some.total,
        "{some:?}"
    );
}

/// `n` points going round a circle of `r` meters around `(cx, cy)`, 20 points a lap, the
/// first at angle `phase` (in twentieths of a lap): a knot of points piled in one place.
fn knot(cx: f64, cy: f64, r: f64, n: usize, phase: f64) -> Vec<Point> {
    (0..n)
        .map(|k| {
            let a = (k as f64 + phase) * std::f64::consts::TAU / 20.0;
            Point::new(cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// A pending build from an empty state within `bounds`.
fn fresh_within(lines: &[Vec<Point>], bounds: &Bounds) -> Result<Pending, TopologyError> {
    build_incremental_within(&Incremental::new(), &[], lines, &Params::default(), bounds)
        .map(|(_, _, p)| p)
}

/// The nearby checks are those of the whole graph, however it was reached: a fresh
/// build, an edit that adds a route, or one that removes one. So the same routes are
/// refused, or not, whatever came before; and a refused call leaves the state as it was.
#[test]
fn the_nearby_budget_counts_the_whole_graph_whatever_came_before() {
    let street = |y: f64| vec![Point::new(0.0, y), Point::new(600.0, y)];
    let lines = vec![
        street(0.0),
        street(2.0),
        street(4.0),
        vec![Point::new(300.0, -200.0), Point::new(300.0, 200.0)],
        knot(450.0, 3.0, 8.0, 60, 0.0),
    ];
    let p = Params::default();
    let all = fresh_within(&lines, &Bounds::default())
        .unwrap()
        .nearby_checks();
    assert!(all > 1_000, "{all}");
    let within = |nearby_checks| Bounds {
        nearby_checks,
        ..Bounds::default()
    };
    assert!(fresh_within(&lines, &within(all)).is_ok(), "at the bound");
    let refused = Err(TopologyError::TooComplex {
        work: Work::NearbyChecks,
        max: all - 1,
    });
    assert_eq!(fresh_within(&lines, &within(all - 1)).map(|_| ()), refused);

    // An edit that adds the knot or the crossing street computes only some keys, and still
    // counts all of them.
    for added in [4, 3] {
        let mut fewer = lines.clone();
        fewer.remove(added);
        let mut state = Incremental::new();
        build_incremental(&mut state, &fewer, &p).unwrap();
        let before = state.last_keys();
        let got = build_incremental_within(&state, &[], &lines, &p, &within(all - 1));
        assert_eq!(got.map(|_| ()), refused, "adding {added}");
        assert_eq!(state.last_keys(), before, "a refused call keeps the state");
        let (graph, _, pending) =
            build_incremental_within(&state, &[], &lines, &p, &within(all)).unwrap();
        assert!(
            pending.keys().computed < pending.keys().total,
            "incremental"
        );
        assert_eq!(pending.nearby_checks(), all);
        assert_eq!(graph, build(&lines, &p).unwrap());
        // Removing it again counts what a fresh build of the rest counts.
        state.keep(pending);
        let (_, _, back) =
            build_incremental_within(&state, &[], &fewer, &p, &Bounds::default()).unwrap();
        assert!(back.keys().computed < back.keys().total, "incremental");
        assert_eq!(
            back.nearby_checks(),
            fresh_within(&fewer, &Bounds::default())
                .unwrap()
                .nearby_checks()
        );
    }
}

/// Two routes weaving across each other make the flicker smoothing take one pass per
/// crossing; past the graph-step bound the build stops with an error, a fresh build or an
/// edit alike.
#[test]
fn the_graph_step_budget_refuses_weaving_routes() {
    let straight = vec![Point::new(0.0, 0.0), Point::new(2_000.0, 0.0)];
    let weaving: Vec<Point> = (0..=80)
        .map(|k| Point::new(f64::from(k) * 25.0, if k % 2 == 0 { 6.0 } else { -6.0 }))
        .collect();
    let lines = vec![straight.clone(), weaving];
    let p = Params::default();
    assert!(build(&lines, &p).is_ok());
    let tight = Bounds {
        graph_steps: 10_000,
        ..Bounds::default()
    };
    let refused = Err(TopologyError::TooComplex {
        work: Work::GraphSteps,
        max: 10_000,
    });
    assert_eq!(build_within(&lines, &p, &tight).map(|_| ()), refused);
    let mut state = Incremental::new();
    build_incremental(&mut state, &[straight.clone(), straight], &p).unwrap();
    assert_eq!(
        build_incremental_within(&state, &[], &lines, &p, &tight).map(|_| ()),
        refused
    );
    let message = TopologyError::TooComplex {
        work: Work::GraphSteps,
        max: 10_000,
    }
    .to_string();
    assert!(
        message.starts_with("too complex: the routes cross and overlap"),
        "{message}"
    );
}

/// Matched by id, reordering the routes or swapping one route for another stays
/// incremental, and the result is still exactly the full build's.
#[test]
fn matched_by_id_a_reorder_or_a_swap_stays_incremental() {
    let street = |k: u32| {
        vec![
            Point::new(0.0, f64::from(k) * 100.0),
            Point::new(1000.0, f64::from(k) * 100.0),
        ]
    };
    let lines: Vec<Vec<Point>> = (0..10).map(street).collect();
    let ids: Vec<u32> = (100..110).collect();
    let p = Params::default();
    let mut state = Incremental::new();
    build_incremental_with_ids(&mut state, &ids, &lines, &p).unwrap();
    // The same routes, listed backwards: nothing to compute.
    let (rev_lines, rev_ids): (Vec<_>, Vec<_>) =
        lines.iter().cloned().zip(ids.iter().copied()).rev().unzip();
    let got = build_incremental_with_ids(&mut state, &rev_ids, &rev_lines, &p).unwrap();
    assert_eq!(got, build_with_traversals(&rev_lines, &p).unwrap());
    assert_eq!(state.last_keys().computed, 0, "a reorder computes no key");
    // Route 103 replaced by a new route 200 on an eleventh street, in the same call.
    let mut swap_lines = rev_lines.clone();
    let mut swap_ids = rev_ids.clone();
    let at = swap_ids.iter().position(|&id| id == 103).unwrap();
    swap_lines.remove(at);
    swap_ids.remove(at);
    swap_lines.insert(0, street(10));
    swap_ids.insert(0, 200);
    let got = build_incremental_with_ids(&mut state, &swap_ids, &swap_lines, &p).unwrap();
    assert_eq!(got, build_with_traversals(&swap_lines, &p).unwrap());
    let keys = state.last_keys();
    assert!(
        keys.computed > 0 && keys.computed * 4 < keys.total,
        "{keys:?}"
    );
    // By position, the same swap shifted every route: everything computed again.
    let mut by_position = Incremental::new();
    build_incremental(&mut by_position, &rev_lines, &p).unwrap();
    build_incremental(&mut by_position, &swap_lines, &p).unwrap();
    assert_eq!(
        by_position.last_keys().computed,
        by_position.last_keys().total
    );
}

/// Ids that can't be matched by (missing, one too few, repeated) fall back to positions; the
/// result is the full build's every time.
#[test]
fn unusable_ids_match_by_position() {
    assert!(usable_ids(&[1, 2, 3], 3));
    assert!(!usable_ids(&[], 0));
    assert!(!usable_ids(&[1, 2], 3));
    assert!(!usable_ids(&[1, 1, 2], 3));
    let lines = vec![
        vec![Point::new(0.0, 0.0), Point::new(300.0, 0.0)],
        vec![Point::new(0.0, 200.0), Point::new(300.0, 200.0)],
    ];
    let p = Params::default();
    for ids in [vec![], vec![7], vec![7, 7], vec![u32::MAX, 0]] {
        let mut state = Incremental::new();
        build_incremental_with_ids(&mut state, &ids, &lines, &p).unwrap();
        let got = build_incremental_with_ids(&mut state, &ids, &lines, &p).unwrap();
        assert_eq!(got, build_with_traversals(&lines, &p).unwrap(), "{ids:?}");
        assert_eq!(state.last_keys().computed, 0, "unchanged: {ids:?}");
    }
}

#[test]
fn route_edits_by_id_follow_routes_wherever_they_move() {
    let a = vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)];
    let b = vec![Point::new(0.0, 5.0), Point::new(1.0, 5.0)];
    let c = vec![Point::new(0.0, 9.0), Point::new(1.0, 9.0)];
    let b2 = vec![Point::new(0.0, 6.0), Point::new(1.0, 6.0)];
    // Old [a, b, c] as ids 1, 2, 3; new [c, b2, d] as 3, 2, 4: a removed, b edited, d new.
    let d = vec![Point::new(0.0, 20.0), Point::new(1.0, 20.0)];
    let edit = route_edit_by_ids(
        &[1, 2, 3],
        &[a.clone(), b.clone(), c.clone()],
        &[3, 2, 4],
        &[c.clone(), b2, d],
    );
    assert_eq!(
        edit.old_to_new,
        vec![None, Some(LineId::new(1)), Some(LineId::new(0))]
    );
    assert_eq!(edit.changed, vec![LineId::new(1), LineId::new(2)]);
    // A pure reorder changes nothing.
    let edit = route_edit_by_ids(
        &[1, 2],
        &[a.clone(), b.clone()],
        &[2, 1],
        &[b.clone(), a.clone()],
    );
    assert_eq!(
        edit.old_to_new,
        vec![Some(LineId::new(1)), Some(LineId::new(0))]
    );
    assert!(edit.changed.is_empty());
    // Unusable ids: by position, as `route_edit`.
    let old = [a.clone(), b.clone()];
    let new = [b, a];
    assert_eq!(
        route_edit_by_ids(&[1, 1], &old, &[2, 1], &new),
        route_edit(&old, &new)
    );
    assert_eq!(
        route_edit_by_ids(&[], &old, &[], &new),
        route_edit(&old, &new)
    );
    assert_eq!(route_edit_by_ids(&[], &[], &[], &[]), route_edit(&[], &[]));
}

#[test]
fn new_settings_or_a_refused_call_rebuild_safely() {
    let lines = vec![
        vec![Point::new(0.0, 0.0), Point::new(300.0, 0.0)],
        vec![Point::new(0.0, 2.0), Point::new(300.0, 2.0)],
    ];
    let mut state = Incremental::new();
    build_incremental(&mut state, &lines, &Params::default()).unwrap();
    // A refused call (bad point, bad settings) leaves the state as it was.
    let bad = vec![
        lines[0].clone(),
        vec![Point::new(f64::NAN, 0.0), Point::new(1.0, 0.0)],
    ];
    assert_eq!(
        build_incremental(&mut state, &bad, &Params::default()),
        Err(TopologyError::NotFinite { route: 1 })
    );
    let wrong = Params {
        merge_m: -1.0,
        ..Params::default()
    };
    assert!(build_incremental(&mut state, &lines, &wrong).is_err());
    build_incremental(&mut state, &lines, &Params::default()).unwrap();
    assert_eq!(state.last_keys().computed, 0, "still the last good state");
    // Other settings: everything again, the same as a full build with them.
    let wider = Params {
        merge_m: 8.0,
        ..Params::default()
    };
    let got = build_incremental(&mut state, &lines, &wider).unwrap();
    assert_eq!(got, build_with_traversals(&lines, &wider).unwrap());
    assert_eq!(state.last_keys().computed, state.last_keys().total);
    // No routes at all.
    assert_eq!(
        build_incremental(&mut state, &[], &Params::default()).unwrap(),
        build_with_traversals(&[], &Params::default()).unwrap()
    );
}

proptest! {
    /// After any two edits in a row, the incremental build equals the
    /// full build, graph and walks, exactly.
    #[test]
    fn incremental_equals_full_recompute(
        input in street_walks(),
        first in edits(),
        second in edits(),
    ) {
        let params = Params::default();
        let mut state = Incremental::new();
        build_incremental(&mut state, &input, &params).unwrap();
        let once = apply(&input, &first);
        prop_assert_eq!(
            build_incremental(&mut state, &once, &params).unwrap(),
            build_with_traversals(&once, &params).unwrap()
        );
        let twice = apply(&once, &second);
        prop_assert_eq!(
            build_incremental(&mut state, &twice, &params).unwrap(),
            build_with_traversals(&twice, &params).unwrap()
        );
    }

    /// After any two edits in a row, the incremental build counts the nearby checks a
    /// fresh build of the same routes counts, so the budget refuses the same routes
    /// whatever came before.
    #[test]
    fn incremental_counts_what_a_fresh_build_counts(
        input in street_walks(),
        first in edits(),
        second in edits(),
    ) {
        let params = Params::default();
        let all = Bounds::default();
        let mut state = Incremental::new();
        build_incremental(&mut state, &input, &params).unwrap();
        for lines in [apply(&input, &first), apply(&apply(&input, &first), &second)] {
            let (_, _, pending) = build_incremental_within(&state, &[], &lines, &params, &all).unwrap();
            prop_assert_eq!(pending.nearby_checks(), fresh_within(&lines, &all).unwrap().nearby_checks());
            state.keep(pending);
        }
    }

    /// Matched by id, after the routes are shuffled and then edited, the incremental
    /// build equals the full build, graph and walks, exactly.
    #[test]
    fn incremental_by_id_equals_full_recompute(
        input in street_walks(),
        shuffle in prop::collection::vec(any::<u64>(), 0..8),
        edit in edits(),
    ) {
        let params = Params::default();
        let ids: Vec<u32> = (0..input.len() as u32).map(|i| i + 1000).collect();
        let mut state = Incremental::new();
        build_incremental_with_ids(&mut state, &ids, &input, &params).unwrap();
        let mut order: Vec<usize> = (0..input.len()).collect();
        order.sort_by_key(|&i| (shuffle.get(i).copied().unwrap_or(0), i));
        let lines: Vec<Vec<Point>> = order.iter().map(|&i| input[i].clone()).collect();
        let moved: Vec<u32> = order.iter().map(|&i| ids[i]).collect();
        prop_assert_eq!(
            build_incremental_with_ids(&mut state, &moved, &lines, &params).unwrap(),
            build_with_traversals(&lines, &params).unwrap()
        );
        prop_assert_eq!(state.last_keys().computed, 0, "a reorder computes no key");
        // Then an edit; edited routes keep their ids, new ones get fresh ids.
        let edited = apply(&lines, &edit);
        let edited_ids: Vec<u32> = (0..edited.len() as u32)
            .map(|i| moved.get(i as usize).copied().unwrap_or(5000 + i))
            .collect();
        prop_assert_eq!(
            build_incremental_with_ids(&mut state, &edited_ids, &edited, &params).unwrap(),
            build_with_traversals(&edited, &params).unwrap()
        );
    }
}
