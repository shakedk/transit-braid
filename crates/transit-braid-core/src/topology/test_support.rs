//! Inputs shared by the topology's tests.

use proptest::prelude::*;

use crate::geom::Point;

pub(super) fn routes() -> impl Strategy<Value = Vec<Vec<Point>>> {
    let line = prop::collection::vec((0.0..300.0_f64, 0.0..300.0_f64), 2..6)
        .prop_map(|v| {
            v.into_iter()
                .map(|(x, y)| Point::new(x, y))
                .collect::<Vec<_>>()
        })
        .prop_filter("needs length", |l: &Vec<Point>| {
            l.windows(2).any(|w| w[0].distance(w[1]) > 0.0)
        });
    prop::collection::vec(line, 0..5)
}
