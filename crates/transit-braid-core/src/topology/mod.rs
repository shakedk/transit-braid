//! Topology: which routes run together where. Routes in meters go in; a line graph comes
//! out ([`Topology`]): edges, each with one centerline and the routes running along it, and
//! nodes where routes join, leave or end; plus each route's walk through it
//! ([`Traversal`]).
//!
//! The build:
//!
//! 0. Short spikes (out and back within a few meters, e.g. a stop pull-in in GTFS shapes)
//!    are removed from each route (`despike`).
//! 1. Every route is sampled every [`Params::step_m`].
//! 2. Each sample gets a **key**: the set of routes running parallel (heading within
//!    [`Params::angle_deg`], either way) within [`Params::merge_m`], itself included, plus
//!    whether the route passes here twice (out and back).
//! 3. Consecutive samples with the same key form a **run**, after a few meters of flicker
//!    between equal keys are smoothed away. Runs with the same set that lie on top of each other
//!    (one's middle within `merge_m` of the other) are one **edge**; its centerline is its
//!    longest run.
//!    3b. An edge is split where one of its routes joins or leaves it while the others
//!    carry on (`split_at_run_ends`).
//! 4. Edge ends closer than `merge_m` become one **node** (their average).
//!
//! Known limits: short runs where the set of routes changes can leave short edges, and a
//! centerline is one member's path, not an average.
//!
//! It validates its input and returns errors instead of panicking, and the work it may do is
//! bounded ([`crate::budget`]).
//!
//! The steps live in their own files: `keys` (steps 0–2, and the flicker smoothing of keys),
//! `grid` (the segment grid the keys look things up in), `assemble` (steps 3–4 and the walks),
//! `split` (step 3b) and `path` (measuring along a path). `incremental` and `matching` rebuild
//! the graph after an edit, computing again only the keys that can change.

mod assemble;
mod grid;
mod incremental;
mod keys;
mod matching;
mod path;
mod split;

pub(crate) use incremental::build_incremental_within;
pub use incremental::{Incremental, KeyCounts, build_incremental, build_incremental_with_ids};
pub use matching::{RouteEdit, route_edit, route_edit_by_ids, usable_ids};
pub(crate) use path::distance_to_segment;

use std::fmt;

use crate::budget::{self, Bounds, Counter, Exceeded, Work};
use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId, NodeId};

use assemble::assemble;
use grid::{Grid, segments};
use keys::{Key, Prepared, key};

/// Which way a route runs along an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// From the edge's `from` node to its `to` node.
    Forward,
    /// From `to` to `from`.
    Backward,
    /// Out and back along the edge; still one slot.
    Both,
}

/// A stretch between two nodes with one fixed set of routes.
#[derive(Debug, Clone, PartialEq)]
pub struct TopoEdge {
    /// The node it starts at.
    pub from: NodeId,
    /// The node it ends at, never equal to `from`.
    pub to: NodeId,
    /// Centerline in meters, from the `from` node to the `to` node (at least 2 points).
    pub path: Vec<Point>,
    /// `(line, direction)`, sorted by line, each line once.
    pub lines: Vec<(LineId, Dir)>,
}

/// The line graph of a set of routes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Topology {
    /// Node positions in meters.
    pub nodes: IdVec<NodeId, Point>,
    /// Edges between nodes.
    pub edges: IdVec<EdgeId, TopoEdge>,
}

/// How one route runs through the graph: its edges in order, each
/// with the way it runs along it (`Forward` or `Backward`, never `Both`). A route that runs
/// out and back along one edge visits it twice, once each way, though the edge lists it once.
pub type Traversal = Vec<(EdgeId, Dir)>;

/// Why [`build`] refused its input.
#[derive(Debug, Clone, PartialEq)]
pub enum TopologyError {
    /// A setting is out of range; the text says which.
    InvalidParams(String),
    /// A coordinate of route `route` is NaN or infinite.
    NotFinite {
        /// Index of the route.
        route: usize,
    },
    /// Route `route` has fewer than two distinct points.
    TooShort {
        /// Index of the route.
        route: usize,
    },
    /// The build needs more than `max` of `work` ([`crate::budget`]).
    TooComplex {
        /// Which work.
        work: Work,
        /// Its bound.
        max: u64,
    },
}

impl From<Exceeded> for TopologyError {
    fn from(e: Exceeded) -> Self {
        Self::TooComplex {
            work: e.work,
            max: e.max,
        }
    }
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParams(why) => write!(f, "invalid topology settings: {why}"),
            Self::NotFinite { route } => {
                write!(f, "route {route}: a coordinate is not a finite number")
            }
            Self::TooShort { route } => {
                write!(f, "route {route}: needs at least two distinct points")
            }
            Self::TooComplex { work, max } => budget::describe(f, *work, *max),
        }
    }
}

