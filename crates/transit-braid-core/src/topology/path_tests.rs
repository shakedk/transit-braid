//! Measuring along a path: the nearest point, the path index, cutting and simplifying.

use proptest::prelude::*;

use super::path::{PathIndex, arc_along, simplify, sub_path};
use crate::geom::Point;

#[test]
fn simplify_keeps_ends_and_real_bends() {
    let p = |x, y| Point::new(x, y);
    let pts = [
        p(0.0, 0.0),
        p(5.0, 0.3),
        p(10.0, 0.0),
        p(15.0, 4.0),
        p(20.0, 0.0),
    ];
    assert_eq!(simplify(&pts, 0.5), vec![pts[0], pts[2], pts[3], pts[4]]);
    assert_eq!(simplify(&pts[..2], 0.5), pts[..2].to_vec());
}

#[test]
fn arc_along_finds_the_nearest_point_even_on_degenerate_paths() {
    let path = [
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
    ];
    assert!((arc_along(&path, Point::new(5.0, 3.0)) - 5.0).abs() < 1e-9);
    assert!((arc_along(&path, Point::new(12.0, 5.0)) - 15.0).abs() < 1e-9);
    assert!(
        arc_along(&path, Point::new(-5.0, 0.0)).abs() < 1e-9,
        "before the start: 0"
    );
    assert!(
        (arc_along(&path, Point::new(10.0, 30.0)) - 20.0).abs() < 1e-9,
        "past the end"
    );
    // A zero-length segment is skipped, not divided by.
    let dup = [
        Point::new(0.0, 0.0),
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
    ];
    assert!((arc_along(&dup, Point::new(4.0, 1.0)) - 4.0).abs() < 1e-9);
    // One point, or none: 0, no panic.
    assert!(arc_along(&path[..1], Point::new(3.0, 3.0)).abs() < 1e-9);
    assert!(arc_along(&[], Point::new(3.0, 3.0)).abs() < 1e-9);
}

#[test]
fn sub_path_cuts_between_two_distances_and_keeps_two_points() {
    let p = Point::new;
    let path = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)];
    assert_eq!(
        sub_path(&path, 2.0, 15.0),
        vec![p(2.0, 0.0), p(10.0, 0.0), p(10.0, 5.0)]
    );
    assert_eq!(sub_path(&path, 0.0, 20.0), path.to_vec());
    assert_eq!(
        sub_path(&path, 12.0, 18.0),
        vec![p(10.0, 2.0), p(10.0, 8.0)]
    );
    // Degenerate input: never fewer than two points, never a panic.
    let still = [p(1.0, 1.0), p(1.0, 1.0)];
    assert_eq!(sub_path(&still, 0.0, 0.0), vec![p(1.0, 1.0), p(1.0, 1.0)]);
    assert_eq!(sub_path(&path, 25.0, 30.0).len(), 2, "past the end");
}

/// A wandering path of `n` steps (some of length zero, some doubling back) and points near
/// it, on it and far from it.
fn path_and_points() -> impl Strategy<Value = (Vec<Point>, Vec<Point>)> {
    let step = (-30.0f64..30.0, -30.0f64..30.0, 0u8..10);
    (
        prop::collection::vec(step, 1..120),
        prop::collection::vec((-200.0f64..1200.0, -200.0f64..1200.0), 1..40),
    )
        .prop_map(|(steps, points)| {
            let mut path = vec![Point::new(100.0, 100.0)];
            for (dx, dy, kind) in steps {
                let last = path[path.len() - 1];
                path.push(match kind {
                    0 => last,                                             // zero length
                    1 => path[path.len() / 2],                             // back on itself
                    _ => Point::new(last.x + dx * 4.0, last.y + dy * 4.0), // on its way
                });
            }
            let mut points: Vec<Point> =
                points.into_iter().map(|(x, y)| Point::new(x, y)).collect();
            points.extend(path.iter().step_by(7).copied()); // on the path's corners
            (path, points)
        })
}

proptest! {
    /// The path index gives exactly `arc_along`'s answer, for any path and point.
    #[test]
    fn the_path_index_finds_what_a_full_scan_finds((path, points) in path_and_points(), cell in 1.0f64..40.0) {
        let index = PathIndex::new(&path, cell);
        for p in points {
            prop_assert_eq!(index.arc(p).to_bits(), arc_along(&path, p).to_bits(), "at {:?}", p);
        }
    }
}
