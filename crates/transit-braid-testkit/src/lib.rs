//! What the tests need to prove transit-braid's output: the fixtures (GeoJSON line networks plus
//! `braid` metadata, loaded and validated here; the format is in `fixtures/README.md`), their
//! hand-built expected graphs, the golden digests of everything the pipeline draws ([`golden`]),
//! the quality rules ([`quality`]) and the clean-graph criteria ([`criteria`]) with the graph the
//! core's topology builds ([`built`]).

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use transit_braid_core::geom::Point;

pub mod built;
pub mod criteria;
pub mod despike;
pub mod golden;
mod graph;
pub mod place;
pub mod quality;
pub use graph::{Direction, Edge, EdgeLine, LineGraph, Node, load_graph, parse_graph};

/// Coordinate units of a fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Units {
    /// Local x/y in meters (synthetic fixtures).
    Meters,
    /// `[longitude, latitude]` in degrees (real data).
    Degrees,
}

/// One transit line in a fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureLine {
    /// Line id, unique within the fixture.
    pub id: String,
    /// Color as `#rrggbb`.
    pub color: String,
    /// The line's path, at least two points.
    pub points: Vec<Point>,
    /// The route sign riders see (GTFS `route_short_name`, e.g. `"130"`), when known. Several
    /// lines can share one: both directions, or two copies of one route. Labels only.
    pub sign: Option<String>,
    /// The direction riders see it go in (GTFS `direction_id`, e.g. `"0"`), when known. A route
    /// is a sign and a direction; lines sharing both are variants of one route.
    pub direction: Option<String>,
}

/// A stop in a fixture, from `braid.stations`.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureStation {
    /// The feed's stop id, when known (GTFS `stop_id`; several joined by ", " for stops the
    /// importer merged at one spot). Only for finding the stop; it doesn't change any bar.
    pub id: Option<String>,
    /// The feed's stop code (the number riders see at the stop), when known; joined like `id`.
    pub code: Option<String>,
    /// The stop's name, when it has one. Same-name stops close together share a bar.
    pub name: Option<String>,
    /// Where it is, in the fixture's units.
    pub at: Point,
    /// The ids of this fixture's lines that stop here (from the feed), when known; else empty.
    pub routes: Vec<String>,
}

/// A validated fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct Fixture {
    /// Short id, same as the file name.
    pub name: String,
    /// What situation it tests.
    pub description: String,
    /// Coordinate units.
    pub units: Units,
    /// License of the data.
    pub license: String,
    /// Who made the data.
    pub attribution: String,
    /// What good output looks like, in plain words.
    pub expect: String,
    /// The lines, in file order.
    pub lines: Vec<FixtureLine>,
    /// Stops (`braid.stations`), in file order; most fixtures have none.
    pub stations: Vec<FixtureStation>,
}

/// Why a fixture could not be loaded. The message names the file part that is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureError(pub String);

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FixtureError {}

fn err<T>(msg: impl Into<String>) -> Result<T, FixtureError> {
    Err(FixtureError(msg.into()))
}

