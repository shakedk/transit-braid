//! The segment grid: the cells a segment crosses, and an edit's reach.

use proptest::prelude::*;

use super::grid::{Grid, Reach, cells_crossed, segments};
use super::{Incremental, Params, build_incremental, build_with_traversals};
use crate::geom::Point;

/// A long diagonal marks a strip of cells along itself, not its bounding box (a 5 km
/// diagonal: about 10 000 cells instead of 500 000), and a full build marks none.
#[test]
fn a_long_diagonal_reaches_a_strip_not_its_box() {
    let line = [Point::new(0.0, 0.0), Point::new(3_535.0, 3_535.0)];
    let segs = segments(std::iter::once(&line[..]));
    let reach = Reach::new(segs.iter(), 5.0);
    assert!(reach.cells.len() < 20_000, "{} cells", reach.cells.len());
    assert!(reach.near(Point::new(1_000.0, 1_002.0)));
    assert!(
        !reach.near(Point::new(3_000.0, 500.0)),
        "inside the box, far from the line"
    );
    assert!(!Reach::new(std::iter::empty(), 5.0).near(Point::new(0.0, 0.0)));
    // An edit that adds such a diagonal to a kept street stays incremental and exact.
    let street = vec![Point::new(0.0, 100.0), Point::new(3_000.0, 100.0)];
    let mut state = Incremental::new();
    build_incremental(
        &mut state,
        &[street.clone(), street.clone()],
        &Params::default(),
    )
    .unwrap();
    let edited = vec![street.clone(), street, line.to_vec()];
    assert_eq!(
        build_incremental(&mut state, &edited, &Params::default()).unwrap(),
        build_with_traversals(&edited, &Params::default()).unwrap()
    );
    let keys = state.last_keys();
    assert!(keys.computed < keys.total, "{keys:?}");
}

proptest! {
    /// Every point of a segment lies in a cell `cells_crossed` reports.
    #[test]
    fn a_segment_lies_in_the_cells_it_crosses(
        a in (-5000.0f64..5000.0, -5000.0f64..5000.0),
        b in (-5000.0f64..5000.0, -5000.0f64..5000.0),
        flat in 0u8..3,
        cell in 0.5f64..20.0,
    ) {
        let a = Point::new(a.0, a.1);
        // Some segments exactly level or upright, the hard cases for a column walk.
        let b = match flat {
            0 => Point::new(b.0, a.y),
            1 => Point::new(a.x, b.1),
            _ => Point::new(b.0, b.1),
        };
        let mut cells = std::collections::HashSet::new();
        cells_crossed(a, b, cell, |k| { cells.insert(k); });
        for i in 0..=200 {
            let t = f64::from(i) / 200.0;
            let q = Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y));
            let k = ((q.x / cell).floor() as i64, (q.y / cell).floor() as i64);
            prop_assert!(cells.contains(&k), "{:?} at t = {} not in a reported cell", q, t);
        }
    }

    /// `Reach` marks exactly the cells where `Grid::near` finds one of its segments, for
    /// any segments (long or short, level, upright or diagonal) and any point.
    #[test]
    fn reach_is_where_the_grid_finds_the_segments(
        ends in prop::collection::vec(((-3000.0f64..3000.0, -3000.0f64..3000.0), (-60.0f64..60.0, -60.0f64..60.0), 0u8..4), 1..4),
        points in prop::collection::vec((-3100.0f64..3100.0, -3100.0f64..3100.0), 1..60),
        cell in 0.5f64..20.0,
    ) {
        let lines: Vec<Vec<Point>> = ends
            .iter()
            .map(|&((x, y), (dx, dy), kind)| {
                let a = Point::new(x, y);
                let b = match kind {
                    0 => Point::new(x + dx * 50.0, y), // long and level
                    1 => Point::new(x, y + dy * 50.0), // long and upright
                    2 => Point::new(x + dx * 50.0, y + dy * 50.0), // long diagonal
                    _ => Point::new(x + dx, y + dy),
                };
                vec![a, b]
            })
            .collect();
        let segs = segments(lines.iter().map(|l| &l[..]));
        let grid = Grid::new(&segs, cell);
        let reach = Reach::new(segs.iter(), cell);
        // Points anywhere, on the segments and just beside them.
        let mut probe: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        for g in &segs {
            for i in 0..=20 {
                let t = f64::from(i) / 20.0;
                let on = Point::new(g.a.x + t * (g.b.x - g.a.x), g.a.y + t * (g.b.y - g.a.y));
                probe.extend([on, Point::new(on.x + 1.9 * cell, on.y - 1.9 * cell), Point::new(on.x - 2.1 * cell, on.y)]);
            }
        }
        for p in probe {
            prop_assert_eq!(reach.near(p), !grid.near(p).is_empty(), "at {:?}", p);
        }
    }
}
