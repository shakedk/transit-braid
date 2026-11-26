//! Every fixture in the repo's `fixtures/` folder must load and pass validation.

use std::path::Path;

use transit_braid_core::geom::Point;
use transit_braid_testkit::{load, load_dir, load_graph};

#[test]
fn all_fixtures_load() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let fixtures = load_dir(&dir).unwrap_or_else(|e| panic!("{e}"));
    let names: Vec<&str> = fixtures.iter().map(|(_, f)| f.name.as_str()).collect();
    for expected in [
        "dog-bone",
        "hairpin",
        "opposite-direction",
        "parallel-corridor",
        "x-crossing",
        "y-split",
        "out-and-back",
        "one-way-pair",
        "loop-line",
    ] {
        assert!(
            names.contains(&expected),
            "missing fixture {expected}; found {names:?}"
        );
    }
}

/// `(folder, name, nodes, edges)` of every hand-built expected graph.
const EXPECTED_GRAPHS: [(&str, &str, usize, usize); 9] = [
    ("synthetic", "dog-bone", 6, 5),
    ("synthetic", "hairpin", 2, 1),
    ("synthetic", "opposite-direction", 2, 1),
    ("synthetic", "parallel-corridor", 2, 1),
    ("synthetic", "x-crossing", 4, 2),
    ("synthetic", "y-split", 5, 4),
    ("bus", "out-and-back", 3, 2),
    ("bus", "one-way-pair", 4, 4),
    ("bus", "loop-line", 5, 5),
];

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}

#[test]
fn every_hand_made_fixture_has_a_valid_expected_graph() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (folder, name, nodes, edges) in EXPECTED_GRAPHS {
        let dir = root.join(folder);
        let fixture = load(&dir.join(format!("{name}.geojson"))).unwrap();
        let graph = load_graph(&dir.join(format!("{name}.graph.json")), &fixture)
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (graph.nodes.len(), graph.edges.len()),
            (nodes, edges),
            "{name}"
        );

        // Sanity check of the hand-built data (C1): every input point lies within
        // 5 m of the centerline of an edge that lists its route.
        for line in &fixture.lines {
            let centerlines: Vec<Vec<Point>> = graph
                .edges
                .iter()
                .filter(|e| e.lines.iter().any(|l| l.line == line.id))
                .map(|e| e.centerline(&graph.nodes))
                .collect();
            for &p in &line.points {
                let nearest = centerlines
                    .iter()
                    .flat_map(|c| c.windows(2).map(|w| distance_to_segment(p, w[0], w[1])))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    nearest <= 5.0,
                    "{name}, line {}: {p:?} is {nearest} m away",
                    line.id
                );
            }
        }
    }
}

#[test]
fn every_graph_file_matches_a_fixture() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let fixtures = load_dir(&dir).unwrap();
    let mut graphs = 0;
    for (path, fixture) in &fixtures {
        let graph_path = path.with_file_name(format!("{}.graph.json", fixture.name));
        if graph_path.is_file() {
            load_graph(&graph_path, fixture).unwrap_or_else(|e| panic!("{e}"));
            graphs += 1;
        }
    }
    let files = walk(&dir)
        .into_iter()
        .filter(|p| p.to_string_lossy().ends_with(".graph.json"));
    assert_eq!(
        files.count(),
        graphs,
        "a .graph.json file has no fixture next to it"
    );
}

fn walk(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(walk(&path));
        } else {
            out.push(path);
        }
    }
    out
}
