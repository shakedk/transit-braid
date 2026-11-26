//! What an order costs at a node: crossings, separations, and lines that make no chord.

use std::cmp::Ordering;

use proptest::prelude::*;

use super::anneal::{LN_COOLING, START_TEMPERATURE};
use super::cost::counterclockwise;
use super::test_support::*;
use super::*;
use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, NodeId};
use crate::topology::{Dir, TopoEdge};

#[test]
fn y_the_trunk_takes_the_sides_of_the_branches() {
    let (t, walks) = y();
    // Input order A, B, C (A on the right, looking east): C must cross A and B to turn south.
    assert_eq!(costs(&t, &walks, &input_order(&t)), (0, 2, 0));
    // Hand-computed optimum: C on the right (south), A in the middle, B on the left (north).
    let best = Order::from(vec![vec![C, A, B], vec![B], vec![A], vec![C]]);
    assert_eq!(costs(&t, &walks, &best), (0, 0, 0));
    let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    assert_eq!(s.order, best);
    assert_eq!(s.cost.total, 0.0);
    assert_eq!((s.exhaustive_parts, s.annealed_parts), (1, 0));
}

#[test]
fn x_one_crossing_cannot_be_avoided_and_a_tie_keeps_the_input_order() {
    let (t, walks) = x();
    // A on the right: they cross where they join; A on the left: where they part.
    assert_eq!(costs(&t, &walks, &input_order(&t)), (0, 1, 0));
    let mut flipped = input_order(&t);
    flipped[EdgeId::new(0)] = vec![B, A];
    assert_eq!(costs(&t, &walks, &flipped), (0, 1, 0));
    let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    assert_eq!(s.cost.different_segment, 1);
    assert_eq!(s.order, input_order(&t));
}

#[test]
fn x_crossing_fixture_lines_sharing_no_edge_cost_nothing() {
    // Two corridors crossing with no node: one line per edge, nothing to order.
    let t = graph(
        &[(0.0, 0.0), (200.0, 0.0), (100.0, -100.0), (100.0, 100.0)],
        &[(0, 1, &[A]), (2, 3, &[B])],
    );
    let walks = walks_of(vec![vec![(0, F)], vec![(1, F)]]);
    let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    assert_eq!(s.cost, Cost::default());
    assert_eq!(s.exhaustive_parts, 0, "single-line edges need no search");
}

#[test]
fn dog_bone_the_north_line_stays_on_the_north_side() {
    let (t, walks) = dog_bone();
    // A on the right (south) of the shared edge: it crosses B where they join and where they part.
    assert_eq!(costs(&t, &walks, &input_order(&t)), (0, 2, 0));
    let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    assert_eq!(
        s.order[EdgeId::new(0)],
        vec![B, A],
        "B right (south), A left (north)"
    );
    assert_eq!(s.cost, Cost::default());
}

#[test]
fn a_twist_between_two_edges_is_a_same_segment_crossing() {
    let (t, walks) = straight();
    assert_eq!(
        costs(&t, &walks, &Order::from(vec![vec![A, B], vec![A, B]])),
        (0, 0, 0)
    );
    let twisted = Order::from(vec![vec![A, B], vec![B, A]]);
    assert_eq!(costs(&t, &walks, &twisted), (1, 0, 0));
    assert_eq!(cost(&t, &walks, &twisted, &w()).total, 3.0);
    // The same, with the second edge drawn the other way (B→… as its `from`).
    let mut back = t.clone();
    back.edges[EdgeId::new(1)] = TopoEdge {
        from: NodeId::new(2),
        to: NodeId::new(1),
        path: vec![t.nodes[NodeId::new(2)], t.nodes[NodeId::new(1)]],
        lines: vec![(A, Dir::Backward), (B, Dir::Backward)],
    };
    let walks_back = walks_of(vec![
        vec![(0, F), (1, Dir::Backward)],
        vec![(0, F), (1, Dir::Backward)],
    ]);
    // Seen from its own `from` end the order is mirrored: [B, A] there is no twist.
    assert_eq!(
        costs(
            &back,
            &walks_back,
            &Order::from(vec![vec![A, B], vec![B, A]])
        ),
        (0, 0, 0)
    );
    assert_eq!(
        costs(
            &back,
            &walks_back,
            &Order::from(vec![vec![A, B], vec![A, B]])
        ),
        (1, 0, 0)
    );
}

