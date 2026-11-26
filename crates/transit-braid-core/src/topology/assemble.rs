//! Steps 3–4 of the build, from every sample's key to the graph ([`assemble`]): runs of equal
//! keys grouped into edges, each edge split where a route joins or leaves it
//! ([`split_at_run_ends`]), edge ends joined into nodes, and each route's walk through the
//! result.

use std::collections::{BTreeMap, BTreeSet};

use super::keys::{FLICKER_M, Key, Prepared, Sample, smooth_islands};
use super::path::{arc_along, direction_near, distance_to_segment, simplify};
use super::split::split_at_run_ends;
use super::{Dir, Params, TopoEdge, Topology, TopologyError, Traversal};
use crate::budget::{Bounds, Counter, Exceeded, Work};
use crate::geom::{Point, path_length};
use crate::ids::{EdgeId, IdVec, LineId, NodeId};

#[derive(Debug, Clone)]
pub(super) struct Run {
    pub(super) line: LineId,
    pub(super) key: Key,
    /// Sample range, inclusive.
    pub(super) first: usize,
    pub(super) last: usize,
}

/// Steps 3–9 from every sample's key (steps 1–2): flicker smoothing, runs, edges, nodes, walks.
/// `keys[line][s]` is the key of that route's sample `s`.
pub(super) fn assemble(
    prepared: &IdVec<LineId, &Prepared>,
    keys: &IdVec<LineId, Vec<&Key>>,
    params: &Params,
    bounds: &Bounds,
) -> Result<(Topology, IdVec<LineId, Traversal>), TopologyError> {
    let samples: IdVec<LineId, &[Sample]> = prepared.iter().map(|p| &p.samples[..]).collect();
    let samples = &samples;
    // The work budget counts the steps below whose number grows faster than the input.
    let mut steps = Counter::new(Work::GraphSteps, bounds.graph_steps);

    // 1–3: runs of equal keys, after smoothing (on references: the keys stay as they are).
    let mut runs = Vec::new();
    for (line, line_keys) in keys.iter_enumerated() {
        let mut keys: Vec<&Key> = line_keys.clone();
        smooth_islands(&mut keys, params.step_m, FLICKER_M, &mut steps)?;
        let mut first = 0;
        for i in 1..=keys.len() {
            if i == keys.len() || keys[i] != keys[first] {
                runs.push(Run {
                    line,
                    key: keys[first].clone(),
                    first,
                    last: i - 1,
                });
                first = i;
            }
        }
    }

    // 3: group runs with the same route set that lie on top of each other.
    let mut groups = UnionFind::new(runs.len());
    let mut by_set: BTreeMap<&BTreeSet<LineId>, Vec<usize>> = BTreeMap::new();
    for (i, r) in runs.iter().enumerate() {
        by_set.entry(&r.key.0).or_default().push(i);
    }
    for members in by_set.values() {
        for (x, &i) in members.iter().enumerate() {
            for &j in &members[x + 1..] {
                // Two stretches of one route share an edge only where it passes twice.
                let same_line = runs[i].line == runs[j].line;
                if same_line && !(runs[i].key.1 && runs[j].key.1) {
                    continue;
                }
                if overlaps(&runs[i], &runs[j], samples, params.merge_m, &mut steps)? {
                    groups.union(i, j);
                }
            }
        }
    }
    let mut edge_runs: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..runs.len() {
        edge_runs.entry(groups.find(i)).or_default().push(i);
    }

    // Each group: centerline = its longest run; members' directions relative to it.
    let mut protos = Vec::new();
    // Which edge each run became, and whether it runs along it (true) or against it.
    let mut run_edge: Vec<(usize, bool)> = vec![(0, true); runs.len()];
    for members in edge_runs.values() {
        let rep = *members
            .iter()
            .max_by_key(|&&i| (runs[i].last - runs[i].first, std::cmp::Reverse(i)))
            .unwrap_or(&members[0]);
        let r = &runs[rep];
        let mut path: Vec<Point> = samples[r.line][r.first..=r.last]
            .iter()
            .map(|s| s.at)
            .collect();
        if path.len() < 2 {
            path.push(path[0]); // a one-sample run: a zero-length edge, dropped below
        }
        let mut dirs: BTreeMap<LineId, Dir> = BTreeMap::new();
        for &i in members {
            let m = &runs[i];
            let forward = runs_along(&path, &samples[m.line][m.first..=m.last], params.step_m);
            run_edge[i] = (protos.len(), forward);
            add_dir(&mut dirs, m.line, forward);
        }
        protos.push(Proto {
            path,
            lines: dirs.into_iter().collect(),
        });
    }

    // 3b: split each edge where one of its runs starts or ends inside it.
    let (protos, spans) = split_at_run_ends(protos, &runs, &run_edge, samples, params.merge_m);

    // 4: nodes. Edge k has ends 2k (start) and 2k + 1 (end). Where one run of a route
    // ends and its next run starts is the same place on that route, so those ends are
    // always one node (continuity). Ends closer than merge_m join too
    // (the pieces of a split edge meet at the cut, so they join here).
    let ends: Vec<Point> = protos
        .iter()
        .flat_map(|p| [p.path[0], p.path[p.path.len() - 1]])
        .collect();
    let mut close = UnionFind::new(ends.len());
    let run_start = |i: usize| {
        let s = spans[i];
        if let Some(end) = s.at {
            return end;
        }
        if s.forward {
            2 * s.first
        } else {
            2 * s.last + 1
        }
    };
    let run_end = |i: usize| {
        let s = spans[i];
        if let Some(end) = s.at {
            return end;
        }
        if s.forward {
            2 * s.last + 1
        } else {
            2 * s.first
        }
    };
    for i in 1..runs.len() {
        // Two runs in a row on the same edge, the same way, are one stretch: no node between.
        if runs[i - 1].line == runs[i].line && run_edge[i - 1] != run_edge[i] {
            close.union(run_end(i - 1), run_start(i));
        }
    }
    let n = ends.len() as u64;
    steps.add(n.saturating_mul(n.saturating_sub(1)) / 2)?;
    for i in 0..ends.len() {
        for j in i + 1..ends.len() {
            if ends[i].distance(ends[j]) <= params.merge_m {
                close.union(i, j);
            }
        }
    }
    // Node per union-find root. Every end is visited, so every root gets a node.
    let mut node_of_root: Vec<Option<NodeId>> = vec![None; ends.len()];
    let mut sums: IdVec<NodeId, (Point, f64)> = IdVec::new();
    let mut node_of_end: Vec<NodeId> = Vec::with_capacity(ends.len());
    for (i, p) in ends.iter().enumerate() {
        let root = close.find(i);
        let n = *node_of_root[root].get_or_insert_with(|| sums.push((Point::new(0.0, 0.0), 0.0)));
        sums[n].0 = Point::new(sums[n].0.x + p.x, sums[n].0.y + p.y);
        sums[n].1 += 1.0;
        node_of_end.push(n);
    }
    let mut nodes: IdVec<NodeId, Point> = sums
        .iter()
        .map(|(s, n)| Point::new(s.x / n, s.y / n))
        .collect();

    let mut edges: IdVec<EdgeId, TopoEdge> = IdVec::new();
    // The final edges each piece became, in its path order: none (dropped), one, or two (a loop
    // split at its middle).
    let mut edges_of: Vec<Vec<EdgeId>> = vec![Vec::new(); protos.len()];
    for (k, p) in protos.into_iter().enumerate() {
        if p.lines.is_empty() {
            continue; // a piece no run covers (not expected: the longest run covers them all)
        }
        let (from, to) = (node_of_end[2 * k], node_of_end[2 * k + 1]);
        if from == to {
            // Both ends became one node. Up to 4 × merge_m (the "a bend, not a second pass"
            // length of `key`): a set-change artifact, dropped. Longer: a real loop (a circular
            // route), split at its middle so no edge is a self-loop and no route is lost.
            let n = p.path.len();
            if path_length(&p.path) <= 4.0 * params.merge_m || n < 3 {
                continue;
            }
            let mid = nodes.push(p.path[n / 2]);
            for (a, b, part) in [(from, mid, &p.path[..=n / 2]), (mid, to, &p.path[n / 2..])] {
                let e = edges.push(edge(a, b, part, &nodes, p.lines.clone()));
                edges_of[k].push(e);
            }
            continue;
        }
        let e = edges.push(edge(from, to, &p.path, &nodes, p.lines));
        edges_of[k].push(e);
    }

    // Each route's runs, in order along it, become its walk through the graph.
    let mut traversals: IdVec<LineId, Traversal> = vec![Vec::new(); keys.len()].into();
    for (i, r) in runs.iter().enumerate() {
        let s = spans[i];
        if s.at.is_some() {
            continue; // a run at a point covers no edge: the walk goes on through that node
        }
        let dir = if s.forward {
            Dir::Forward
        } else {
            Dir::Backward
        };
        let pieces: Vec<usize> = if s.forward {
            (s.first..=s.last).collect()
        } else {
            (s.first..=s.last).rev().collect()
        };
        for k in pieces {
            let mut on = edges_of[k].clone();
            if !s.forward {
                on.reverse();
            }
            for e in on {
                // A piece's lines are the runs that cover its middle: skip one this run only grazes.
                if !edges[e].lines.iter().any(|&(l, _)| l == r.line) {
                    continue;
                }
                let walk = &mut traversals[r.line];
                if walk.last() != Some(&(e, dir)) {
                    walk.push((e, dir));
                }
            }
        }
    }
    Ok((Topology { nodes, edges }, traversals))
}

