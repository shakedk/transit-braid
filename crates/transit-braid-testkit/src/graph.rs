//! Line graphs: the topology a fixture should turn into.
//!
//! A line graph has **nodes** (where the set of routes changes, or a route ends) and
//! **edges** between them. Each edge lists the routes that run along it and in which
//! direction. The file format is described in `fixtures/README.md`; hand-built expected graphs
//! sit next to their fixture as `<name>.graph.json`.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Deserialize;
use transit_braid_core::geom::Point;

use crate::{Fixture, FixtureError, err, to_point};

/// Which way a route runs along an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// From the edge's `from` node to its `to` node.
    Forward,
    /// From `to` to `from`.
    Backward,
    /// Both ways: the route runs the edge out and back. It still
    /// takes one slot.
    Both,
}

/// A place where the set of routes changes, or a route ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Unique within the graph.
    pub id: String,
    /// Position, in the fixture's units.
    pub at: Point,
}

/// One route on an edge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeLine {
    /// The fixture line's id.
    pub line: String,
    /// Which way it runs.
    pub dir: Direction,
}

/// A stretch between two nodes with one fixed set of routes.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    /// Unique within the graph.
    pub id: String,
    /// Index into [`LineGraph::nodes`].
    pub from: usize,
    /// Index into [`LineGraph::nodes`].
    pub to: usize,
    /// Centerline bends between the two nodes (may be empty for a straight edge).
    pub via: Vec<Point>,
    /// The routes on this edge, each listed once, in file order.
    pub lines: Vec<EdgeLine>,
}

impl Edge {
    /// The centerline from the `from` node to the `to` node, bends included.
    #[must_use]
    pub fn centerline(&self, nodes: &[Node]) -> Vec<Point> {
        let mut pts = Vec::with_capacity(self.via.len() + 2);
        pts.push(nodes[self.from].at);
        pts.extend_from_slice(&self.via);
        pts.push(nodes[self.to].at);
        pts
    }
}

