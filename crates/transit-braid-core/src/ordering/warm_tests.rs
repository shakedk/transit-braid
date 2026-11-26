//! The warm-started order after an edit.

use proptest::prelude::*;

use super::test_support::*;
use super::*;
use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId};
use crate::topology::Dir;

/// Routes in meters from point lists.
fn polys(lines: &[&[(f64, f64)]]) -> Vec<Vec<Point>> {
    lines
        .iter()
        .map(|l| l.iter().map(|&(x, y)| Point::new(x, y)).collect())
        .collect()
}

/// The real topology and its cold order.
fn solved(lines: &[Vec<Point>]) -> (Topology, IdVec<LineId, Traversal>, Order) {
    let (t, walks) =
        crate::topology::build_with_traversals(lines, &crate::topology::Params::default()).unwrap();
    let order = solve(&t, &walks, &w(), ExhaustiveLimit::default()).order;
    (t, walks, order)
}

#[test]
fn a_warm_start_with_no_edit_keeps_every_order_and_solves_nothing() {
    let (t, walks) = y();
    let cold = solve(&t, &walks, &w(), ExhaustiveLimit::default());
    let edit = crate::topology::RouteEdit {
        old_to_new: [0, 1, 2]
            .map(|l| Some(LineId::new(l)))
            .into_iter()
            .collect(),
        changed: vec![],
    };
    let warm = solve_warm(
        &t,
        &walks,
        &w(),
        ExhaustiveLimit::default(),
        &Previous {
            topo: &t,
            order: &cold.order,
            edit: &edit,
        },
    );
    assert_eq!(warm.solution.order, cold.order);
    assert_eq!(warm.solution.cost, cold.cost);
    assert_eq!((warm.kept_parts, warm.solved_parts), (1, 0));
}

/// Two separate corridors are two parts; an edit in one solves
/// that part again and keeps the other's order untouched (test counts).
#[test]
fn an_edit_solves_only_the_part_it_touches() {
    // Each corridor: three lines share a street, then split three ways.
    let corridor = |y0: f64| -> Vec<Vec<Point>> {
        polys(&[
            &[(0.0, y0), (300.0, y0), (500.0, y0 + 150.0)],
            &[(0.0, y0 + 1.0), (300.0, y0 + 1.0), (500.0, y0 + 1.0)],
            &[(0.0, y0 - 1.0), (300.0, y0 - 1.0), (500.0, y0 - 150.0)],
        ])
    };
    let lines: Vec<Vec<Point>> = corridor(0.0).into_iter().chain(corridor(2000.0)).collect();
    let (t0, _, order0) = solved(&lines);
    let mut edited = lines.clone();
    edited[1][1].y += 2.0; // a line of the first corridor bends a little
    let (t1, walks1, _) = solved(&edited);
    let edit = crate::topology::route_edit(&lines, &edited);
    assert_eq!(edit.changed, vec![LineId::new(1)]);
    let warm = solve_warm(
        &t1,
        &walks1,
        &w(),
        ExhaustiveLimit::default(),
        &Previous {
            topo: &t0,
            order: &order0,
            edit: &edit,
        },
    );
    assert_eq!((warm.kept_parts, warm.solved_parts), (1, 1));
    assert!(valid(&t1, &warm.solution.order));
    // The far corridor's shared street keeps its order exactly.
    let far = |t: &Topology| {
        t.edges
            .iter()
            .position(|e| e.lines.len() == 3 && e.path[0].y > 1000.0)
            .map(EdgeId::new)
            .unwrap()
    };
    assert_eq!(warm.solution.order[far(&t1)], order0[far(&t0)]);
}

#[test]
fn a_warm_start_from_an_unrelated_or_empty_graph_is_still_valid() {
    let (t, walks) = x();
    let (old, old_walks) = y();
    let old_order = solve(&old, &old_walks, &w(), ExhaustiveLimit::default()).order;
    for (prev, order, edit) in [
        (
            &old,
            &old_order,
            crate::topology::RouteEdit {
                old_to_new: [Some(3), None, Some(0)]
                    .map(|l| l.map(LineId::new))
                    .into_iter()
                    .collect(),
                changed: vec![LineId::new(1), LineId::new(2)],
            },
        ),
        (
            &Topology::default(),
            &Order::new(),
            crate::topology::RouteEdit {
                old_to_new: IdVec::new(),
                changed: (0..4).map(LineId::new).collect(),
            },
        ),
    ] {
        let warm = solve_warm(
            &t,
            &walks,
            &w(),
            ExhaustiveLimit::default(),
            &Previous {
                topo: prev,
                order,
                edit: &edit,
            },
        );
        assert!(valid(&t, &warm.solution.order), "{:?}", warm.solution.order);
        assert_eq!(
            warm.solution.cost,
            cost(&t, &walks, &warm.solution.order, &w())
        );
    }
}

proptest! {
    /// After any edit of routes along a street grid, the warm order is a valid
    /// order costing what `cost` says, the same every time, and every edge the edit did not
    /// touch (an exact twin from before carrying no changed route) keeps its order.
    #[test]
    fn warm_orders_are_valid_and_keep_untouched_edges(
        input in grid_walks(),
        which in any::<usize>(),
        change in prop_oneof![Just(0_u8), Just(1), Just(2)],
        extra in grid_walks(),
    ) {
        let (t0, _, order0) = solved(&input);
        let mut edited = input.clone();
        let k = which % input.len();
        match change {
            0 => edited[k][0].x += 3.0,
            1 if input.len() > 1 => {
                edited.remove(k);
            }
            _ => edited.insert(k, extra[0].clone()),
        }
        let (t1, walks1, _) = solved(&edited);
        let edit = crate::topology::route_edit(&input, &edited);
        let prev = Previous { topo: &t0, order: &order0, edit: &edit };
        let warm = solve_warm(&t1, &walks1, &w(), ExhaustiveLimit::default(), &prev);
        prop_assert!(valid(&t1, &warm.solution.order));
        prop_assert_eq!(warm.solution.cost, cost(&t1, &walks1, &warm.solution.order, &w()));
        prop_assert_eq!(&warm, &solve_warm(&t1, &walks1, &w(), ExhaustiveLimit::default(), &prev));
        let to_new = |l: LineId| edit.old_to_new.get(l).copied().flatten();
        for (e, edge) in t1.edges.iter_enumerated() {
            if edge.lines.iter().any(|(l, _)| edit.changed.contains(l)) {
                continue;
            }
            for (j, old) in t0.edges.iter_enumerated() {
                let lines: Option<Vec<(LineId, Dir)>> =
                    old.lines.iter().map(|&(l, d)| to_new(l).map(|n| (n, d))).collect();
                if old.path == edge.path && lines.as_ref() == Some(&edge.lines) {
                    let was: Vec<LineId> = order0[j].iter().filter_map(|&l| to_new(l)).collect();
                    prop_assert_eq!(&warm.solution.order[e], &was, "edge {} changed", e);
                }
            }
        }
    }
}
