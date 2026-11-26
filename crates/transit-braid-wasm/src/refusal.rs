//! Why the pipeline refused a call, as data ([`Refusal`]): JavaScript gets a stable code, the
//! reason and the indices of the route, point or station at fault, and names them itself.

use std::fmt;

use transit_braid_core::pipeline::{PipelineError, Subject};
use wasm_bindgen::prelude::*;

/// A refused call: thrown to JavaScript in place of a result.
#[wasm_bindgen]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    code: &'static str,
    reason: String,
    route: Option<u32>,
    point: Option<u32>,
    station: Option<u32>,
}

#[wasm_bindgen]
impl Refusal {
    /// A stable code: the input error codes of the TypeScript API (`BAD_COORDINATE`,
    /// `TOO_COMPLEX`…), or `BAD_LAYOUT` for a call whose flat layout the bindings can't read
    /// (never the input's fault when the TypeScript package made the call).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn code(&self) -> String {
        self.code.to_owned()
    }

    /// Why, without naming the route, point or station (those are the fields below).
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn reason(&self) -> String {
        self.reason.clone()
    }

    /// The route at fault, by its place in the input; `undefined` for none.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn route(&self) -> Option<u32> {
        self.route
    }

    /// The point at fault, by its place in that route; `undefined` for none.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn point(&self) -> Option<u32> {
        self.point
    }

    /// The station at fault, by its place in the input; `undefined` for none.
    #[wasm_bindgen(getter)]
    #[must_use]
    pub fn station(&self) -> Option<u32> {
        self.station
    }
}

impl Refusal {
    /// A refusal of the call's flat layout or state, not of what it describes.
    pub(crate) fn layout(reason: String) -> Self {
        Self::of_call("BAD_LAYOUT", reason)
    }

    /// A refusal about the call as a whole.
    pub(crate) fn of_call(code: &'static str, reason: String) -> Self {
        Self {
            code,
            reason,
            route: None,
            point: None,
            station: None,
        }
    }

    /// The stable code (as [`Refusal::code`], without a copy).
    #[must_use]
    pub fn code_str(&self) -> &'static str {
        self.code
    }
}

/// An index as JavaScript gets it; past `u32::MAX` (never, given the limits) it saturates.
fn index(i: usize) -> u32 {
    u32::try_from(i).unwrap_or(u32::MAX)
}

impl From<PipelineError> for Refusal {
    fn from(e: PipelineError) -> Self {
        let code = match e {
            PipelineError::TooMany { .. } => "TOO_MANY_ROUTES",
            PipelineError::BadCoordinate { .. } => "BAD_COORDINATE",
            PipelineError::TooShort { .. } => "TOO_FEW_POINTS",
            PipelineError::InvalidSettings(_) => "BAD_OPTION",
            PipelineError::BadStation { .. } => "BAD_STATION",
            PipelineError::TooManyStations { .. } => "TOO_MANY_STATIONS",
            PipelineError::TooManyPoints { .. } => "TOO_MANY_POINTS",
            PipelineError::CrossesAntimeridian { .. } => "CROSSES_ANTIMERIDIAN",
            PipelineError::TooWide { .. } => "TOO_WIDE",
            PipelineError::TooLong { .. } => "TOO_LONG",
            // The bindings always send one group per station, or none.
            PipelineError::StationGroups { .. } => "BAD_LAYOUT",
            PipelineError::TooComplex { .. } => "TOO_COMPLEX",
        };
        let mut refusal = Self::of_call(code, e.reason());
        match e.subject() {
            Subject::Call => {}
            Subject::Route { route, point } => {
                refusal.route = Some(index(route));
                refusal.point = point.map(index);
            }
            Subject::Station { station } => refusal.station = Some(index(station)),
        }
        refusal
    }
}

/// The message the pipeline gives: the subject, then the reason (`route 0, point 1: …`).
impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.route, self.point, self.station) {
            (Some(route), Some(point), _) => write!(f, "route {route}, point {point}: ")?,
            (Some(route), None, _) => write!(f, "route {route}: ")?,
            (None, _, Some(station)) => write!(f, "station {station}: ")?,
            _ => {}
        }
        f.write_str(&self.reason)
    }
}

impl std::error::Error for Refusal {}
