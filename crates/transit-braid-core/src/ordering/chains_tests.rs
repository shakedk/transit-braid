//! Chains of pass-through edges: found where they should be, and the optimum kept.

use proptest::prelude::*;

use super::test_support::*;
use super::*;
use crate::ids::{EdgeId, IdVec};
use crate::topology::Dir;

/// A street of three edges through two pass-through nodes, carrying A and B, with a
/// twist in the input order and the middle edge drawn the other way. The three edges are one
/// chain with one order (fewer states), carried straight through: no twist left.
#[test]
fn a_chain_of_pass_through_edges_has_one_order() {
    let t = graph(
        &[
            (-100.0, 50.0),
            (0.0, 0.0),
            (100.0, 0.0),
            (200.0, 0.0),
            (300.0, 50.0),
            (300.0, -50.0),
        ],
        &[
            (0, 1, &[A, B]),
            (2, 1, &[A, B]), // drawn west: A, B here twists at nodes 1 and 2
            (2, 3, &[A, B]),
            (3, 4, &[A]),
            (3, 5, &[B]),
        ],
    );
    let walks = walks_of(vec![
        vec![(0, F), (1, Dir::Backward), (2, F), (3, F)],
        vec![(0, F), (1, Dir::Backward), (2, F), (4, F)],
    ]);
    let views = node_views(&t, &walks);
    let chains = Chains::new(&t, &views);
    assert_eq!(chains.members[EdgeId::new(0)], edge_ids(&[0, 1, 2]));
    assert_eq!(chains.root, edge_ids(&[0, 0, 0, 3, 4]));
    assert_eq!(chains.flip, vec![false, true, false, false, false]);
    let order = input_order(&t);
    let plain = Chains::identity(t.edges.len()).log10_states(&order);
    assert!((plain - 3.0 * 2_f64.log10()).abs() < 1e-12);
    assert!((chains.log10_states(&order) - 2_f64.log10()).abs() < 1e-12);
    let mut carried = order.clone();
    chains.carry_all(&mut carried);
    assert_eq!(carried[EdgeId::new(1)], vec![B, A]); // reversed, since it is drawn the other way
    // As given: two twists, and A (on the right) crosses B to turn north at node 3.
    assert_eq!(costs(&t, &walks, &order), (2, 1, 0));
    assert_eq!(costs(&t, &walks, &carried), (0, 1, 0));
    let s = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    assert_eq!(s.cost.total, 0.0);
    assert_eq!(s.exhaustive_parts, 1);
}

/// A node where a third edge brings a line in, or where a line ends, is no pass-through.
#[test]
fn chains_stop_where_lines_join_or_end() {
    let (t, walks) = y();
    let chains = Chains::new(&t, &node_views(&t, &walks));
    assert_eq!(chains, Chains::identity(t.edges.len()));
    // Two edges A,B then A only: A's line set differs, so no chain.
    let t = graph(
        &[(0.0, 0.0), (100.0, 0.0), (200.0, 0.0)],
        &[(0, 1, &[A, B]), (1, 2, &[A])],
    );
    let walks = walks_of(vec![vec![(0, F), (1, F)], vec![(0, F)]]);
    assert_eq!(
        Chains::new(&t, &node_views(&t, &walks)),
        Chains::identity(2)
    );
    // No edges, no walks: nothing to chain.
    let t = graph(&[], &[]);
    assert_eq!(
        Chains::new(&t, &node_views(&t, &IdVec::new())),
        Chains::identity(0)
    );
}

proptest! {
    /// With chains contracted, trying every order finds the same optimum cost as the plain
    /// search, from fewer states; real topologies from street-grid
    /// routes (pass-through nodes are common there) and corridors with unused arms.
    #[test]
    fn chain_contraction_keeps_the_optimum(input in grid_walks(), (ct, cwalks) in corridor()) {
        let (t, walks) =
            crate::topology::build_with_traversals(&input, &crate::topology::Params::default())
                .unwrap();
        for (t, walks) in [(t, walks), (ct, cwalks)] {
            let order = input_order(&t);
            let views = node_views(&t, &walks);
            let chains = Chains::new(&t, &views);
            let plain_states = Chains::identity(t.edges.len()).log10_states(&order);
            prop_assert!(chains.log10_states(&order) <= plain_states + 1e-9);
            prop_assume!(plain_states <= 6.0); // keep the plain search under 10⁶ orders
            let all = ExhaustiveLimit(u64::MAX);
            let plain = solve_inner(&t, &walks, &w(), all, &Annealing::default(), false);
            let chained = solve_inner(&t, &walks, &w(), all, &Annealing::default(), true);
            prop_assert!(
                (plain.cost.total - chained.cost.total).abs() < 1e-9,
                "plain {:?} vs chained {:?}", plain.cost, chained.cost
            );
            prop_assert!(valid(&t, &chained.order));
            prop_assert_eq!(chained.cost, cost(&t, &walks, &chained.order, &w()));
        }
    }
}
