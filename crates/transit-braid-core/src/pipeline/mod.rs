//! The whole pipeline: routes in `[lng, lat]` → one side-by-side route each.
//!
//! 1. Check the input, then project it to meters ([`Projection`]): a local projection around the
//!    middle of the input, true ground meters at city scale, the same bits on every platform.
//! 2. Topology: the line graph and each route's walk through it
//!    ([`crate::topology::build_with_traversals`]).
//! 3. Ordering ([`crate::ordering::solve`]): each line's side of every edge, with as few
//!    crossings and separations as the search finds.
//! 4. Geometry ([`crate::geometry`]): each line in its slot along every edge it runs, one pitch
//!    from its neighbors, joined smoothly at the nodes; and the station bars.
//! 5. Project back to `[lng, lat]`.
//!
//! A [`Cache`] keeps steps 1–3 between calls: a zoom change reruns only step 4, and an edit
//! rebuilds the graph incrementally and orders it starting from the order before.
//!
//! The parts live in their own files: `settings` (the map scale and line widths), `limits` (the
//! input limits and the checks every call makes first), `error` (why a call was refused),
//! `projection` (`[lng, lat]` ↔ meters), `cache` (what a call keeps for the next one, and the
//! cached entry points) and `cap` (the lane cap).

mod cache;
mod cap;
mod error;
mod limits;
mod projection;
mod settings;

pub use cache::{Cache, StageRuns, side_by_side_cached, side_by_side_cached_with_ids};
pub use cap::{Capped, side_by_side_capped};
pub use error::{PipelineError, Subject};
pub use limits::{MAX_EXTENT_M, MAX_LENGTH_M, MAX_POINTS, MAX_ROUTES, MAX_STATIONS};
pub use projection::{MAX_LATITUDE, Projection};
pub use settings::{Scale, Settings};

use crate::geom::Point;

/// The pipeline's output with stations ([`side_by_side_with_stations`]), all `[lng, lat]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Drawn {
    /// One route per input route, in order and direction.
    pub lines: Vec<Vec<Point>>,
    /// One bar per station, in order: its two ends across the bundle, or None for a station
    /// farther than [`crate::geometry::STATION_SNAP_M`] from every route.
    pub bars: Vec<Option<(Point, Point)>>,
}

/// Runs the whole pipeline: `lines` are routes of `[lng, lat]` points (`x` = longitude,
/// `y` = latitude, degrees). Returns one route per input route, in the same order and
/// direction, shifted into its slot. No routes give no routes.
///
/// # Errors
/// [`PipelineError`] for too many routes, bad coordinates, a route with fewer than two
/// distinct points, or bad settings; and for more than [`MAX_POINTS`] points, a route
/// that crosses the date line, or routes that spread wider than [`MAX_EXTENT_M`] or are
/// longer than [`MAX_LENGTH_M`] together.
pub fn side_by_side(
    lines: &[Vec<Point>],
    settings: &Settings,
) -> Result<Vec<Vec<Point>>, PipelineError> {
    side_by_side_with_stations(lines, &[], settings).map(|d| d.lines)
}

/// [`side_by_side`] with stations: `stations` are `[lng, lat]` points; each gets a bar across
/// the bundle where it is ([`crate::geometry::station_bars`]), or none when no route passes within
/// [`crate::geometry::STATION_SNAP_M`].
///
/// # Errors
/// As [`side_by_side`], plus [`PipelineError::TooManyStations`] and
/// [`PipelineError::BadStation`].
pub fn side_by_side_with_stations(
    lines: &[Vec<Point>],
    stations: &[Point],
    settings: &Settings,
) -> Result<Drawn, PipelineError> {
    side_by_side_with_named_stations(lines, stations, &[], settings)
}

/// [`side_by_side_with_stations`] where stations with the same name share a bar: `groups`
/// gives each station's name as a number (equal numbers, equal names; 0 or an empty `groups`, no
/// name), and same-name stations within [`crate::geometry::STATION_MERGE_M`] of each other on the
/// same edge get their bar at their average place
/// ([`crate::geometry::merge_named_stations_on_edges`]; on two edges, such as a boulevard's two
/// carriageways, each keeps its own bar). Still one bar per station, in order.
///
/// # Errors
/// As [`side_by_side_with_stations`], plus [`PipelineError::StationGroups`] when `groups` is
/// neither empty nor one per station.
pub fn side_by_side_with_named_stations(
    lines: &[Vec<Point>],
    stations: &[Point],
    groups: &[u32],
    settings: &Settings,
) -> Result<Drawn, PipelineError> {
    side_by_side_cached(&mut Cache::new(), lines, stations, groups, settings)
}

#[cfg(test)]
mod cache_tests;
#[cfg(test)]
mod cap_tests;
#[cfg(test)]
mod error_tests;
#[cfg(test)]
mod limits_tests;
#[cfg(test)]
mod side_by_side_tests;
#[cfg(test)]
mod test_support;
