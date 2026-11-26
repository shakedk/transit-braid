//! Solving every part: the exhaustive search, the annealing and its random generator, and the
//! search's swaps costed exactly.

use proptest::prelude::*;

use super::anneal::SplitMix64;
use super::exhaustive::next_permutation;
use super::search::Search;
use super::test_support::*;
use super::*;
use crate::geom::Point;
use crate::ids::{EdgeId, LineId, NodeId};
use crate::topology::{Dir, TopoEdge};

#[test]
fn a_part_too_large_is_annealed_and_still_finds_the_optimum() {
    let (t, walks) = y();
    let s = solve(&t, &walks, &w(), ExhaustiveLimit(5)); // 3! = 6 orders > 5
    assert_eq!((s.exhaustive_parts, s.annealed_parts), (0, 1));
    assert_eq!(s.order, vec![vec![C, A, B], vec![B], vec![A], vec![C]]);
    assert_eq!(s.cost, Cost::default());
}

#[test]
fn annealing_alone_reaches_the_hand_computed_optima() {
    let none = ExhaustiveLimit(0);
    for (name, (t, walks)) in [
        ("y", y()),
        ("dog-bone", dog_bone()),
        ("straight", straight()),
    ] {
        let s = solve(&t, &walks, &w(), none);
        assert_eq!(s.cost, Cost::default(), "{name}");
        assert_eq!(s.annealed_parts, 1, "{name}");
    }
    let (t, walks) = x();
    assert_eq!(
        solve(&t, &walks, &w(), none).cost.different_segment,
        1,
        "X: the one forced crossing"
    );
}

#[test]
fn the_same_seed_gives_the_same_order_and_no_steps_still_descends() {
    let (t, walks) = dog_bone();
    let a = Annealing {
        seed: 7,
        ..Annealing::default()
    };
    let none = ExhaustiveLimit(0);
    assert_eq!(
        solve_with(&t, &walks, &w(), none, &a),
        solve_with(&t, &walks, &w(), none, &a)
    );
    // With no annealing steps, the greedy descent alone fixes the dog-bone.
    let greedy = Annealing {
        max_steps: 0,
        min_steps: 0,
        ..a
    };
    assert_eq!(
        solve_with(&t, &walks, &w(), none, &greedy).cost,
        Cost::default()
    );
}

#[test]
fn the_random_generator_stays_in_range() {
    let mut r = SplitMix64(1);
    for _ in 0..1000 {
        assert!(r.below(3) < 3);
        let u = r.unit();
        assert!((0.0..1.0).contains(&u));
    }
    assert_eq!(SplitMix64(42).next(), SplitMix64(42).next());
    assert_ne!(SplitMix64(1).next(), SplitMix64(2).next());
}

#[test]
fn next_permutation_walks_every_order_once() {
    let mut v = vec![0, 1, 2];
    let mut seen = vec![v.clone()];
    while next_permutation(&mut v) {
        seen.push(v.clone());
    }
    assert_eq!(seen.len(), 6);
    assert_eq!(seen.last(), Some(&vec![2, 1, 0]));
    assert!(!next_permutation(&mut []));
    assert_eq!(factorial(0), 1);
    assert_eq!(factorial(5), 120);
    assert_eq!(factorial(30), u64::MAX, "too large: saturates");
}

