//! Settings and inputs shared by the pipeline's tests.

use proptest::prelude::*;

use super::{Projection, Scale, Settings};
use crate::geom::Point;

pub(super) const ONE_PX_PER_M: Settings = Settings {
    scale: Scale::MetersPerPixel(1.0),
    line_width_px: 4.0,
    spacing_px: 1.0,
};

/// A street running east from `(lng, lat)` for about `m` meters, as `[lng, lat]` points.
pub(super) fn east(lng: f64, lat: f64, m: f64) -> Vec<Point> {
    let proj = Projection::at(Point::new(lng, lat));
    (0..=4)
        .map(|k| proj.inverse(Point::new(m * f64::from(k) / 4.0, 0.0)))
        .collect()
}

pub(super) fn assert_near(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} is not within {tol} of {b}");
}

/// Up to 6 routes of 2–4 points within 300 m of a city center anywhere on the map.
pub(super) fn city_lines() -> impl Strategy<Value = Vec<Vec<Point>>> {
    let center = (-170.0f64..170.0, -80.0f64..80.0).prop_map(|(x, y)| Point::new(x, y));
    let line = prop::collection::vec((-300.0f64..300.0, -300.0f64..300.0), 2..5);
    (center, prop::collection::vec(line, 1..7)).prop_map(|(c, ls)| {
        let proj = Projection::at(c);
        ls.into_iter()
            .map(|l| {
                l.into_iter()
                    .map(|(x, y)| proj.inverse(Point::new(x, y)))
                    .collect()
            })
            .collect()
    })
}

pub(super) fn at_zoom(z: f64) -> Settings {
    Settings {
        scale: Scale::Zoom(z),
        line_width_px: 4.0,
        spacing_px: 1.0,
    }
}
