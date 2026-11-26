//! The input limits ([`MAX_ROUTES`], [`MAX_POINTS`], [`MAX_EXTENT_M`]…) and the checks every
//! call makes before any work ([`check_call`]), so bad or oversized input is refused at once with
//! a clear error, and a refused call changes nothing.

use super::{MAX_LATITUDE, PipelineError, Projection, Settings};
use crate::budget::{Bounds, Counter, Work};
use crate::geom::Point;
use crate::topology;

/// The most routes one call takes: a city's busiest routes; more is an error. 70 is the most
/// that keeps an edit in a session under 200 ms on the densest city we test (Dublin's 70 busiest
/// bus patterns: 196 ms in Chromium; 100 would take 295 ms).
pub const MAX_ROUTES: usize = 70;

/// The most stations one call takes: a city's 70 busiest routes have a few thousand stops at
/// most; more is an error, so a mistaken input can't make a call slow.
pub const MAX_STATIONS: usize = 5_000;

/// The most points one call takes, all routes together: a bound on memory. A city's 70
/// busiest routes have about 20 000.
pub const MAX_POINTS: usize = 1_000_000;

/// The farthest the routes may spread, east-west and north-south, in meters. Braid draws
/// city-scale networks (a city's busiest routes span about 40 km), and its local projection
/// ([`Projection`]) stays within a few percent of true scale across this.
pub const MAX_EXTENT_M: f64 = 200_000.0;

/// The most route length one call takes, all routes together, in meters. The topology
/// samples every route every 2 m, so this bounds its memory and time; a city's 70 busiest
/// routes are about 1 400 km long together.
pub const MAX_LENGTH_M: f64 = 2_000_000.0;

/// Neighboring points of a route farther apart than this in longitude (degrees) jump across
/// the date line: GeoJSON writes such a line from 179.9 to -179.9, which reads as a line
/// almost around the globe.
const MAX_LONGITUDE_STEP_DEG: f64 = 180.0;
/// At most the bound of station checks, stations × the graph's centerline segments: every
/// station looks for its nearest edge along every segment, at every zoom. Checked before any
/// station is snapped, and before a new graph is kept.
pub(super) fn check_station_work(
    topo: &topology::Topology,
    stations: usize,
    bounds: &Bounds,
) -> Result<(), PipelineError> {
    let segments: u64 = topo
        .edges
        .iter()
        .map(|e| e.path.len().saturating_sub(1) as u64)
        .sum();
    Counter::new(Work::StationChecks, bounds.station_checks)
        .add(segments.saturating_mul(stations as u64))
        .map_err(PipelineError::from)
}

/// The checks every call makes before any work, so a refused call changes nothing.
pub(super) fn check_call(
    lines: &[Vec<Point>],
    stations: &[Point],
    groups: &[u32],
    settings: &Settings,
) -> Result<(), PipelineError> {
    settings.validate()?;
    if !groups.is_empty() && groups.len() != stations.len() {
        return Err(PipelineError::StationGroups {
            stations: stations.len(),
            groups: groups.len(),
        });
    }
    if lines.len() > MAX_ROUTES {
        return Err(PipelineError::TooMany { count: lines.len() });
    }
    if stations.len() > MAX_STATIONS {
        return Err(PipelineError::TooManyStations {
            count: stations.len(),
        });
    }
    let points: usize = lines.iter().map(Vec::len).sum();
    if points > MAX_POINTS {
        return Err(PipelineError::TooManyPoints { count: points });
    }
    check_coordinates(lines)?;
    check_size(lines)?;
    for (station, p) in stations.iter().enumerate() {
        if !on_the_map(*p) {
            return Err(PipelineError::BadStation { station });
        }
    }
    Ok(())
}

/// A point with a finite longitude in `[-180, 180]` and latitude in `[-MAX_LATITUDE, MAX_LATITUDE]`.
fn on_the_map(p: Point) -> bool {
    p.x.is_finite()
        && (-180.0..=180.0).contains(&p.x)
        && p.y.is_finite()
        && (-MAX_LATITUDE..=MAX_LATITUDE).contains(&p.y)
}

/// Every point finite and on the Web Mercator map, none more than 180° of longitude from the
/// point before; every route with two distinct points.
fn check_coordinates(lines: &[Vec<Point>]) -> Result<(), PipelineError> {
    for (i, line) in lines.iter().enumerate() {
        for (j, p) in line.iter().enumerate() {
            if !on_the_map(*p) {
                return Err(PipelineError::BadCoordinate { route: i, point: j });
            }
            if let Some(before) = j.checked_sub(1).and_then(|b| line.get(b))
                && (p.x - before.x).abs() > MAX_LONGITUDE_STEP_DEG
            {
                return Err(PipelineError::CrossesAntimeridian { route: i, point: j });
            }
        }
        if !line.iter().any(|p| line.first().is_some_and(|f| p != f)) {
            return Err(PipelineError::TooShort { route: i });
        }
    }
    Ok(())
}

/// The routes' spread and length, measured in the pipeline's own projection: at most
/// [`MAX_EXTENT_M`] each way and [`MAX_LENGTH_M`] together. Expects valid coordinates
/// ([`check_coordinates`]). Plain arithmetic and square roots only, so native and WebAssembly
/// agree.
pub(super) fn check_size(lines: &[Vec<Point>]) -> Result<(), PipelineError> {
    let proj = Projection::around(lines);
    let mut min = Point::new(f64::INFINITY, f64::INFINITY);
    let mut max = Point::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut length = 0.0;
    for line in lines {
        let mut before: Option<Point> = None;
        for &p in line {
            let q = proj.forward(p);
            min = Point::new(min.x.min(q.x), min.y.min(q.y));
            max = Point::new(max.x.max(q.x), max.y.max(q.y));
            if let Some(b) = before {
                length += b.distance(q);
            }
            before = Some(q);
        }
    }
    if min.x > max.x {
        return Ok(()); // no points
    }
    let (width, height) = (max.x - min.x, max.y - min.y);
    if width > MAX_EXTENT_M || height > MAX_EXTENT_M {
        return Err(PipelineError::TooWide {
            width_km: width / 1000.0,
            height_km: height / 1000.0,
        });
    }
    if length > MAX_LENGTH_M {
        return Err(PipelineError::TooLong {
            km: length / 1000.0,
        });
    }
    Ok(())
}