#[test]
fn a_line_squeezed_between_two_neighbors_is_a_separation() {
    // A and B run west → east through Q; X joins at Q from the north.
    let t = graph(
        &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0), (100.0, 100.0)],
        &[(0, 1, &[A, B]), (1, 2, &[A, B, X]), (3, 1, &[X])],
    );
    let walks = walks_of(vec![
        vec![(0, F), (1, F)],
        vec![(0, F), (1, F)],
        vec![],
        vec![(2, F), (1, F)],
    ]);
    // X between A and B: they are apart after Q (a separation), and X crosses B.
    assert_eq!(
        costs(
            &t,
            &walks,
            &Order::from(vec![vec![A, B], vec![A, X, B], vec![X]])
        ),
        (0, 1, 1)
    );
    // X on the north (left) side: nothing.
    assert_eq!(
        costs(
            &t,
            &walks,
            &Order::from(vec![vec![A, B], vec![A, B, X], vec![X]])
        ),
        (0, 0, 0)
    );
    let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    assert_eq!(s.cost, Cost::default());
    assert_eq!(s.order[EdgeId::new(1)], vec![A, B, X]);
}

#[test]
fn lines_that_end_or_turn_back_at_a_node_make_no_chord() {
    let (t, _) = straight();
    // A ends at the middle node; B runs out on edge 1 and back.
    let walks = walks_of(vec![
        vec![(0, F)],
        vec![(0, F), (1, F), (1, Dir::Backward), (0, Dir::Backward)],
    ]);
    assert_eq!(
        costs(&t, &walks, &Order::from(vec![vec![A, B], vec![B, A]])),
        (0, 0, 0)
    );
}

#[test]
fn broken_walks_and_orders_are_skipped_not_panics() {
    let (t, _) = y();
    // A step on an edge that doesn't exist, and a jump (edges 1 and 3 don't meet).
    let walks = walks_of(vec![vec![(9, F), (0, F)], vec![(1, F), (3, F)], vec![]]);
    assert_eq!(costs(&t, &walks, &input_order(&t)), (0, 0, 0));
    // An order missing an edge or a line costs nothing for it.
    let (t, walks) = y();
    assert_eq!(costs(&t, &walks, &Order::from(vec![vec![A, B]])), (0, 0, 0));
    assert_eq!(
        solve(
            &Topology::default(),
            &IdVec::new(),
            &w(),
            ExhaustiveLimit::default()
        )
        .cost,
        Cost::default()
    );
}

#[test]
fn directions_sort_counterclockwise_from_east_exactly() {
    let p = |x, y| Point::new(x, y);
    let mut dirs = vec![
        p(0.0, -1.0),
        p(-1.0, 0.0),
        p(1.0, 1.0),
        p(1.0, 0.0),
        p(-1.0, -1.0),
        p(0.0, 1.0),
        p(1.0, -1e-12),
    ];
    dirs.sort_by(|a, b| counterclockwise(*a, *b));
    let angles: Vec<f64> = dirs
        .iter()
        .map(|d| d.y.atan2(d.x).rem_euclid(std::f64::consts::TAU))
        .collect();
    assert!(angles.windows(2).all(|w| w[0] <= w[1]), "{dirs:?}");
    assert_eq!(
        counterclockwise(p(2.0, 2.0), p(1.0, 1.0)),
        Ordering::Equal,
        "same direction"
    );
    assert_eq!(
        counterclockwise(p(0.0, 0.0), p(1.0, 0.0)),
        Ordering::Equal,
        "zero counts as east"
    );
    assert!((LN_COOLING - (0.01_f64 / START_TEMPERATURE).ln()).abs() < 1e-15);
}

proptest! {
    /// Drawing every edge the other way (and mirroring each order to match) is the same
    /// picture, so it costs the same.
    #[test]
    fn mirroring_every_edge_keeps_the_cost((t, walks) in star()) {
        // Reversing every order and every edge's direction is the same picture, mirrored.
        let order = input_order(&t);
        let c = cost(&t, &walks, &order, &w());
        let mut mirrored = t.clone();
        let mut mirrored_walks = walks.clone();
        for e in &mut mirrored.edges {
            std::mem::swap(&mut e.from, &mut e.to);
            e.path.reverse();
        }
        for walk in &mut mirrored_walks {
            for step in walk.iter_mut() {
                step.1 = if step.1 == Dir::Backward { F } else { Dir::Backward };
            }
        }
        let reversed: Order = order.iter().map(|l| l.iter().rev().copied().collect()).collect();
        prop_assert_eq!(c, cost(&mirrored, &mirrored_walks, &reversed, &w()));
    }
}