/// Found by the property test: two lines that run together over three edges can only change
/// sides together; one edge at a time, each swap first makes a twist (cost 3). Swapping them on
/// their whole shared stretch at once finds the optimum, with or without annealing steps.
#[test]
fn lines_running_together_change_sides_together() {
    let nodes: Vec<Point> = [
        (0.0, 0.0),
        (100.0, 0.0),
        (200.0, 0.0),
        (300.0, 0.0),
        (130.0, 100.0),
        (130.0, -100.0),
        (230.0, -100.0),
        (330.0, 100.0),
    ]
    .iter()
    .map(|&(x, y)| Point::new(x, y))
    .collect();
    // A from the north at stop 1, C from the south there, B from the south at stop 2; all
    // leave north at stop 3.
    let edges: Vec<(usize, usize, Vec<LineId>)> = vec![
        (1, 2, vec![A, C]),
        (2, 3, vec![A, B, C]),
        (1, 4, vec![A]),
        (1, 5, vec![C]),
        (2, 6, vec![B]),
        (3, 7, vec![A, B, C]),
    ];
    let edges = edges
        .into_iter()
        .map(|(from, to, lines)| TopoEdge {
            from: NodeId::new(from),
            to: NodeId::new(to),
            path: vec![nodes[from], nodes[to]],
            lines: lines.into_iter().map(|l| (l, F)).collect(),
        })
        .collect();
    let t = Topology {
        nodes: nodes.into(),
        edges,
    };
    let walks = walks_of(vec![
        vec![(2, Dir::Backward), (0, F), (1, F), (5, F)],
        vec![(4, Dir::Backward), (1, F), (5, F)],
        vec![(3, Dir::Backward), (0, F), (1, F), (5, F)],
    ]);
    assert_eq!(cost(&t, &walks, &input_order(&t), &w()).total, 4.0);
    let best = solve(&t, &walks, &w(), ExhaustiveLimit(u64::MAX));
    assert_eq!(best.cost.total, 0.0);
    for steps in [0, 100, 10_000] {
        for seed in [1, 2, 3] {
            let a = Annealing {
                seed,
                steps_per_slot: u64::MAX,
                max_steps: steps,
                min_steps: 0,
            };
            let s = solve_with(&t, &walks, &w(), ExhaustiveLimit(0), &a);
            assert_eq!(
                s.cost.total, 0.0,
                "steps {steps}, seed {seed}: {:?}",
                s.order
            );
        }
    }
}

proptest! {
    /// Annealing (exhaustive search off) ends within 10% of the exhaustive
    /// optimum, and the same seed gives the same order.
    #[test]
    fn annealing_is_within_ten_percent_of_exhaustive((t, walks) in corridor()) {
        let best = solve(&t, &walks, &w(), ExhaustiveLimit(u64::MAX));
        prop_assert_eq!(best.annealed_parts, 0);
        let annealed = solve(&t, &walks, &w(), ExhaustiveLimit(0));
        prop_assert!(
            annealed.cost.total <= best.cost.total * 1.1 + 1e-9,
            "annealed {:?} vs exhaustive {:?}", annealed.cost, best.cost
        );
        prop_assert_eq!(&annealed, &solve(&t, &walks, &w(), ExhaustiveLimit(0)));
    }

    /// The fast search's cost of a swap (on one edge, or on a shared stretch) equals the
    /// difference of the full costs before and after: the incremental costing is exact.
    #[test]
    fn a_swap_costs_what_the_full_cost_says((t, walks) in corridor(), picks in prop::collection::vec((0_usize..20, 0_usize..10, 0_usize..10, any::<bool>()), 1..20)) {
        let views = node_views(&t, &walks);
        let mut s = Search::new(&t, &views, input_order(&t), &w(), walks.len());
        for (e, i, j, stretch) in picks {
            let e = EdgeId::new(e % t.edges.len());
            let n = s.order[e].len();
            if n < 2 {
                continue;
            }
            let (a, b) = (s.order[e][i % n], s.order[e][j % n]);
            if a == b {
                continue;
            }
            let run = if stretch { s.shared_run(e, a, b) } else { vec![e] };
            let before = cost(&t, &walks, &s.order, &w()).total;
            let delta = s.swap_delta(&run, a, b);
            let after = cost(&t, &walks, &s.order, &w()).total;
            prop_assert!((delta - (after - before)).abs() < 1e-9, "delta {} vs {} on {:?}", delta, after - before, run);
        }
    }

    /// The solution is never worse than the input order, is a valid order (each edge keeps
    /// exactly its lines), costs what `cost` says, and is the same every time.
    #[test]
    fn solve_is_never_worse_valid_and_deterministic((t, walks) in star()) {
        let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
        prop_assert!(s.cost.total <= cost(&t, &walks, &input_order(&t), &w()).total);
        for (e, lines) in s.order.iter_enumerated() {
            let mut sorted = lines.clone();
            sorted.sort_unstable();
            prop_assert_eq!(sorted, input_order(&t)[e].clone());
        }
        prop_assert_eq!(s.cost, cost(&t, &walks, &s.order, &w()));
        prop_assert_eq!(&s, &solve(&t, &walks, &w(), ExhaustiveLimit::default()));
    }
}
