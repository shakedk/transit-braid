//! Step 3b of the build: an edge is cut where one of its routes joins or leaves it while the
//! others carry on ([`split_at_run_ends`]), and each run gets its place on the pieces ([`Span`]).

use std::collections::BTreeMap;

use super::Dir;
use super::assemble::{Proto, Run, add_dir};
use super::keys::Sample;
use super::path::{PathIndex, sub_path};
use crate::geom::path_length;
use crate::ids::{IdVec, LineId};

/// Where a run sits on its (split) edge: its first and last piece, and whether it runs along
/// the path. `at`: a run whose whole reach lies within `merge_m` of one cut or path end (a few
/// meters where a route briefly meets the edge) sits at that point: its start and end are that
/// piece end (2 × piece, or 2 × piece + 1), and it covers no piece.
#[derive(Debug, Clone, Copy)]
pub(super) struct Span {
    pub(super) first: usize,
    pub(super) last: usize,
    pub(super) forward: bool,
    pub(super) at: Option<usize>,
}

/// Step 3b: splits an edge where one of its routes joins or leaves it while the edge's
/// other routes carry on: where a run starts or ends farther than `merge_m` from both ends of
/// its edge, and that route's run before (or after) it is on another edge. Example: a route
/// that goes around a block and comes back to the same point, while the others go straight on.
/// Without the split, continuity (step 4) would join the edge's two far ends to that point and
/// put one node at their average, off the street. A route's start or end inside an edge, or a
/// turn back onto the same edge, is not cut (as before). Cuts closer than `merge_m` to each other
/// become one (their mean). Each piece carries the routes whose runs cover its middle.
/// Returns the pieces (edge by edge, each edge's pieces in path order) and each run's span.
pub(super) fn split_at_run_ends(
    protos: Vec<Proto>,
    runs: &[Run],
    run_edge: &[(usize, bool)],
    samples: &IdVec<LineId, &[Sample]>,
    merge_m: f64,
) -> (Vec<Proto>, Vec<Span>) {
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); protos.len()];
    for (i, &(k, _)) in run_edge.iter().enumerate() {
        members[k].push(i);
    }
    // Whether the same route's previous / next run is on another edge.
    let other_edge =
        |i: usize, j: usize| runs[j].line == runs[i].line && run_edge[j].0 != run_edge[i].0;
    // Whether run `i` is its route's first (or last) run, and the route has another run on
    // the same edge: a loop line that starts and ends on a street it shares. Each of its
    // runs covers only its own part of the edge, so the edge is cut where the route starts or
    // ends.
    let again_here = |i: usize| {
        members[run_edge[i].0]
            .iter()
            .any(|&j| j != i && runs[j].line == runs[i].line)
    };
    let starts_line = |i: usize| i == 0 || runs[i - 1].line != runs[i].line;
    let ends_line = |i: usize| i + 1 == runs.len() || runs[i + 1].line != runs[i].line;
    let mut out = Vec::with_capacity(protos.len());
    let mut spans = vec![
        Span {
            first: 0,
            last: 0,
            forward: true,
            at: None,
        };
        runs.len()
    ];
    for (k, p) in protos.into_iter().enumerate() {
        let total = path_length(&p.path);
        let index = PathIndex::new(&p.path, 2.0 * merge_m);
        let at = |i: usize, s: usize| index.arc(samples[runs[i].line][s].at);
        let mut found: Vec<f64> = Vec::new();
        for &i in &members[k] {
            if (i > 0 && other_edge(i, i - 1)) || (starts_line(i) && again_here(i)) {
                found.push(at(i, runs[i].first));
            }
            if (i + 1 < runs.len() && other_edge(i, i + 1)) || (ends_line(i) && again_here(i)) {
                found.push(at(i, runs[i].last));
            }
        }
        found.retain(|&a| a > merge_m && a < total - merge_m);
        // A run's reach along the path: over its samples (every 5th, and both ends), since a run
        // can go out and come back.
        let reach = |i: usize| {
            let r = &runs[i];
            (r.first..=r.last)
                .step_by(5)
                .chain([r.last])
                .map(|s| at(i, s))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), a| {
                    (lo.min(a), hi.max(a))
                })
        };
        if found.is_empty() {
            // One piece. A run within merge_m of one end is a point there (as below); the
            // edge keeps only the routes that also run along it, in the ways those runs go.
            let mut along: BTreeMap<LineId, Dir> = BTreeMap::new();
            for &i in &members[k] {
                // Only a run that goes on to (or comes from) another edge: there continuity would
                // join that edge to this one's far end.
                let joins =
                    (i > 0 && other_edge(i, i - 1)) || (i + 1 < runs.len() && other_edge(i, i + 1));
                // Its two ends first: a point needs both by one end. Most runs fail here, so the
                // whole reach (every 5th sample against the path) is read only when it can be one.
                let (a, b) = (at(i, runs[i].first), at(i, runs[i].last));
                let may_start = a <= merge_m && b <= merge_m;
                let may_end = a >= total - merge_m && b >= total - merge_m;
                let point = if joins && (may_start || may_end) {
                    let (lo, hi) = reach(i);
                    if hi <= merge_m {
                        Some(2 * out.len())
                    } else if lo >= total - merge_m {
                        Some(2 * out.len() + 1)
                    } else {
                        None
                    }
                } else {
                    None
                };
                if point.is_none() {
                    add_dir(&mut along, runs[i].line, run_edge[i].1);
                }
                spans[i] = Span {
                    first: out.len(),
                    last: out.len(),
                    forward: run_edge[i].1,
                    at: point,
                };
            }
            let mut p = p;
            p.lines = along.into_iter().collect();
            out.push(p);
            continue;
        }
        found.sort_by(f64::total_cmp);
        // Cuts closer than merge_m to the previous one join its group; each group's mean.
        let mut groups: Vec<(f64, f64, f64)> = Vec::new(); // (sum, count, last)
        for a in found {
            match groups.last_mut() {
                Some(g) if a - g.2 <= merge_m => *g = (g.0 + a, g.1 + 1.0, a),
                _ => groups.push((a, 1.0, a)),
            }
        }
        let mut bounds = vec![0.0];
        bounds.extend(groups.iter().map(|g| g.0 / g.1));
        bounds.push(total);
        let pieces = bounds.len() - 1;
        let mid = |q: usize| f64::midpoint(bounds[q], bounds[q + 1]);
        // A reach end within merge_m of a bound sits on it.
        let snap = |a: f64| {
            bounds
                .iter()
                .copied()
                .find(|&b| (a - b).abs() <= merge_m)
                .unwrap_or(a)
        };
        let mut dirs: Vec<BTreeMap<LineId, Dir>> = vec![BTreeMap::new(); pieces];
        for &i in &members[k] {
            let r = &runs[i];
            let (lo, hi) = reach(i);
            let (raw_lo, raw_hi) = (lo, hi);
            let (lo, hi) = (snap(lo), snap(hi));
            let mut covered = (0..pieces).filter(|&q| lo <= mid(q) && mid(q) <= hi);
            let first = covered.next();
            let last = covered.next_back().or(first);
            let (first, last) = match (first, last) {
                (Some(f), Some(l)) => (f, l),
                _ => {
                    // A run that covers no piece's middle, with an end within merge_m of a
                    // bound (a cut or a path end): a few meters where the route meets this edge
                    // there. It is a point at that bound. Given a piece instead, one of its ends
                    // would be that piece's far end, and continuity would join the route's next
                    // run to it, off the street.
                    let bound_of = |a: f64| bounds.iter().position(|&b| (a - b).abs() <= merge_m);
                    if let Some(b) = bound_of(raw_lo).or_else(|| bound_of(raw_hi)) {
                        let end = if b == pieces {
                            2 * (out.len() + pieces - 1) + 1
                        } else {
                            2 * (out.len() + b)
                        };
                        spans[i] = Span {
                            first: out.len(),
                            last: out.len(),
                            forward: run_edge[i].1,
                            at: Some(end),
                        };
                        continue;
                    }
                    // Otherwise (a run inside a piece, away from its bounds): the piece under its
                    // own middle, from its unsnapped reach.
                    let m = f64::midpoint(raw_lo, raw_hi);
                    let q = (0..pieces)
                        .find(|&q| m < bounds[q + 1])
                        .unwrap_or(pieces - 1);
                    (q, q)
                }
            };
            let forward = run_edge[i].1;
            spans[i] = Span {
                first: out.len() + first,
                last: out.len() + last,
                forward,
                at: None,
            };
            for piece in &mut dirs[first..=last] {
                add_dir(piece, r.line, forward);
            }
        }
        for (q, lines) in dirs.into_iter().enumerate() {
            out.push(Proto {
                path: sub_path(&p.path, bounds[q], bounds[q + 1]),
                lines: lines.into_iter().collect(),
            });
        }
    }
    (out, spans)
}