/// Records that `line` runs along a piece (`forward`) or against it: a line that runs both ways
/// is `Both`.
pub(super) fn add_dir(dirs: &mut BTreeMap<LineId, Dir>, line: LineId, forward: bool) {
    let d = if forward { Dir::Forward } else { Dir::Backward };
    dirs.entry(line)
        .and_modify(|old| {
            if *old != d {
                *old = Dir::Both;
            }
        })
        .or_insert(d);
}

/// An edge from node `from` to node `to` along `path` (simplified; its ends set to the nodes).
fn edge(
    from: NodeId,
    to: NodeId,
    path: &[Point],
    nodes: &IdVec<NodeId, Point>,
    lines: Vec<(LineId, Dir)>,
) -> TopoEdge {
    let mut path = simplify(path, SIMPLIFY_M);
    let last = path.len() - 1; // simplify keeps both ends, so len >= 2
    path[0] = nodes[from];
    path[last] = nodes[to];
    TopoEdge {
        from,
        to,
        path,
        lines,
    }
}

/// Whether a run (its `samples`, in order) runs along `path`, from its start toward its end. Decided
/// by where its first and last samples fall along the path; only when those are within `step`
/// of each other (a run about one sample long) does the direction at its middle sample decide. The
/// direction at one sample is ambiguous at a sharp corner: a run whose middle sample sat on one once
/// read as backward, its ends were joined to the wrong nodes, and a node landed 70 m off both
/// lines. An empty run counts as forward.
pub(super) fn runs_along(path: &[Point], samples: &[Sample], step: f64) -> bool {
    let (Some(first), Some(last)) = (samples.first(), samples.last()) else {
        return true;
    };
    let (a, b) = (arc_along(path, first.at), arc_along(path, last.at));
    if (b - a).abs() > step {
        return b > a;
    }
    let mid = samples[samples.len() / 2];
    let along = direction_near(path, mid.at);
    mid.dir.0 * along.0 + mid.dir.1 * along.1 >= 0.0
}