impl std::error::Error for TopologyError {}

/// Settings for [`build`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Routes closer than this (meters) share an edge.
    pub merge_m: f64,
    /// Largest heading difference (degrees) that still counts as parallel.
    pub angle_deg: f64,
    /// Distance between samples along each route (meters).
    pub step_m: f64,
}

impl Default for Params {
    /// Routes within 5 m share an edge, headings within 30° are parallel, a sample every 2 m.
    fn default() -> Self {
        Self {
            merge_m: 5.0,
            angle_deg: 30.0,
            step_m: 2.0,
        }
    }
}

impl Params {
    /// Checks every setting is a finite number in range.
    ///
    /// # Errors
    /// [`TopologyError::InvalidParams`] naming the bad setting.
    pub fn validate(&self) -> Result<(), TopologyError> {
        let bad = |why: &str| Err(TopologyError::InvalidParams(why.to_owned()));
        if !(self.merge_m.is_finite() && self.merge_m > 0.0) {
            return bad("merge_m must be a positive number");
        }
        if !(self.step_m.is_finite() && self.step_m > 0.0) {
            return bad("step_m must be a positive number");
        }
        if !(self.angle_deg.is_finite() && self.angle_deg > 0.0 && self.angle_deg < 90.0) {
            return bad("angle_deg must be between 0 and 90");
        }
        Ok(())
    }
}

/// Builds the line graph of `lines` (routes in meters, in a plane). Deterministic: the same
/// input gives the same graph.
///
/// # Errors
/// If a setting is out of range, a coordinate is not finite, or a route has fewer than two
/// distinct points; or the build needs more than [`budget::MAX_NEARBY_CHECKS`] nearby
/// checks or [`budget::MAX_GRAPH_STEPS`] graph steps ([`TopologyError::TooComplex`]).
pub fn build(lines: &[Vec<Point>], params: &Params) -> Result<Topology, TopologyError> {
    build_with_traversals(lines, params).map(|(topology, _)| topology)
}

/// [`build`], plus each route's [`Traversal`]: how it continues through every node, which
/// line ordering needs to count crossings. `traversals[LineId::new(i)]` is route `i`'s; it is
/// empty only for a route on no edge (shorter than about `merge_m`).
///
/// # Errors
/// Same as [`build`].
pub fn build_with_traversals(
    lines: &[Vec<Point>],
    params: &Params,
) -> Result<(Topology, IdVec<LineId, Traversal>), TopologyError> {
    build_within(lines, params, &Bounds::default())
}

/// [`build_with_traversals`] within the work bounds `bounds`.
pub(crate) fn build_within(
    lines: &[Vec<Point>],
    params: &Params,
    bounds: &Bounds,
) -> Result<(Topology, IdVec<LineId, Traversal>), TopologyError> {
    check_input(lines, params)?;
    let prepared: IdVec<LineId, Prepared> =
        lines.iter().map(|l| Prepared::new(l, params)).collect();
    let segs = segments(prepared.iter().map(|p| &p.line[..]));
    let grid = Grid::new(&segs, params.merge_m);
    let min_cos = crate::det::cos(params.angle_deg.to_radians()); // the same bits on every platform
    let mut nearby = Counter::new(Work::NearbyChecks, bounds.nearby_checks);
    let mut keys: IdVec<LineId, Vec<Key>> = IdVec::with_capacity(prepared.len());
    for (line, p) in prepared.iter_enumerated() {
        let mut line_keys = Vec::with_capacity(p.samples.len());
        for s in &p.samples {
            let (k, n) = key(line, s, &segs, &grid, params.merge_m, min_cos);
            nearby.add(u64::from(n))?;
            line_keys.push(k);
        }
        keys.push(line_keys);
    }
    let prepared: IdVec<LineId, &Prepared> = prepared.iter().collect();
    let keys: IdVec<LineId, Vec<&Key>> = keys.iter().map(|l| l.iter().collect()).collect();
    assemble(&prepared, &keys, params, bounds)
}

/// Settings in range, every point finite, every route with two distinct points.
fn check_input(lines: &[Vec<Point>], params: &Params) -> Result<(), TopologyError> {
    params.validate()?;
    for (route, pts) in lines.iter().enumerate() {
        if pts.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err(TopologyError::NotFinite { route });
        }
        if pts.windows(2).all(|w| w[0].distance(w[1]) == 0.0) {
            return Err(TopologyError::TooShort { route });
        }
    }
    Ok(())
}

#[cfg(test)]
mod build_tests;
#[cfg(test)]
mod grid_tests;
#[cfg(test)]
mod incremental_tests;
#[cfg(test)]
mod keys_tests;
#[cfg(test)]
mod path_tests;
#[cfg(test)]
mod split_tests;
#[cfg(test)]
mod test_support;
