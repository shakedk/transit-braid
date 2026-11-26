//! Steps 0–2 of the build, for each route on its own: spikes removed ([`despike`]), samples
//! every [`Params::step_m`] ([`Prepared`]), and each sample's key ([`key`]): which routes run
//! alongside it there. Also the flicker smoothing that cleans up those keys before step 3 cuts
//! them into runs ([`smooth_islands`]).

use std::collections::BTreeSet;

use super::Params;
use super::grid::{Grid, Seg};
use super::path::distance_to_segment;
use crate::budget::{Counter, Exceeded};
use crate::geom::Point;
use crate::ids::LineId;

#[derive(Debug, Clone, Copy)]
pub(super) struct Sample {
    pub(super) at: Point,
    pub(super) dir: (f64, f64),
    /// Distance along its route.
    pub(super) arc: f64,
}

/// (routes here, passes here twice)
pub(super) type Key = (BTreeSet<LineId>, bool);

/// One route as the topology sees it, which depends on nothing else: spikes removed (step 0)
/// and sampled (step 1).
#[derive(Debug, Clone)]
pub(super) struct Prepared {
    pub(super) line: Vec<Point>,
    pub(super) samples: Vec<Sample>,
}

impl Prepared {
    pub(super) fn new(line: &[Point], params: &Params) -> Self {
        let line = despike(line, params.merge_m);
        let samples = sample(&line, params.step_m);
        Self { line, samples }
    }
}

/// Runs shorter than this (meters) between two runs with the same key are flicker
/// ([`smooth_islands`]).
pub(super) const FLICKER_M: f64 = 20.0;

/// Flicker: a run of samples shorter than `min_m` whose neighbors on both sides have the
/// same key takes that key (another line touching the bundle for a few meters, e.g. while
/// turning, is not a real change). Repeats until nothing changes; ends of the line are kept.
/// Each pass reads every key, so `steps` counts them for the work budget (two routes weaving
/// across each other make one pass per crossing).
pub(super) fn smooth_islands<T: PartialEq + Clone>(
    keys: &mut [T],
    step_m: f64,
    min_m: f64,
    steps: &mut Counter,
) -> Result<(), Exceeded> {
    loop {
        steps.add(keys.len() as u64)?;
        // Runs as (first, last) sample index, inclusive.
        let mut runs = Vec::new();
        let mut first = 0;
        for i in 1..=keys.len() {
            if i == keys.len() || keys[i] != keys[first] {
                runs.push((first, i - 1));
                first = i;
            }
        }
        let island = runs.windows(3).find(|w| {
            let (a, b, c) = (w[0], w[1], w[2]);
            (b.1 - b.0 + 1) as f64 * step_m < min_m && keys[a.0] == keys[c.0]
        });
        let Some(w) = island else { return Ok(()) };
        let fill = keys[w[0].0].clone();
        for k in &mut keys[w[1].0..=w[1].1] {
            *k = fill.clone();
        }
    }
}

/// A spike's legs may be at most this many merge distances long (the same length below which
/// a route coming back is a bend, not a second pass; see `key`).
const SPIKE_LEGS: f64 = 4.0;

/// Removes spikes: a point whose neighbors are within `merge_m` of each other and at most
/// `SPIKE_LEGS` (4) × `merge_m` away from it (the route goes out a little and comes straight
/// back). Repeats until none is left; keeps both ends. Without this, a spike in the middle of
/// a shared stretch cut the stretch in two, and both halves landed on one edge whose ends then
/// became one node, so the edge was dropped (Dublin, route 27). A route that
/// would lose all its length is returned unchanged.
///
/// Bad input is returned unchanged: `merge_m` not a finite number above 0, or any point not
/// finite.
pub(crate) fn despike(line: &[Point], merge_m: f64) -> Vec<Point> {
    let finite = |p: &Point| p.x.is_finite() && p.y.is_finite();
    if !(merge_m.is_finite() && merge_m > 0.0) || !line.iter().all(finite) {
        return line.to_vec();
    }
    let mut out: Vec<Point> = Vec::with_capacity(line.len());
    for &p in line {
        out.push(p);
        while let [.., a, b, c] = out[..] {
            let legs = a.distance(b).max(b.distance(c));
            if a.distance(c) > merge_m || legs > SPIKE_LEGS * merge_m {
                break;
            }
            out.remove(out.len() - 2);
        }
    }
    if out.windows(2).all(|w| w[0].distance(w[1]) == 0.0) {
        return line.to_vec();
    }
    out
}

/// The key of one sample: which routes run parallel within `merge_m`, and whether its own
/// route passes here again (farther than 4 × `merge_m` along it, so a bend isn't a pass). With
/// it, how many segments it looked at (the work budget's nearby checks).
pub(super) fn key(
    line: LineId,
    s: &Sample,
    segs: &[Seg],
    grid: &Grid,
    merge_m: f64,
    min_cos: f64,
) -> (Key, u32) {
    let mut set = BTreeSet::from([line]);
    let mut twice = false;
    let near = grid.near(s.at);
    let checks = u32::try_from(near.len()).unwrap_or(u32::MAX);
    for i in near {
        let g = &segs[i];
        let cos = s.dir.0 * g.dir.0 + s.dir.1 * g.dir.1;
        if cos.abs() < min_cos || distance_to_segment(s.at, g.a, g.b) > merge_m {
            continue;
        }
        if g.line != line {
            set.insert(g.line);
        } else if (s.arc < g.arc_a - 4.0 * merge_m) || (s.arc > g.arc_b + 4.0 * merge_m) {
            twice = true;
        }
    }
    ((set, twice), checks)
}

/// Samples every `step` along `line` (from its first point; the last point is always
/// included), skipping zero-length segments.
fn sample(line: &[Point], step: f64) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut next = 0.0;
    let mut walked = 0.0;
    let mut last_dir = (1.0, 0.0);
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        if len == 0.0 {
            continue;
        }
        let dir = ((w[1].x - w[0].x) / len, (w[1].y - w[0].y) / len);
        while next <= walked + len {
            let t = (next - walked) / len;
            out.push(Sample {
                at: Point::new(w[0].x + t * len * dir.0, w[0].y + t * len * dir.1),
                dir,
                arc: next,
            });
            next += step;
        }
        walked += len;
        last_dir = dir;
    }
    if let Some(&end) = line.last()
        && out.last().is_none_or(|s| s.at.distance(end) > 1e-9)
    {
        out.push(Sample {
            at: end,
            dir: last_dir,
            arc: walked,
        });
    }
    out
}
