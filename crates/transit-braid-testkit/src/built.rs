//! The graph the core's topology builds for a fixture: runs
//! [`transit_braid_core::topology::build`] on it and returns the result as a fixture-style
//! [`LineGraph`], so the criteria ([`crate::criteria`]) and the hand-made topology test (C4)
//! compare it with the expected graph.

use transit_braid_core::geom::Point;
use transit_braid_core::topology::{self, Dir, Params, Topology};

use crate::place::{from_meters, reference_latitude, to_meters};
use crate::{Direction, Edge, EdgeLine, Fixture, LineGraph, Node};

/// Builds `fixture`'s line graph with the spike algorithm. Node and edge ids are `n0, n1, …`
/// and `e0, e1, …`; coordinates are in the fixture's units.
///
/// # Errors
/// If the algorithm refuses the fixture's lines; the message starts with the fixture name.
pub fn line_graph(fixture: &Fixture, params: &Params) -> Result<LineGraph, String> {
    let lines = in_meters(fixture);
    let t = topology::build(&lines, params).map_err(|e| format!("{}: {e}", fixture.name))?;
    Ok(to_line_graph(fixture, &t))
}

/// [`line_graph`], plus each fixture line's walk through it (the topology's traversals), for
/// C2 ([`crate::criteria::check_walked`]).
///
/// # Errors
/// As [`line_graph`].
pub fn line_graph_walked(
    fixture: &Fixture,
    params: &Params,
) -> Result<(LineGraph, Vec<crate::criteria::Walk>), String> {
    let lines = in_meters(fixture);
    let (t, traversals) = topology::build_with_traversals(&lines, params)
        .map_err(|e| format!("{}: {e}", fixture.name))?;
    let walks = traversals
        .iter()
        .map(|walk| {
            walk.iter()
                .map(|&(e, dir)| {
                    let dir = if dir == Dir::Backward {
                        Direction::Backward
                    } else {
                        Direction::Forward
                    };
                    (e.index(), dir)
                })
                .collect()
        })
        .collect();
    Ok((to_line_graph(fixture, &t), walks))
}

fn in_meters(fixture: &Fixture) -> Vec<Vec<Point>> {
    let lat0 = reference_latitude(fixture);
    fixture
        .lines
        .iter()
        .map(|l| to_meters(&l.points, fixture.units, lat0))
        .collect()
}

