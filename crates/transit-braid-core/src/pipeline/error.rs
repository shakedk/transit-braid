//! Why a call was refused ([`PipelineError`]), and the topology's errors as the pipeline's.

use std::fmt;

use super::{MAX_EXTENT_M, MAX_LATITUDE, MAX_LENGTH_M, MAX_POINTS, MAX_ROUTES, MAX_STATIONS};
use crate::budget::{self, Exceeded, Work};
use crate::topology::TopologyError;

/// Why [`side_by_side`](super::side_by_side) refused its input. Routes are counted from 0, in
/// input order.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineError {
    /// More than [`MAX_ROUTES`] routes.
    TooMany {
        /// How many were given.
        count: usize,
    },
    /// A coordinate is not a finite number, or outside `[-180, 180]` × `[-MAX_LATITUDE, MAX_LATITUDE]`.
    BadCoordinate {
        /// Index of the route.
        route: usize,
        /// Index of the point in that route.
        point: usize,
    },
    /// The route has fewer than two distinct points.
    TooShort {
        /// Index of the route.
        route: usize,
    },
    /// A setting is out of range; the text says which.
    InvalidSettings(String),
    /// Station `station`'s position is not a finite number, or outside
    /// `[-180, 180]` × `[-MAX_LATITUDE, MAX_LATITUDE]`.
    BadStation {
        /// Index of the station.
        station: usize,
    },
    /// More than [`MAX_STATIONS`] stations.
    TooManyStations {
        /// How many were given.
        count: usize,
    },
    /// More than [`MAX_POINTS`] points, all routes together.
    TooManyPoints {
        /// How many were given.
        count: usize,
    },
    /// Point `point` of route `route` is more than 180° of longitude from the point before:
    /// the route crosses the date line, which the pipeline doesn't draw.
    CrossesAntimeridian {
        /// Index of the route.
        route: usize,
        /// Index of the point after the jump.
        point: usize,
    },
    /// The routes spread farther than [`MAX_EXTENT_M`] east-west or north-south.
    TooWide {
        /// East-west, in kilometers.
        width_km: f64,
        /// North-south, in kilometers.
        height_km: f64,
    },
    /// The routes are longer than [`MAX_LENGTH_M`] together.
    TooLong {
        /// Their length together, in kilometers.
        km: f64,
    },
    /// Station groups (names) given, but not one per station.
    StationGroups {
        /// How many stations were given.
        stations: usize,
        /// How many groups were given.
        groups: usize,
    },
    /// The call needs more than `max` of `work` ([`crate::budget`]): valid input, but too
    /// much work to draw.
    TooComplex {
        /// Which work.
        work: Work,
        /// Its bound.
        max: u64,
    },
}

/// What a [`PipelineError`] is about: one route (and maybe one of its points), one station,
/// or the call as a whole. Indices count from 0, in input order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    /// The call as a whole: its size, spread or settings.
    Call,
    /// Route `route`, and point `point` of it when one point is at fault.
    Route {
        /// Index of the route.
        route: usize,
        /// Index of the point in that route, when one point is at fault.
        point: Option<usize>,
    },
    /// Station `station`.
    Station {
        /// Index of the station.
        station: usize,
    },
}

impl PipelineError {
    /// What the error is about ([`Subject`]); the [`Display`](fmt::Display) text names it first.
    #[must_use]
    pub fn subject(&self) -> Subject {
        match *self {
            Self::BadCoordinate { route, point } | Self::CrossesAntimeridian { route, point } => {
                Subject::Route {
                    route,
                    point: Some(point),
                }
            }
            Self::TooShort { route } => Subject::Route { route, point: None },
            Self::BadStation { station } => Subject::Station { station },
            _ => Subject::Call,
        }
    }

    /// Why, without naming the subject: the [`Display`](fmt::Display) text after `route 3,
    /// point 4: ` (or `station 2: `), so a caller can name the route or station its own way.
    #[must_use]
    pub fn reason(&self) -> String {
        Reason(self).to_string()
    }
}

/// [`PipelineError::reason`]'s text.
struct Reason<'a>(&'a PipelineError);

impl fmt::Display for Reason<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            PipelineError::TooMany { count } => write!(
                f,
                "too many routes: {count} given, at most {MAX_ROUTES} allowed"
            ),
            PipelineError::BadCoordinate { .. } | PipelineError::BadStation { .. } => write!(
                f,
                "needs a finite longitude from -180 to 180 and latitude from -{MAX_LATITUDE:.2} to \
                 {MAX_LATITUDE:.2}"
            ),
            PipelineError::TooShort { .. } => write!(f, "needs at least two distinct points"),
            PipelineError::InvalidSettings(why) => write!(f, "invalid settings: {why}"),
            PipelineError::TooManyStations { count } => write!(
                f,
                "too many stations: {count} given, at most {MAX_STATIONS} allowed"
            ),
            PipelineError::TooManyPoints { count } => write!(
                f,
                "too many points: {count} given, at most {MAX_POINTS} allowed (all routes \
                 together)"
            ),
            PipelineError::CrossesAntimeridian { .. } => write!(
                f,
                "more than 180° of longitude from the point before, so it crosses the date line; \
                 split the route there"
            ),
            PipelineError::TooWide {
                width_km,
                height_km,
            } => write!(
                f,
                "the routes spread {width_km:.1} km east-west and {height_km:.1} km \
                 north-south; at most {:.0} km each way",
                MAX_EXTENT_M / 1000.0
            ),
            PipelineError::TooLong { km } => write!(
                f,
                "the routes are {km:.1} km long together; at most {:.0} km",
                MAX_LENGTH_M / 1000.0
            ),
            PipelineError::StationGroups { stations, groups } => write!(
                f,
                "station groups must be one per station: {stations} stations, {groups} groups"
            ),
            PipelineError::TooComplex { work, max } => budget::describe(f, *work, *max),
        }
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.subject() {
            Subject::Call => {}
            Subject::Route {
                route,
                point: Some(point),
            } => write!(f, "route {route}, point {point}: ")?,
            Subject::Route { route, point: None } => write!(f, "route {route}: ")?,
            Subject::Station { station } => write!(f, "station {station}: ")?,
        }
        Reason(self).fmt(f)
    }
}

impl std::error::Error for PipelineError {}

impl From<Exceeded> for PipelineError {
    fn from(e: Exceeded) -> Self {
        Self::TooComplex {
            work: e.work,
            max: e.max,
        }
    }
}

/// A topology error as the pipeline's. [`check_call`](super::limits::check_call) makes these
/// impossible; mapped anyway, never a panic.
pub(super) fn from_topology(e: TopologyError) -> PipelineError {
    match e {
        TopologyError::TooShort { route } | TopologyError::NotFinite { route } => {
            PipelineError::TooShort { route }
        }
        TopologyError::InvalidParams(why) => PipelineError::InvalidSettings(why),
        TopologyError::TooComplex { work, max } => PipelineError::TooComplex { work, max },
    }
}