/// Centerlines are simplified with this tolerance (meters) before they are returned.
const SIMPLIFY_M: f64 = 0.5;

/// An edge before its ends are turned into nodes.
pub(super) struct Proto {
    pub(super) path: Vec<Point>,
    pub(super) lines: Vec<(LineId, Dir)>,
}

/// Whether two runs lie on top of each other: either one's middle is within `merge_m` of the
/// other's path. `steps` counts the path segments read, for the work budget.
fn overlaps(
    a: &Run,
    b: &Run,
    samples: &IdVec<LineId, &[Sample]>,
    merge_m: f64,
    steps: &mut Counter,
) -> Result<bool, Exceeded> {
    let path = |r: &Run| -> Vec<Point> {
        samples[r.line][r.first..=r.last]
            .iter()
            .map(|s| s.at)
            .collect()
    };
    let mid = |r: &Run| samples[r.line][(r.first + r.last) / 2].at;
    let mut read = 0_u64;
    let mut near = |p: Point, path: &[Point]| {
        if path.len() == 1 {
            read += 1;
            return p.distance(path[0]) <= merge_m;
        }
        path.windows(2).any(|w| {
            read += 1;
            distance_to_segment(p, w[0], w[1]) <= merge_m
        })
    };
    let found = near(mid(a), &path(b)) || near(mid(b), &path(a));
    steps.add(read)?;
    Ok(found)
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }

    fn find(&mut self, i: usize) -> usize {
        let mut r = i;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut j = i;
        while self.0[j] != r {
            let next = self.0[j];
            self.0[j] = r;
            j = next;
        }
        r
    }

    /// Joins two sets; the smaller root wins, so results don't depend on call order.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        let (lo, hi) = (ra.min(rb), ra.max(rb));
        self.0[hi] = lo;
    }
}