/// A validated line graph for one fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct LineGraph {
    /// Nodes, in file order.
    pub nodes: Vec<Node>,
    /// Edges, in file order.
    pub edges: Vec<Edge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawGraph {
    fixture: String,
    nodes: Vec<RawNode>,
    edges: Vec<RawEdge>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawNode {
    id: String,
    at: Vec<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEdge {
    id: String,
    from: String,
    to: String,
    #[serde(default)]
    via: Vec<Vec<f64>>,
    lines: Vec<RawEdgeLine>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEdgeLine {
    line: String,
    dir: Direction,
}

/// Parses and validates a line graph against the fixture it belongs to.
///
/// # Errors
/// If the JSON is malformed, `fixture` names another fixture, an id is empty or used twice,
/// an edge points at a missing node or at its own start, a coordinate is invalid, an edge has
/// no routes or lists one twice, a route is not in the fixture, or a fixture route is
/// on no edge.
pub fn parse_graph(json: &str, fixture: &Fixture) -> Result<LineGraph, FixtureError> {
    let raw: RawGraph =
        serde_json::from_str(json).map_err(|e| FixtureError(format!("invalid graph JSON: {e}")))?;
    if raw.fixture != fixture.name {
        return err(format!(
            "graph is for fixture {}, not {}",
            raw.fixture, fixture.name
        ));
    }
    if raw.nodes.is_empty() || raw.edges.is_empty() {
        return err("graph needs at least one node and one edge");
    }

    let mut node_index = HashMap::new();
    let mut nodes = Vec::with_capacity(raw.nodes.len());
    for (i, n) in raw.nodes.into_iter().enumerate() {
        check_id("node", i, &n.id)?;
        if node_index.insert(n.id.clone(), nodes.len()).is_some() {
            return err(format!("node {}: id is used twice", n.id));
        }
        let at = to_point(&format!("node {}", n.id), &n.at, fixture.units)?;
        nodes.push(Node { id: n.id, at });
    }

    let fixture_lines: HashSet<&str> = fixture.lines.iter().map(|l| l.id.as_str()).collect();
    let mut used_lines = HashSet::new();
    let mut edge_ids = HashSet::new();
    let mut edges = Vec::with_capacity(raw.edges.len());
    for (i, e) in raw.edges.into_iter().enumerate() {
        check_id("edge", i, &e.id)?;
        if !edge_ids.insert(e.id.clone()) {
            return err(format!("edge {}: id is used twice", e.id));
        }
        let end = |name: &str| {
            node_index
                .get(name)
                .copied()
                .ok_or_else(|| FixtureError(format!("edge {}: no node {name}", e.id)))
        };
        let (from, to) = (end(&e.from)?, end(&e.to)?);
        if from == to {
            return err(format!("edge {}: starts and ends at node {}", e.id, e.from));
        }
        let via = e
            .via
            .iter()
            .enumerate()
            .map(|(k, c)| to_point(&format!("edge {}, via {k}", e.id), c, fixture.units))
            .collect::<Result<Vec<_>, _>>()?;
        if e.lines.is_empty() {
            return err(format!("edge {}: has no routes", e.id));
        }
        let mut on_edge = HashSet::new();
        let mut lines = Vec::with_capacity(e.lines.len());
        for l in e.lines {
            if !fixture_lines.contains(l.line.as_str()) {
                return err(format!(
                    "edge {}: line {} is not in the fixture",
                    e.id, l.line
                ));
            }
            if !on_edge.insert(l.line.clone()) {
                return err(format!(
                    "edge {}: line {} is listed twice (a route takes one slot per edge)",
                    e.id, l.line
                ));
            }
            used_lines.insert(l.line.clone());
            lines.push(EdgeLine {
                line: l.line,
                dir: l.dir,
            });
        }
        edges.push(Edge {
            id: e.id,
            from,
            to,
            via,
            lines,
        });
    }

    if let Some(missing) = fixture.lines.iter().find(|l| !used_lines.contains(&l.id)) {
        return err(format!("line {} is on no edge", missing.id));
    }
    Ok(LineGraph { nodes, edges })
}

/// Loads `<name>.graph.json` and validates it against `fixture`.
///
/// # Errors
/// If the file can't be read or the graph is invalid. The message starts with the path.
pub fn load_graph(path: &Path, fixture: &Fixture) -> Result<LineGraph, FixtureError> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| FixtureError(format!("{}: {e}", path.display())))?;
    parse_graph(&text, fixture).map_err(|e| FixtureError(format!("{}: {e}", path.display())))
}

fn check_id(kind: &str, i: usize, id: &str) -> Result<(), FixtureError> {
    if id.trim().is_empty() {
        return err(format!("{kind} {i}: id must not be empty"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FixtureLine, Units};

    fn fixture(units: Units) -> Fixture {
        let line = |id: &str| FixtureLine {
            id: id.to_owned(),
            color: "#000000".to_owned(),
            points: vec![Point::new(0.0, 0.0), Point::new(1.0, 0.0)],
            sign: None,
            direction: None,
        };
        Fixture {
            name: "f".to_owned(),
            description: "d".to_owned(),
            units,
            license: "MIT".to_owned(),
            attribution: "a".to_owned(),
            expect: "e".to_owned(),
            stations: Vec::new(),
            lines: vec![line("A"), line("B")],
        }
    }

    const GOOD: &str = r#"{
        "fixture": "f",
        "nodes": [{"id": "n1", "at": [0, 0]}, {"id": "n2", "at": [1, 0]}],
        "edges": [{"id": "e1", "from": "n1", "to": "n2", "via": [[0.5, 0.1]],
                   "lines": [{"line": "A", "dir": "forward"}, {"line": "B", "dir": "backward"}]}]
    }"#;

    #[test]
    fn parses_a_valid_graph() {
        let g = parse_graph(GOOD, &fixture(Units::Meters)).unwrap();
        assert_eq!(
            g.nodes[1],
            Node {
                id: "n2".to_owned(),
                at: Point::new(1.0, 0.0)
            }
        );
        let e = &g.edges[0];
        assert_eq!((e.from, e.to), (0, 1));
        assert_eq!(
            e.lines[1],
            EdgeLine {
                line: "B".to_owned(),
                dir: Direction::Backward
            }
        );
        assert_eq!(
            e.centerline(&g.nodes),
            vec![
                Point::new(0.0, 0.0),
                Point::new(0.5, 0.1),
                Point::new(1.0, 0.0)
            ]
        );
    }

    #[test]
    fn both_directions_parse() {
        let json = GOOD.replace(r#""dir": "backward""#, r#""dir": "both""#);
        let g = parse_graph(&json, &fixture(Units::Meters)).unwrap();
        assert_eq!(g.edges[0].lines[1].dir, Direction::Both);
    }

    #[test]
    fn via_is_optional() {
        let json = GOOD.replace(r#""via": [[0.5, 0.1]],"#, "");
        let g = parse_graph(&json, &fixture(Units::Meters)).unwrap();
        assert!(g.edges[0].via.is_empty());
        assert_eq!(g.edges[0].centerline(&g.nodes).len(), 2);
    }

    /// Each broken variant of GOOD must fail with a message containing the given text.
    #[test]
    fn rejects_broken_graphs_with_the_reason() {
        let cases: Vec<(String, &str)> = vec![
            (String::new(), "invalid graph JSON"),
            ("null".to_owned(), "invalid graph JSON"),
            ("{}".to_owned(), "invalid graph JSON"),
            (
                GOOD.replace(r#""fixture": "f""#, r#""fixture": null"#),
                "invalid graph JSON",
            ),
            (
                GOOD.replace(r#""fixture": "f""#, r#""fixture": "g""#),
                "graph is for fixture g, not f",
            ),
            (
                GOOD.replace(r#""fixture": "f","#, r#""fixture": "f", "extra": 1,"#),
                "unknown field",
            ),
            (
                GOOD.replace(
                    r#"[{"id": "n1", "at": [0, 0]}, {"id": "n2", "at": [1, 0]}]"#,
                    "[]",
                ),
                "at least one node and one edge",
            ),
            (
                GOOD.replace(r#""id": "e1""#, r#""id": " ""#),
                "edge 0: id must not be empty",
            ),
            (
                GOOD.replace(r#""id": "n1""#, r#""id": """#),
                "node 0: id must not be empty",
            ),
            (
                GOOD.replace(r#""id": "n2""#, r#""id": "n1""#),
                "node n1: id is used twice",
            ),
            (
                GOOD.replace(r#""at": [1, 0]"#, r#""at": [1]"#),
                "node n2: expected [x, y], got 1",
            ),
            (
                GOOD.replace(r#""at": [1, 0]"#, r#""at": [1, null]"#),
                "invalid graph JSON",
            ),
            (
                GOOD.replace(r#""to": "n2""#, r#""to": "n9""#),
                "edge e1: no node n9",
            ),
            (
                GOOD.replace(r#""from": "n1""#, r#""from": "n0""#),
                "edge e1: no node n0",
            ),
            (
                GOOD.replace(r#""to": "n2""#, r#""to": "n1""#),
                "edge e1: starts and ends at node n1",
            ),
            (
                GOOD.replace("[[0.5, 0.1]]", "[[0.5]]"),
                "edge e1, via 0: expected [x, y]",
            ),
            (
                GOOD.replace(
                    r#"[{"line": "A", "dir": "forward"}, {"line": "B", "dir": "backward"}]"#,
                    "[]",
                ),
                "edge e1: has no routes",
            ),
            (
                GOOD.replace(r#""line": "B""#, r#""line": "Z""#),
                "line Z is not in the fixture",
            ),
            (
                GOOD.replace(r#""line": "B""#, r#""line": "A""#),
                "line A is listed twice",
            ),
            (
                GOOD.replace(r#""dir": "backward""#, r#""dir": "up""#),
                "invalid graph JSON",
            ),
            (
                GOOD.replace(r#", {"line": "B", "dir": "backward"}"#, ""),
                "line B is on no edge",
            ),
        ];
        for (json, reason) in cases {
            let e = parse_graph(&json, &fixture(Units::Meters)).unwrap_err();
            assert!(e.0.contains(reason), "expected {reason:?}, got {:?}", e.0);
        }
    }

    #[test]
    fn duplicate_edge_ids_are_rejected() {
        let edge = r#"{"id": "e1", "from": "n1", "to": "n2", "lines": [{"line": "A", "dir": "forward"}, {"line": "B", "dir": "forward"}]}"#;
        let json = format!(
            r#"{{"fixture": "f", "nodes": [{{"id": "n1", "at": [0, 0]}}, {{"id": "n2", "at": [1, 0]}}], "edges": [{edge}, {edge}]}}"#
        );
        let e = parse_graph(&json, &fixture(Units::Meters)).unwrap_err();
        assert!(e.0.contains("edge e1: id is used twice"), "{e}");
    }

    #[test]
    fn degree_graphs_check_longitude_and_latitude() {
        let json = GOOD.replace(r#""at": [1, 0]"#, r#""at": [1, 95]"#);
        let e = parse_graph(&json, &fixture(Units::Degrees)).unwrap_err();
        assert!(
            e.0.contains("node n2: [1, 95] is not a valid [lng, lat]"),
            "{e}"
        );
        assert!(parse_graph(&json, &fixture(Units::Meters)).is_ok());
    }

    #[test]
    fn load_graph_prefixes_errors_with_the_path() {
        let missing = Path::new("/no/such/f.graph.json");
        let e = load_graph(missing, &fixture(Units::Meters)).unwrap_err();
        assert!(e.0.starts_with("/no/such/f.graph.json: "), "{e}");
    }
}
