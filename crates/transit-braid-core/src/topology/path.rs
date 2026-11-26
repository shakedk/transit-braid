//! Measuring along a path: where the point nearest to another lies along it ([`arc_along`], and
//! [`PathIndex`] for many points on one long path), the piece between two distances
//! ([`sub_path`]), its direction near a point, Douglas–Peucker simplification
//! ([`simplify`]), and a point's distance to a segment.

use std::collections::HashMap;

use super::grid::cells_crossed;
use crate::geom::Point;

/// The point of segment `a`–`b` nearest to `p`: its distance from `p`, how far along the segment
/// it is as a fraction `t` (0 for a zero-length segment), and the segment's length.
///
/// Not [`distance_to_segment`]: that one divides by `dx² + dy²`, this one by `len · len` with
/// `len` a square root, so their last bits differ, and each keeps the results its callers had.
fn nearest_on_segment(p: Point, a: Point, b: Point) -> (f64, f64, f64) {
    let len = a.distance(b);
    let t = if len == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / (len * len)).clamp(0.0, 1.0)
    };
    let d = p.distance(Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)));
    (d, t, len)
}

/// Distance along `path` to its point nearest to `p`.
pub(super) fn arc_along(path: &[Point], p: Point) -> f64 {
    let (mut best, mut at, mut start) = (f64::INFINITY, 0.0, 0.0);
    for w in path.windows(2) {
        let (d, t, len) = nearest_on_segment(p, w[0], w[1]);
        if d < best {
            (best, at) = (d, start + t * len);
        }
        start += len;
    }
    at
}

/// [`arc_along`] for many points on one long path: its segments in a grid, so a point looks
/// only at the segments near it. Gives exactly `arc_along`'s answer: the same arithmetic on the
/// same segments, and of equally near ones the first along the path.
pub(super) struct PathIndex<'a> {
    path: &'a [Point],
    /// Distance along the path to each segment's start, summed as `arc_along` sums it.
    starts: Vec<f64>,
    cell: f64,
    /// Segment indices per cell, in path order; empty for a short path (then a plain scan).
    cells: HashMap<(i64, i64), Vec<usize>>,
    /// The cells' bounds, so a search knows when it has seen them all.
    lo: (i64, i64),
    hi: (i64, i64),
}

impl<'a> PathIndex<'a> {
    /// Paths shorter than this many points are scanned as they are.
    const MIN_POINTS: usize = 32;

    pub(super) fn new(path: &'a [Point], cell: f64) -> Self {
        let mut starts = Vec::with_capacity(path.len());
        let mut start = 0.0;
        for w in path.windows(2) {
            starts.push(start);
            start += w[0].distance(w[1]);
        }
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        let (mut lo, mut hi) = ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN));
        if path.len() >= Self::MIN_POINTS {
            for (i, w) in path.windows(2).enumerate() {
                cells_crossed(w[0], w[1], cell, |(x, y)| {
                    lo = (lo.0.min(x), lo.1.min(y));
                    hi = (hi.0.max(x), hi.1.max(y));
                    let list = cells.entry((x, y)).or_default();
                    if list.last() != Some(&i) {
                        list.push(i);
                    }
                });
            }
        }
        Self {
            path,
            starts,
            cell,
            cells,
            lo,
            hi,
        }
    }

    /// Distance along the path to its point nearest to `p` (as [`arc_along`]).
    pub(super) fn arc(&self, p: Point) -> f64 {
        if self.cells.is_empty() {
            return arc_along(self.path, p);
        }
        // Each segment as arc_along measures it: (distance to p, distance along the path).
        let measure = |i: usize| {
            let (d, t, len) = nearest_on_segment(p, self.path[i], self.path[i + 1]);
            (d, self.starts[i] + t * len)
        };
        let c = |v: f64| (v / self.cell).floor() as i64;
        let (px, py) = (c(p.x), c(p.y));
        // (distance, segment, arc): the nearest so far, the first along the path on a tie.
        let mut best: Option<(f64, usize, f64)> = None;
        // Far above rounding error, far below a cell.
        let margin = self.cell * 1e-6;
        for r in 0i64.. {
            // A segment in no cell of the rings seen so far (0 to r - 1) is more than r - 1 cells
            // from p: once the best is nearer, none of them can match it.
            if best.is_some_and(|(d, _, _)| d < (r - 1) as f64 * self.cell - margin) {
                break;
            }
            let all_seen = px - r <= self.lo.0
                && py - r <= self.lo.1
                && px + r >= self.hi.0
                && py + r >= self.hi.1;
            for x in px - r..=px + r {
                let ys: Vec<i64> = if x == px - r || x == px + r {
                    (py - r..=py + r).collect()
                } else {
                    vec![py - r, py + r]
                };
                for y in ys {
                    for &i in self.cells.get(&(x, y)).into_iter().flatten() {
                        let (d, arc) = measure(i);
                        if best.is_none_or(|(bd, bi, _)| d.total_cmp(&bd).then(i.cmp(&bi)).is_lt())
                        {
                            best = Some((d, i, arc));
                        }
                    }
                }
            }
            if all_seen {
                break;
            }
        }
        best.map_or(0.0, |(_, _, arc)| arc)
    }
}

/// The part of `path` from `a` to `b` meters along it (`a < b`); always at least two points.
///
/// Not the geometry's `sub_path`: this one keeps repeated points and never returns fewer than
/// two, which the edge split's pieces rely on; that one drops repeated points. Merging them would
/// change one of the two outputs.
pub(super) fn sub_path(path: &[Point], a: f64, b: f64) -> Vec<Point> {
    let mut out = Vec::new();
    let mut start = 0.0;
    for w in path.windows(2) {
        let len = w[0].distance(w[1]);
        let end = start + len;
        let at = |s: f64| {
            let t = if len == 0.0 {
                0.0
            } else {
                ((s - start) / len).clamp(0.0, 1.0)
            };
            Point::new(
                w[0].x + t * (w[1].x - w[0].x),
                w[0].y + t * (w[1].y - w[0].y),
            )
        };
        if end >= a && start <= b {
            if out.is_empty() {
                out.push(at(a));
            }
            if end < b {
                out.push(w[1]);
            } else {
                out.push(at(b));
                break;
            }
        }
        start = end;
    }
    if out.is_empty() {
        out.push(path[path.len() - 1]);
    }
    if out.len() < 2 {
        out.push(out[0]);
    }
    out
}

/// Douglas–Peucker: drops points within `tol` of the line between the points kept around them.
/// Keeps the first and last point.
pub(super) fn simplify(points: &[Point], tol: f64) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (mut far, mut far_d) = (a, 0.0);
        for i in a + 1..b {
            let d = distance_to_segment(points[i], points[a], points[b]);
            if d > far_d {
                far = i;
                far_d = d;
            }
        }
        if far_d > tol {
            keep[far] = true;
            stack.push((a, far));
            stack.push((far, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(&p, k)| k.then_some(p))
        .collect()
}

/// Unit direction of the path segment nearest to `p`.
pub(super) fn direction_near(path: &[Point], p: Point) -> (f64, f64) {
    let w = path
        .windows(2)
        .min_by(|x, y| {
            distance_to_segment(p, x[0], x[1]).total_cmp(&distance_to_segment(p, y[0], y[1]))
        })
        .unwrap_or(&path[..2]);
    let len = w[0].distance(w[1]).max(f64::MIN_POSITIVE);
    ((w[1].x - w[0].x) / len, (w[1].y - w[0].y) / len)
}

pub(crate) fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq).clamp(0.0, 1.0);
    p.distance(Point::new(a.x + t * dx, a.y + t * dy))
}
