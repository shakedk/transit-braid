//! Cutting small loops out of a drawn line: the loop a connector can make when it turns a line
//! back over its own way in ([`cut_node_loop`]), and the folds offsetting makes on the inside of
//! tight bends ([`without_offset_loops`]).

use crate::geom::Point;

/// How far from a node's connector (in node fronts) a loop through it may reach and still be cut.
pub(super) const NODE_LOOP_REACH_FRONTS: f64 = 1.0;

/// A node's small loop: a connector that turns a line back on itself (a
/// U-turn into a slot on the other side) can make it cross its own way in. `line[from..=to]` is
/// the connector (from the incoming piece's last point to the outgoing piece's first); a crossing
/// between a segment that starts before the connector's end and one that ends after its start,
/// both touching the stretch within `reach` meters (along the line) of the connector, and itself
/// within `reach` of the connector's ends, cuts the stretch between them to the crossing point
/// (the widest such loop first). Loops farther out are the route's own and stay. First and last
/// points are kept; cutting again changes nothing. Plain arithmetic only, so every platform cuts
/// alike.
pub(super) fn cut_node_loop(line: &mut Vec<Point>, mut from: usize, mut to: usize, reach: f64) {
    if !(reach.is_finite() && reach > 0.0) || from >= line.len() || to > line.len() || from >= to {
        return;
    }
    loop {
        let n = line.len();
        // The first point within `reach` before the connector, and the last after it.
        let mut start = from.min(n - 1);
        let mut along = 0.0;
        while start > 0 {
            along += line[start - 1].distance(line[start]);
            if along > reach {
                break;
            }
            start -= 1;
        }
        let mut end = to.saturating_sub(1).min(n - 1);
        along = 0.0;
        while end + 1 < n {
            along += line[end].distance(line[end + 1]);
            if along > reach {
                break;
            }
            end += 1;
        }
        // Segments touching the stretch: the one arriving at `start` and the one leaving `end`.
        let first = start.saturating_sub(1);
        let last = end.min(n.saturating_sub(2));
        let mut cut = None;
        'search: for i in first..to.min(last + 1) {
            // Segment i (from point i) and a later segment j that ends after the connector starts.
            for j in (i + 2..=last).rev() {
                if j < from {
                    break;
                }
                if let Some(x) = crossing(line[i], line[i + 1], line[j], line[j + 1])
                    && (x.distance(line[from]) <= reach || x.distance(line[to - 1]) <= reach)
                {
                    cut = Some((i, j, x));
                    break 'search;
                }
            }
        }
        let Some((i, j, x)) = cut else { return };
        // Points i + 1 ..= j become the crossing; the connector's range moves with them.
        drop(line.splice(i + 1..=j, [x]));
        let removed = j - i - 1;
        let moved = |k: usize| if k > j { k - removed } else { k.min(i + 1) };
        (from, to) = (moved(from), moved(to).max(moved(from) + 1));
    }
}

/// A loop longer than a full turn at its offset plus this (meters) is never cut.
pub(super) const LOOP_SLACK_M: f64 = 1.0;

/// `line`, offset by `offset` meters from `center`, without the small loops that
/// offsetting folds into it on the inside of bends tighter than the offset. Where two of its
/// segments cross and the stretch between them is at most a full turn at that offset
/// (`2π · |offset|` plus [`LOOP_SLACK_M`]) long, that stretch is replaced by the crossing point.
/// Anything longer is a real loop and stays, and so does every loop when `center` crosses itself
/// (the route's own) or when `offset` is 0 or not finite. The first and last points are kept,
/// and cutting again changes nothing.
/// Plain arithmetic only, so every platform cuts alike.
pub(super) fn without_offset_loops(line: Vec<Point>, center: &[Point], offset: f64) -> Vec<Point> {
    if offset == 0.0 || !offset.is_finite() || line.len() < 4 || crosses_itself(center) {
        return line;
    }
    let max_loop = std::f64::consts::TAU * offset.abs() + LOOP_SLACK_M;
    let mut pts = line;
    // Passes until one cuts nothing: a shorter line can bring an earlier loop within reach.
    loop {
        let mut cut_any = false;
        let mut i = 0;
        while i + 2 < pts.len() {
            let (a, b) = (pts[i], pts[i + 1]);
            let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
            let (bottom, top) = (a.y.min(b.y), a.y.max(b.y));
            let mut arc = 0.0;
            let mut cut = None;
            for j in i + 2..pts.len() - 1 {
                arc += pts[j - 1].distance(pts[j]);
                if arc > max_loop {
                    break;
                }
                let (c, d) = (pts[j], pts[j + 1]);
                // Only segments whose boxes overlap can cross.
                if c.x.max(d.x) < lo
                    || c.x.min(d.x) > hi
                    || c.y.max(d.y) < bottom
                    || c.y.min(d.y) > top
                {
                    continue;
                }
                if let Some(x) = crossing(a, b, c, d) {
                    cut = Some((j, x));
                    break;
                }
            }
            match cut {
                // Points i + 1 ..= j become the crossing; segment i may cross again further on.
                Some((j, x)) => {
                    drop(pts.splice(i + 1..=j, [x]));
                    cut_any = true;
                }
                None => i += 1,
            }
        }
        if !cut_any {
            return pts;
        }
    }
}

/// Whether two segments of `path` that don't share a point cross each other.
pub(super) fn crosses_itself(path: &[Point]) -> bool {
    let n = path.len();
    (0..n.saturating_sub(1)).any(|i| {
        (i + 2..n.saturating_sub(1))
            .any(|j| crossing(path[i], path[i + 1], path[j], path[j + 1]).is_some())
    })
}

/// Where segment `a`–`b` crosses segment `c`–`d`, strictly inside both (touching, sharing an end
/// or running along each other is not crossing).
fn crossing(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let side = |p: Point, q: Point, r: Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        let t = d3 / (d3 - d4);
        Some(Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y)))
    } else {
        None
    }
}