fn to_line_graph(fixture: &Fixture, t: &Topology) -> LineGraph {
    let lat0 = reference_latitude(fixture);
    let back = |p| from_meters(p, fixture.units, lat0);
    let nodes = t
        .nodes
        .iter()
        .enumerate()
        .map(|(i, &p)| Node {
            id: format!("n{i}"),
            at: back(p),
        })
        .collect();
    let edges = t
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| Edge {
            id: format!("e{i}"),
            from: e.from.index(),
            to: e.to.index(),
            via: e.path[1..e.path.len() - 1]
                .iter()
                .map(|&p| back(p))
                .collect(),
            lines: e
                .lines
                .iter()
                .map(|&(l, dir)| EdgeLine {
                    line: fixture.lines[l.index()].id.clone(),
                    dir: match dir {
                        Dir::Forward => Direction::Forward,
                        Dir::Backward => Direction::Backward,
                        Dir::Both => Direction::Both,
                    },
                })
                .collect(),
        })
        .collect();
    LineGraph { nodes, edges }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{FixtureLine, Units, load, load_graph};
    use transit_braid_core::geom::Point;
    use transit_braid_core::ids::EdgeId;

    use super::*;
    use crate::criteria::{check, same_topology};

    /// On every hand-made fixture, each route's walk is connected: every step starts at
    /// the node where the step before it ends.
    #[test]
    fn every_hand_made_fixture_walks_connected_paths() {
        for (folder, name) in [
            ("synthetic", "dog-bone"),
            ("synthetic", "hairpin"),
            ("synthetic", "opposite-direction"),
            ("synthetic", "parallel-corridor"),
            ("synthetic", "x-crossing"),
            ("synthetic", "y-split"),
            ("bus", "out-and-back"),
            ("bus", "one-way-pair"),
            ("bus", "loop-line"),
        ] {
            let (f, _) = pair(folder, name);
            let (t, walks) =
                topology::build_with_traversals(&in_meters(&f), &Params::default()).unwrap();
            for (i, walk) in walks.iter_enumerated() {
                assert!(!walk.is_empty(), "{name}: route {i} walks nothing");
                let ends = |(e, dir): (EdgeId, Dir)| {
                    let edge = &t.edges[e];
                    if dir == Dir::Backward {
                        (edge.to, edge.from)
                    } else {
                        (edge.from, edge.to)
                    }
                };
                assert!(
                    walk.windows(2).all(|w| ends(w[0]).1 == ends(w[1]).0),
                    "{name}: route {i} jumps: {walk:?}"
                );
            }
        }
    }

    fn pair(folder: &str, name: &str) -> (Fixture, LineGraph) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(folder);
        let f = load(&dir.join(format!("{name}.geojson"))).unwrap();
        let g = load_graph(&dir.join(format!("{name}.graph.json")), &f).unwrap();
        (f, g)
    }

    #[test]
    fn builds_the_expected_topology_for_hand_made_fixtures() {
        for (folder, name) in [
            ("synthetic", "dog-bone"),
            ("synthetic", "hairpin"),
            ("synthetic", "opposite-direction"),
            ("synthetic", "parallel-corridor"),
            ("synthetic", "x-crossing"),
            ("synthetic", "y-split"),
            ("bus", "out-and-back"),
            ("bus", "one-way-pair"),
            ("bus", "loop-line"),
        ] {
            let (f, expected) = pair(folder, name);
            let built = line_graph(&f, &Params::default()).unwrap();
            assert_eq!(same_topology(&expected, &built), Ok(()), "{name}");
            let r = check(&f, &built);
            assert!(r.disconnected.is_empty(), "{name}: {r:?}");
        }
    }

    #[test]
    fn plain_crossings_are_not_nodes() {
        // Two bundles that only cross get no node (two edges, 4 end nodes).
        let (f, expected) = pair("synthetic", "x-crossing");
        let built = line_graph(&f, &Params::default()).unwrap();
        assert_eq!((built.nodes.len(), built.edges.len()), (4, 2));
        assert_eq!(same_topology(&expected, &built), Ok(()));
    }

    #[test]
    fn out_and_back_is_both_ways_in_the_lab_format() {
        let (f, _) = pair("bus", "out-and-back");
        let built = line_graph(&f, &Params::default()).unwrap();
        let shared = built.edges.iter().find(|e| e.lines.len() == 2).unwrap();
        let a = shared.lines.iter().find(|l| l.line == "A").unwrap();
        assert_eq!(a.dir, Direction::Both);
    }

    #[test]
    fn degree_fixtures_come_back_in_degrees() {
        // Two lines 1 m apart in Edmonton: one edge, nodes near the input.
        let line = |id: &str, y: f64| FixtureLine {
            id: id.to_owned(),
            color: "#000000".to_owned(),
            points: vec![Point::new(-113.50, y), Point::new(-113.49, y)],
            sign: None,
            direction: None,
        };
        let f = Fixture {
            name: "t".to_owned(),
            description: String::new(),
            units: Units::Degrees,
            license: String::new(),
            attribution: String::new(),
            expect: String::new(),
            stations: Vec::new(),
            lines: vec![line("A", 53.54), line("B", 53.540_01)],
        };
        let g = line_graph(&f, &Params::default()).unwrap();
        assert_eq!((g.nodes.len(), g.edges.len()), (2, 1));
        let west = g
            .nodes
            .iter()
            .map(|n| n.at)
            .fold(
                Point::new(f64::MAX, 0.0),
                |a, p| if p.x < a.x { p } else { a },
            );
        assert!(
            (west.x + 113.50).abs() < 1e-6 && (west.y - 53.540_005).abs() < 1e-5,
            "{west:?}"
        );
    }

    #[test]
    fn a_fixture_the_algorithm_refuses_is_an_error_with_its_name() {
        let (mut f, _) = pair("synthetic", "y-split");
        f.lines[1].points = vec![Point::new(5.0, 5.0), Point::new(5.0, 5.0)];
        let err = line_graph(&f, &Params::default()).unwrap_err();
        assert_eq!(err, "y-split: route 1: needs at least two distinct points");
    }
}
