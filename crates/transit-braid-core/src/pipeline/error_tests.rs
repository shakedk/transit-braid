//! Tests of [`PipelineError`]'s text: its subject and reason, and the full message they make.

use super::{PipelineError, Subject};
use crate::budget::Work;

/// Every variant: its subject, its reason, and the full message (the subject's name, then the
/// reason). The full messages are the ones the pipeline always gave.
#[test]
fn every_error_names_its_subject_then_says_why() {
    let coordinate = "needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05";
    let cases = [
        (
            PipelineError::TooMany { count: 71 },
            Subject::Call,
            "too many routes: 71 given, at most 70 allowed".to_owned(),
        ),
        (
            PipelineError::BadCoordinate { route: 1, point: 3 },
            Subject::Route {
                route: 1,
                point: Some(3),
            },
            coordinate.to_owned(),
        ),
        (
            PipelineError::TooShort { route: 2 },
            Subject::Route {
                route: 2,
                point: None,
            },
            "needs at least two distinct points".to_owned(),
        ),
        (
            PipelineError::InvalidSettings("zoom must be a number from 0 to 24".to_owned()),
            Subject::Call,
            "invalid settings: zoom must be a number from 0 to 24".to_owned(),
        ),
        (
            PipelineError::BadStation { station: 4 },
            Subject::Station { station: 4 },
            coordinate.to_owned(),
        ),
        (
            PipelineError::TooManyStations { count: 5001 },
            Subject::Call,
            "too many stations: 5001 given, at most 5000 allowed".to_owned(),
        ),
        (
            PipelineError::TooManyPoints { count: 1_000_001 },
            Subject::Call,
            "too many points: 1000001 given, at most 1000000 allowed (all routes together)"
                .to_owned(),
        ),
        (
            PipelineError::CrossesAntimeridian { route: 0, point: 2 },
            Subject::Route {
                route: 0,
                point: Some(2),
            },
            "more than 180° of longitude from the point before, so it crosses the date line; \
             split the route there"
                .to_owned(),
        ),
        (
            PipelineError::TooWide {
                width_km: 250.04,
                height_km: 1.0,
            },
            Subject::Call,
            "the routes spread 250.0 km east-west and 1.0 km north-south; at most 200 km \
             each way"
                .to_owned(),
        ),
        (
            PipelineError::TooLong { km: 2100.0 },
            Subject::Call,
            "the routes are 2100.0 km long together; at most 2000 km".to_owned(),
        ),
        (
            PipelineError::StationGroups {
                stations: 3,
                groups: 1,
            },
            Subject::Call,
            "station groups must be one per station: 3 stations, 1 groups".to_owned(),
        ),
        (
            PipelineError::TooComplex {
                work: Work::GraphSteps,
                max: 50_000_000,
            },
            Subject::Call,
            "too complex: the routes cross and overlap in too many short pieces (more than \
             50000000 graph steps)"
                .to_owned(),
        ),
    ];
    for (error, subject, reason) in cases {
        assert_eq!(error.subject(), subject, "{error:?}");
        assert_eq!(error.reason(), reason, "{error:?}");
        let named = match subject {
            Subject::Call => String::new(),
            Subject::Route {
                route,
                point: Some(point),
            } => format!("route {route}, point {point}: "),
            Subject::Route { route, point: None } => format!("route {route}: "),
            Subject::Station { station } => format!("station {station}: "),
        };
        assert_eq!(error.to_string(), format!("{named}{reason}"));
    }
}

/// The messages the pipeline gave before the subject and reason were split, word for word.
#[test]
fn the_full_messages_are_unchanged() {
    assert_eq!(
        PipelineError::BadCoordinate { route: 1, point: 3 }.to_string(),
        "route 1, point 3: needs a finite longitude from -180 to 180 and latitude from -85.05 \
         to 85.05"
    );
    assert_eq!(
        PipelineError::BadStation { station: 0 }.to_string(),
        "station 0: needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05"
    );
    assert_eq!(
        PipelineError::TooShort { route: 0 }.to_string(),
        "route 0: needs at least two distinct points"
    );
    assert_eq!(
        PipelineError::CrossesAntimeridian { route: 2, point: 1 }.to_string(),
        "route 2, point 1: more than 180° of longitude from the point before, so it crosses \
         the date line; split the route there"
    );
}
