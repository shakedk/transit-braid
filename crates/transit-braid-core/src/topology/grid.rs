//! The segment grid behind the keys: every route's segments ([`Seg`], [`segments`]) filed
//! under the square cells they cross ([`cells_crossed`]), so a sample looks only at the segments
//! near it ([`Grid`]); and, for an edit, the cells where a set of segments can change a key
//! ([`Reach`]).

use std::collections::HashMap;

use crate::geom::Point;
use crate::ids::LineId;

#[derive(Debug, Clone, Copy)]
pub(super) struct Seg {
    pub(super) line: LineId,
    pub(super) a: Point,
    pub(super) b: Point,
    pub(super) dir: (f64, f64),
    /// Distance along its route at `a` and `b`.
    pub(super) arc_a: f64,
    pub(super) arc_b: f64,
}

/// Where a set of segments can reach a sample's key: exactly the grid cells where [`Grid::near`]
/// finds one of them, which are every cell the grid files one under ([`cells_crossed`]) and every
/// cell next to one. A sample in any other cell finds none of them, so they can't change its key.
/// Marking each segment's whole bounding box instead would be a million cells for a 5 km diagonal.
pub(super) struct Reach {
    cell: f64,
    pub(super) cells: std::collections::HashSet<(i64, i64)>,
    /// The lowest and highest cell indices marked, a quick test before the set.
    lo: (i64, i64),
    hi: (i64, i64),
}

impl Reach {
    pub(super) fn new<'a>(segs: impl Iterator<Item = &'a Seg>, cell: f64) -> Self {
        let mut cells = std::collections::HashSet::new();
        let (mut lo, mut hi) = ((i64::MAX, i64::MAX), (i64::MIN, i64::MIN));
        for s in segs {
            cells_crossed(s.a, s.b, cell, |(cx, cy)| {
                for x in cx - 1..=cx + 1 {
                    for y in cy - 1..=cy + 1 {
                        cells.insert((x, y));
                    }
                }
                lo = (lo.0.min(cx - 1), lo.1.min(cy - 1));
                hi = (hi.0.max(cx + 1), hi.1.max(cy + 1));
            });
        }
        Self {
            cell,
            cells,
            lo,
            hi,
        }
    }

    /// Whether [`Grid::near`] at `p` finds one of the segments (its cell is computed the same way).
    pub(super) fn near(&self, p: Point) -> bool {
        let k = (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        );
        k.0 >= self.lo.0
            && k.0 <= self.hi.0
            && k.1 >= self.lo.1
            && k.1 <= self.hi.1
            && self.cells.contains(&k)
    }
}

pub(super) fn segments<'a>(lines: impl Iterator<Item = &'a [Point]>) -> Vec<Seg> {
    let mut segs = Vec::new();
    for (line, pts) in lines.enumerate() {
        let mut arc = 0.0;
        for w in pts.windows(2) {
            let len = w[0].distance(w[1]);
            if len > 0.0 {
                segs.push(Seg {
                    line: LineId::new(line),
                    a: w[0],
                    b: w[1],
                    dir: ((w[1].x - w[0].x) / len, (w[1].y - w[0].y) / len),
                    arc_a: arc,
                    arc_b: arc + len,
                });
            }
            arc += len;
        }
    }
    segs
}

/// Segments bucketed in square cells of `cell` meters (a segment is in every cell its bounding
/// box touches), so everything within `cell` of a point is in its 3 × 3 neighborhood.
pub(super) struct Grid {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<usize>>,
}

impl Grid {
    pub(super) fn new(segs: &[Seg], cell: f64) -> Self {
        let mut cells: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, s) in segs.iter().enumerate() {
            cells_crossed(s.a, s.b, cell, |k| cells.entry(k).or_default().push(i));
        }
        Self { cell, cells }
    }

    /// Indices of segments that may be within `cell` of `p` (each once, sorted).
    pub(super) fn near(&self, p: Point) -> Vec<usize> {
        let (cx, cy) = (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        );
        let mut out: Vec<usize> = (cx - 1..=cx + 1)
            .flat_map(|x| (cy - 1..=cy + 1).map(move |y| (x, y)))
            .filter_map(|k| self.cells.get(&k))
            .flatten()
            .copied()
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Calls `f` once for every grid cell (side `cell`, cell `(x, y)` covering `x·cell` to
/// `(x + 1)·cell`) that the segment `a`–`b` passes through, plus at most a few next to them
/// (never fewer). A long diagonal segment fills a strip of cells, not its whole bounding box
/// (a 1 km diagonal: about 400 cells, not 40 000).
///
/// Enough for [`Grid::near`]: a point within `cell` of the segment is within `cell` of one of its
/// points, whose cell is one of the nine around the point's own. Plain arithmetic and `floor`
/// only, so native and WebAssembly builds agree to the bit.
pub(super) fn cells_crossed(a: Point, b: Point, cell: f64, mut f: impl FnMut((i64, i64))) {
    let c = |v: f64| (v / cell).floor() as i64;
    // Far above rounding error at city coordinates, far below a cell: a point of the segment
    // this close to a cell's side also marks the cell across it.
    let eps = cell * 1e-6;
    let (lo, hi) = if a.x <= b.x { (a, b) } else { (b, a) };
    let dx = hi.x - lo.x;
    let y_at = |x: f64| {
        if dx == 0.0 {
            lo.y
        } else {
            lo.y + (x - lo.x) / dx * (hi.y - lo.y)
        }
    };
    for cx in c(lo.x - eps)..=c(hi.x + eps) {
        // The part of the segment in this column, widened by eps on both sides.
        let x0 = (cx as f64 * cell - eps).max(lo.x);
        let x1 = ((cx + 1) as f64 * cell + eps).min(hi.x);
        let (y0, y1) = if dx == 0.0 {
            (lo.y.min(hi.y), lo.y.max(hi.y))
        } else {
            let (p, q) = (y_at(x0), y_at(x1));
            (p.min(q), p.max(q))
        };
        for cy in c(y0 - eps)..=c(y1 + eps) {
            f((cx, cy));
        }
    }
}