// Raw shapes as they appear in the JSON, before validation.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCollection {
    #[serde(rename = "type")]
    kind: String,
    braid: RawMeta,
    features: Vec<RawFeature>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMeta {
    name: String,
    description: String,
    units: Units,
    license: String,
    attribution: String,
    expect: String,
    #[serde(default)]
    stations: Vec<RawStation>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStation {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    routes: Vec<String>,
    #[serde(default)]
    name: Option<String>,
    at: Vec<f64>,
}

#[derive(Deserialize)]
struct RawFeature {
    properties: RawProps,
    geometry: RawGeometry,
}

#[derive(Deserialize)]
struct RawProps {
    line: String,
    color: String,
    #[serde(default)]
    sign: Option<String>,
    #[serde(default)]
    direction: Option<String>,
}

#[derive(Deserialize)]
struct RawGeometry {
    #[serde(rename = "type")]
    kind: String,
    coordinates: Vec<Vec<f64>>,
}

/// Parses and validates a fixture from JSON text.
///
/// # Errors
/// If the JSON is malformed or breaks a rule in `fixtures/README.md`.
pub fn parse(json: &str) -> Result<Fixture, FixtureError> {
    let raw: RawCollection =
        serde_json::from_str(json).map_err(|e| FixtureError(format!("invalid JSON: {e}")))?;
    if raw.kind != "FeatureCollection" {
        return err(format!("type must be FeatureCollection, got {}", raw.kind));
    }
    let meta = raw.braid;
    for (field, value) in [
        ("name", &meta.name),
        ("description", &meta.description),
        ("license", &meta.license),
        ("attribution", &meta.attribution),
        ("expect", &meta.expect),
    ] {
        if value.trim().is_empty() {
            return err(format!("braid.{field} must not be empty"));
        }
    }
    if raw.features.is_empty() {
        return err("fixture has no lines");
    }

    let mut seen = HashSet::new();
    let mut lines = Vec::with_capacity(raw.features.len());
    for (i, f) in raw.features.into_iter().enumerate() {
        let id = f.properties.line;
        if id.trim().is_empty() {
            return err(format!("feature {i}: properties.line must not be empty"));
        }
        if !seen.insert(id.clone()) {
            return err(format!("line {id}: id is used twice"));
        }
        if !is_hex_color(&f.properties.color) {
            return err(format!(
                "line {id}: color must be #rrggbb, got {}",
                f.properties.color
            ));
        }
        if f.geometry.kind != "LineString" {
            return err(format!(
                "line {id}: geometry must be LineString, got {}",
                f.geometry.kind
            ));
        }
        let points = to_points(&id, &f.geometry.coordinates, meta.units)?;
        let sign = match f.properties.sign {
            Some(s) if s.trim().is_empty() => {
                return err(format!("line {id}: properties.sign must not be empty"));
            }
            other => other,
        };
        let direction = match f.properties.direction {
            Some(d) if d.trim().is_empty() => {
                return err(format!("line {id}: properties.direction must not be empty"));
            }
            other => other,
        };
        lines.push(FixtureLine {
            id,
            color: f.properties.color,
            points,
            sign,
            direction,
        });
    }

    let mut stations = Vec::with_capacity(meta.stations.len());
    for (i, st) in meta.stations.into_iter().enumerate() {
        let at = to_point(&format!("station {i}"), &st.at, meta.units)?;
        if st.name.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return err(format!("station {i}: name must not be empty"));
        }
        if st.id.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return err(format!("station {i}: id must not be empty"));
        }
        if st.code.as_deref().is_some_and(|n| n.trim().is_empty()) {
            return err(format!("station {i}: code must not be empty"));
        }
        if let Some(r) = st
            .routes
            .iter()
            .find(|r| !lines.iter().any(|l| &l.id == *r))
        {
            return err(format!(
                "station {i}: route {r:?} is not a line of this fixture"
            ));
        }
        stations.push(FixtureStation {
            routes: st.routes,
            code: st.code,
            id: st.id,
            name: st.name,
            at,
        });
    }

    Ok(Fixture {
        name: meta.name,
        description: meta.description,
        units: meta.units,
        license: meta.license,
        attribution: meta.attribution,
        expect: meta.expect,
        lines,
        stations,
    })
}

/// One `[x, y]` coordinate; `what` names it in error messages (e.g. "line A, point 3").
fn to_point(what: &str, c: &[f64], units: Units) -> Result<Point, FixtureError> {
    let &[x, y] = c else {
        return err(format!("{what}: expected [x, y], got {} numbers", c.len()));
    };
    if !x.is_finite() || !y.is_finite() {
        return err(format!("{what}: not a finite number"));
    }
    if units == Units::Degrees && (!(-180.0..=180.0).contains(&x) || !(-90.0..=90.0).contains(&y)) {
        return err(format!("{what}: [{x}, {y}] is not a valid [lng, lat]"));
    }
    Ok(Point::new(x, y))
}

fn to_points(id: &str, coords: &[Vec<f64>], units: Units) -> Result<Vec<Point>, FixtureError> {
    if coords.len() < 2 {
        return err(format!(
            "line {id}: needs at least 2 points, got {}",
            coords.len()
        ));
    }
    coords
        .iter()
        .enumerate()
        .map(|(k, c)| to_point(&format!("line {id}, point {k}"), c, units))
        .collect()
}

fn is_hex_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Loads one fixture file and checks that `braid.name` matches the file name.
///
/// # Errors
/// If the file can't be read or the fixture is invalid. The message starts with the path.
pub fn load(path: &Path) -> Result<Fixture, FixtureError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| FixtureError(format!("{}: {e}", path.display())))?;
    let fixture = parse(&text).map_err(|e| FixtureError(format!("{}: {e}", path.display())))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if fixture.name != stem {
        return err(format!(
            "{}: braid.name is {} but the file is {stem}",
            path.display(),
            fixture.name
        ));
    }
    Ok(fixture)
}

