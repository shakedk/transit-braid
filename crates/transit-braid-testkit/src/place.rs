//! Where a fixture's lines and stops go: in `[lng, lat]` for the pipeline (as `sideBySide()` takes
//! them), or in meters for measuring, plus the line widths the checks draw with.

use std::collections::HashMap;

use transit_braid_core::geom::Point;
use transit_braid_core::pipeline;
use transit_braid_core::topology;

use crate::{Fixture, Units};

/// `sideBySide()`'s default line width in pixels; the checks draw the pipeline's lines with it.
pub const LINE_WIDTH_PX: f64 = 4.0;
/// `sideBySide()`'s default gap between lines in pixels.
pub const SPACING_PX: f64 = 1.0;

/// Meters per degree used to place hand-made fixtures around 0°, 0°, on both axes: Web Mercator's
/// own scale at the equator (6 378 137 m × π / 180), so the pipeline's projection gives back the
/// fixture's meters exactly as drawn.
pub const PLACE_M_PER_DEG_LNG: f64 = 111_319.490_793_273_57;
/// The same on the latitude axis.
pub const PLACE_M_PER_DEG_LAT: f64 = PLACE_M_PER_DEG_LNG;

/// A fixture's lines in `[lng, lat]`, as the pipeline (and `sideBySide()`) takes them: degrees
/// as they are; meters placed around 0°, 0° the same way for every check.
#[must_use]
pub fn lng_lat_lines(fixture: &Fixture) -> Vec<Vec<Point>> {
    fixture
        .lines
        .iter()
        .map(|l| {
            l.points
                .iter()
                .map(|&p| match fixture.units {
                    Units::Degrees => p,
                    Units::Meters => {
                        Point::new(p.x / PLACE_M_PER_DEG_LNG, p.y / PLACE_M_PER_DEG_LAT)
                    }
                })
                .collect()
        })
        .collect()
}

/// Stations for a fixture without stops: one in the middle of every edge of the graph the
/// pipeline builds, in `[lng, lat]`. Q8 uses them.
#[must_use]
pub fn midpoint_stations(fixture: &Fixture) -> Vec<Point> {
    let lines = lng_lat_lines(fixture);
    let proj = pipeline::Projection::around(&lines);
    let meters: Vec<Vec<Point>> = lines
        .iter()
        .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
        .collect();
    let Ok(topo) = topology::build(&meters, &topology::Params::default()) else {
        return Vec::new();
    };
    topo.edges
        .iter()
        .filter(|e| !e.lines.is_empty())
        .map(|e| proj.inverse(midpoint(&e.path)))
        .collect()
}

/// The stations for a fixture, in `[lng, lat]`, with their name groups: the fixture's
/// own stops (`braid.stations`, placed like its lines; same-name stops share a number from 1, in
/// order of first appearance, unnamed ones get 0), or, for a fixture without stops, one in the
/// middle of every edge ([`midpoint_stations`], no names). Q8 and the gallery's bars use them.
#[must_use]
pub fn lab_stations(fixture: &Fixture) -> (Vec<Point>, Vec<u32>) {
    if fixture.stations.is_empty() {
        let at = midpoint_stations(fixture);
        let groups = vec![0; at.len()];
        return (at, groups);
    }
    let mut numbers: HashMap<&str, u32> = HashMap::new();
    let mut at = Vec::with_capacity(fixture.stations.len());
    let mut groups = Vec::with_capacity(fixture.stations.len());
    for s in &fixture.stations {
        at.push(match fixture.units {
            Units::Degrees => s.at,
            Units::Meters => Point::new(s.at.x / PLACE_M_PER_DEG_LNG, s.at.y / PLACE_M_PER_DEG_LAT),
        });
        groups.push(s.name.as_deref().map_or(0, |n| {
            let next = u32::try_from(numbers.len() + 1).unwrap_or(u32::MAX);
            *numbers.entry(n).or_insert(next)
        }));
    }
    (at, groups)
}

/// The point halfway along a polyline (by length); its first point if it has no length.
pub fn midpoint(line: &[Point]) -> Point {
    let total: f64 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total / 2.0;
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        if len > 0.0 && left <= len {
            let t = left / len;
            return Point::new(
                w[0].x + t * (w[1].x - w[0].x),
                w[0].y + t * (w[1].y - w[0].y),
            );
        }
        left -= len;
    }
    line[0]
}

/// The latitude every point of a fixture is projected with: its first line's first point.
/// One value per fixture, so all its lines (and its graph) share one projection.
pub fn reference_latitude(fixture: &Fixture) -> f64 {
    fixture
        .lines
        .first()
        .and_then(|l| l.points.first())
        .map_or(0.0, |p| p.y)
}

/// Degrees are drawn with a simple local projection around `lat0` (degrees), good enough at
/// city scale. All points of one fixture must use the same `lat0`, or lines drift apart.
pub fn to_meters(points: &[Point], units: Units, lat0: f64) -> Vec<Point> {
    match units {
        Units::Meters => points.to_vec(),
        Units::Degrees => {
            let lat0 = lat0.to_radians();
            points
                .iter()
                .map(|p| Point::new(p.x * 111_320.0 * lat0.cos(), p.y * 110_540.0))
                .collect()
        }
    }
}

/// Inverse of [`to_meters`] for one point.
pub fn from_meters(p: Point, units: Units, lat0: f64) -> Point {
    match units {
        Units::Meters => p,
        Units::Degrees => Point::new(p.x / (111_320.0 * lat0.to_radians().cos()), p.y / 110_540.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midpoint_is_halfway_along_the_line() {
        let l = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 30.0),
        ];
        assert_eq!(midpoint(&l), Point::new(10.0, 10.0));
        assert_eq!(
            midpoint(&[Point::new(0.0, 0.0), Point::new(4.0, 0.0)]),
            Point::new(2.0, 0.0)
        );
        // Not starting at the origin.
        let up = [Point::new(5.0, 10.0), Point::new(5.0, 30.0)];
        assert_eq!(midpoint(&up), Point::new(5.0, 20.0));
        // No length at all: the first point.
        assert_eq!(
            midpoint(&[Point::new(3.0, 3.0), Point::new(3.0, 3.0)]),
            Point::new(3.0, 3.0)
        );
        // A repeated point before the middle is skipped.
        let r = [
            Point::new(0.0, 0.0),
            Point::new(0.0, 0.0),
            Point::new(8.0, 0.0),
        ];
        assert_eq!(midpoint(&r), Point::new(4.0, 0.0));
    }
}