/// Loads every `*.geojson` file under `dir` (recursively), sorted by path.
///
/// # Errors
/// On the first file that can't be read or is invalid.
pub fn load_dir(dir: &Path) -> Result<Vec<(PathBuf, Fixture)>, FixtureError> {
    let mut paths = Vec::new();
    collect(dir, &mut paths)?;
    paths.sort();
    paths
        .into_iter()
        .map(|p| load(&p).map(|f| (p, f)))
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), FixtureError> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| FixtureError(format!("{}: {e}", dir.display())))?;
    for entry in entries {
        let path = entry.map_err(|e| FixtureError(e.to_string()))?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "geojson") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_json(features: &str) -> String {
        format!(
            r#"{{"type":"FeatureCollection","braid":{{"name":"t","description":"d","units":"meters",
            "license":"MIT","attribution":"a","expect":"e"}},"features":[{features}]}}"#
        )
    }

    fn line(id: &str, color: &str, coords: &str) -> String {
        format!(
            r#"{{"type":"Feature","properties":{{"line":"{id}","color":"{color}"}},
            "geometry":{{"type":"LineString","coordinates":{coords}}}}}"#
        )
    }

    #[test]
    fn parses_a_valid_fixture() {
        let f = parse(&fixture_json(&line("A", "#ff0000", "[[0,0],[10,5]]"))).unwrap();
        assert_eq!(f.units, Units::Meters);
        assert_eq!(f.lines[0].id, "A");
        assert_eq!(
            f.lines[0].points,
            vec![Point::new(0.0, 0.0), Point::new(10.0, 5.0)]
        );
    }

    /// A fixture with `braid.stations`, and the same header without lines' features.
    fn with_stations(stations: &str) -> String {
        format!(
            r#"{{"type":"FeatureCollection","braid":{{"name":"t","description":"d","units":"meters",
            "license":"MIT","attribution":"a","expect":"e","stations":{stations}}},"features":[{}]}}"#,
            line("A", "#ff0000", "[[0,0],[10,5]]")
        )
    }

    #[test]
    fn stations_are_optional_named_or_not() {
        assert!(
            parse(&fixture_json(&line("A", "#ff0000", "[[0,0],[10,5]]")))
                .unwrap()
                .stations
                .is_empty()
        );
        let f = parse(&with_stations(
            r#"[{"id":"21402","code":"25823","name":"Main","at":[1,2],"routes":["A"]},{"at":[3,4]}]"#,
        ))
        .unwrap();
        assert_eq!(
            f.stations,
            vec![
                FixtureStation {
                    id: Some("21402".to_owned()),
                    code: Some("25823".to_owned()),
                    name: Some("Main".to_owned()),
                    at: Point::new(1.0, 2.0),
                    routes: vec!["A".to_owned()],
                },
                FixtureStation {
                    id: None,
                    code: None,
                    name: None,
                    at: Point::new(3.0, 4.0),
                    routes: Vec::new(),
                },
            ]
        );
        assert!(parse(&with_stations("[]")).unwrap().stations.is_empty());
    }

    #[test]
    fn bad_stations_are_named_by_index() {
        for (stations, msg) in [
            (r#"[{"at":[1]}]"#, "station 0: expected [x, y]"),
            (
                r#"[{"at":[0,0]},{"at":[1,2,3]}]"#,
                "station 1: expected [x, y]",
            ),
            (
                r#"[{"name":" ","at":[0,0]}]"#,
                "station 0: name must not be empty",
            ),
            (
                r#"[{"at":[0,0]},{"id":"","at":[0,0]}]"#,
                "station 1: id must not be empty",
            ),
            (
                r#"[{"code":" ","at":[0,0]}]"#,
                "station 0: code must not be empty",
            ),
            (
                r#"[{"at":[0,0],"routes":["A","nope"]}]"#,
                r#"station 0: route "nope" is not a line of this fixture"#,
            ),
        ] {
            let e = parse(&with_stations(stations)).unwrap_err();
            assert!(e.0.contains(msg), "{stations}: {e}");
        }
        for broken in [
            r#"[{"name":"x"}]"#,
            r#"[{"at":[0,0],"extra":1}]"#,
            r#"[{"id":7,"at":[0,0]}]"#,
            r#"[{"at":[0,0],"routes":"A"}]"#,
            r#"[{"at":[0,0],"routes":[null]}]"#,
            r#"{"at":[0,0]}"#,
            "[null]",
        ] {
            assert!(
                parse(&with_stations(broken))
                    .unwrap_err()
                    .0
                    .contains("invalid JSON"),
                "{broken}"
            );
        }
    }

    #[test]
    fn rejects_broken_fixtures_with_clear_messages() {
        let two = format!(
            "{},{}",
            line("A", "#ff0000", "[[0,0],[1,1]]"),
            line("A", "#00ff00", "[[0,0],[1,1]]")
        );
        let cases = [
            (fixture_json(&two), "used twice"),
            (fixture_json(&line("A", "red", "[[0,0],[1,1]]")), "#rrggbb"),
            (
                fixture_json(&line("A", "#ff0000", "[[0,0]]")),
                "at least 2 points",
            ),
            (
                fixture_json(&line("A", "#ff0000", "[[0,0,0],[1,1]]")),
                "expected [x, y]",
            ),
            (fixture_json(""), "no lines"),
            ("{".to_owned(), "invalid JSON"),
        ];
        for (json, expected) in cases {
            let e = parse(&json).unwrap_err().0;
            assert!(e.contains(expected), "{e:?} should mention {expected:?}");
        }
    }

    #[test]
    fn the_route_sign_is_optional_and_never_empty() {
        let with = |sign: &str| {
            fixture_json(&format!(
                r##"{{"type":"Feature","properties":{{"line":"A","color":"#ff0000","sign":{sign}}},
                "geometry":{{"type":"LineString","coordinates":[[0,0],[1,1]]}}}}"##
            ))
        };
        assert_eq!(
            parse(&with(r#""130""#)).unwrap().lines[0].sign.as_deref(),
            Some("130")
        );
        let none = parse(&fixture_json(&line("A", "#ff0000", "[[0,0],[1,1]]"))).unwrap();
        assert_eq!(none.lines[0].sign, None);
        let empty = parse(&with(r#"" ""#)).unwrap_err().0;
        assert!(
            empty.contains("properties.sign must not be empty"),
            "{empty}"
        );
        assert!(parse(&with("5")).unwrap_err().0.contains("invalid JSON"));
        assert!(parse(&with("null")).unwrap().lines[0].sign.is_none());
    }

    #[test]
    fn the_route_direction_is_optional_and_never_empty() {
        let with = |direction: &str| {
            fixture_json(&format!(
                r##"{{"type":"Feature","properties":{{"line":"A","color":"#ff0000","direction":{direction}}},
                "geometry":{{"type":"LineString","coordinates":[[0,0],[1,1]]}}}}"##
            ))
        };
        assert_eq!(
            parse(&with(r#""1""#)).unwrap().lines[0]
                .direction
                .as_deref(),
            Some("1")
        );
        let none = parse(&fixture_json(&line("A", "#ff0000", "[[0,0],[1,1]]"))).unwrap();
        assert_eq!(none.lines[0].direction, None);
        for blank in [r#""""#, r#"" ""#, r#""\t""#] {
            let e = parse(&with(blank)).unwrap_err().0;
            assert!(
                e.contains("line A: properties.direction must not be empty"),
                "{e}"
            );
        }
        assert!(parse(&with("0")).unwrap_err().0.contains("invalid JSON"));
        assert!(parse(&with("[]")).unwrap_err().0.contains("invalid JSON"));
        assert!(parse(&with("null")).unwrap().lines[0].direction.is_none());
    }

    #[test]
    fn rejects_out_of_range_degrees() {
        let json =
            fixture_json(&line("A", "#ff0000", "[[0,0],[200,10]]")).replace("meters", "degrees");
        assert!(parse(&json).unwrap_err().0.contains("valid [lng, lat]"));
    }

    fn degrees(coords: &str) -> Result<Fixture, FixtureError> {
        parse(&fixture_json(&line("A", "#ff0000", coords)).replace("meters", "degrees"))
    }

    #[test]
    fn accepts_degrees_up_to_and_including_the_edges() {
        let f = degrees("[[-180,-90],[180,90],[10.02,40.75]]").unwrap();
        assert_eq!(f.units, Units::Degrees);
        assert_eq!(f.lines[0].points[0], Point::new(-180.0, -90.0));
        assert_eq!(f.lines[0].points[2], Point::new(10.02, 40.75));
    }

    #[test]
    fn rejects_each_degree_bound_on_its_own() {
        for bad in ["[-180.5,0]", "[180.5,0]", "[0,-90.5]", "[0,90.5]"] {
            let e = degrees(&format!("[[0,0],{bad}]")).unwrap_err().0;
            assert!(e.contains("valid [lng, lat]"), "{bad}: {e:?}");
        }
    }

    #[test]
    fn rejects_non_finite_coordinates() {
        // JSON can't spell NaN or infinity, so call the checker directly.
        for bad in [
            [f64::NAN, 0.0],
            [0.0, f64::NAN],
            [f64::INFINITY, 0.0],
            [0.0, f64::NEG_INFINITY],
        ] {
            let coords = vec![vec![0.0, 0.0], bad.to_vec()];
            let e = to_points("A", &coords, Units::Meters).unwrap_err().0;
            assert!(e.contains("not a finite number"), "{bad:?}: {e:?}");
        }
    }

    #[test]
    fn colors_must_be_exactly_hash_plus_six_hex_digits() {
        assert!(is_hex_color("#a1B2c3"));
        for bad in [
            "", "#", "#ff000", "#ff00000", "ff00000", "#ff00zz", "#ff00 0",
        ] {
            assert!(!is_hex_color(bad), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn error_displays_its_message() {
        let e = FixtureError("line A: broken".to_owned());
        assert_eq!(e.to_string(), "line A: broken");
    }
}
