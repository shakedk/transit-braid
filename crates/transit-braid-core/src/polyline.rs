//! Offsetting a whole polyline into one continuous parallel line.
//!
//! Each segment is shifted sideways like [`crate::geom::offset_segment`], then neighbors are
//! joined at a **miter**: the point where the two shifted segments' lines cross. Very sharp
//! turns would make that point fly far away (the tip sits 1.4× the offset from the corner at a
//! 90° turn, 11× at 170°, infinitely far at a U-turn), so past `miter_limit` we use a **bevel**
//! instead: end one shifted segment, start the next, and join them with a short cut.
//!
//! Known limit: on the *inside* of a turn, a segment shorter than the offset can fold back on
//! itself. The geometry rounds corners first ([`round_corners`]) and cuts such folds out.

use crate::geom::{Point, length};

/// Same default as SVG's `stroke-miterlimit`: bevel once the miter tip is more than 4× the
/// offset from the corner (turns sharper than about 151°).
pub const DEFAULT_MITER_LIMIT: f64 = 4.0;

/// Shifts the polyline `points` sideways by `distance` (positive = left of travel direction)
/// and returns one continuous line with mitered (or, past `miter_limit`, beveled) corners.
///
/// Repeated points are skipped. Returns `None` if there are fewer than two distinct points,
/// or any value is non-finite.
#[must_use]
pub fn offset_polyline(points: &[Point], distance: f64, miter_limit: f64) -> Option<Vec<Point>> {
    offset_polyline_within(points, distance, miter_limit, f64::INFINITY)
}

/// [`offset_polyline`], also beveling an **outside** corner whose miter tip would reach more than
/// `max_overshoot` (same units as the points) beyond the offset distance. A bevel's points
/// stay exactly `distance` from the line, so the result stays within `|distance| + max_overshoot`
/// of the input at every corner. Inside corners keep their miter (a bevel there would fold the
/// line back on itself).
///
/// Returns `None` like [`offset_polyline`], or if `max_overshoot` is negative or NaN.
#[must_use]
pub fn offset_polyline_within(
    points: &[Point],
    distance: f64,
    miter_limit: f64,
    max_overshoot: f64,
) -> Option<Vec<Point>> {
    // A miter tip is never closer than 1× the offset, so a limit below 1 is meaningless.
    if !distance.is_finite() || !miter_limit.is_finite() || miter_limit < 1.0 {
        return None;
    }
    if max_overshoot.is_nan() || max_overshoot < 0.0 {
        return None;
    }
    let pts = distinct(points)?;
    // Unit left normal of every segment.
    let normals: Vec<(f64, f64)> = pts
        .windows(2)
        .map(|w| {
            let (dx, dy) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let len = length(dx, dy);
            (-dy / len, dx / len)
        })
        .collect();

    // Moves p sideways along normal n.
    let shift = |p: Point, n: (f64, f64)| Point::new(p.x + n.0 * distance, p.y + n.1 * distance);
    let mut out = Vec::with_capacity(pts.len() + 4);
    out.push(shift(pts[0], normals[0]));

    // One join per inner vertex (each corner).
    for (i, &vertex) in pts.iter().enumerate().take(pts.len() - 1).skip(1) {
        let (n1, n2) = (normals[i - 1], normals[i]);
        let cos = n1.0 * n2.0 + n1.1 * n2.1; // cosine of the turn angle
        // Miter length / |distance| = 1 / cos(turn / 2) = sqrt(2 / (1 + cos)).
        let ratio_sq = if cos > -1.0 {
            2.0 / (1.0 + cos)
        } else {
            f64::INFINITY
        };
        // The miter tip sits |distance| × (ratio − 1) beyond the offset; on the outside of a turn
        // (the offset side turning away) that is how far it strays from the line.
        let cross = n1.0 * n2.1 - n1.1 * n2.0;
        let outside = cross * distance < 0.0;
        let overshoot = distance.abs() * (ratio_sq.sqrt() - 1.0);
        if ratio_sq <= miter_limit * miter_limit && !(outside && overshoot > max_overshoot) {
            // Miter point: vertex + distance * (n1 + n2) / (1 + cos).
            let k = distance / (1.0 + cos);
            out.push(Point::new(
                vertex.x + (n1.0 + n2.0) * k,
                vertex.y + (n1.1 + n2.1) * k,
            ));
        } else {
            // Bevel: end the first shifted segment, start the next.
            out.push(shift(vertex, n1));
            out.push(shift(vertex, n2));
        }
    }

    let last = pts.len() - 1;
    out.push(shift(pts[last], normals[last - 1]));
    Some(out)
}

/// Cosine of the largest heading change between two consecutive points of an arc that
/// [`round_corners`] draws: 10°, so a drawn turn never changes heading sharply at one point.
const COS_ARC_STEP: f64 = 0.984_807_753_012_208;

/// Turns whose cosine is above this (under about 1°) are left as they are.
const COS_STRAIGHT: f64 = 0.999_847_695_156_391;

/// Rounds every corner of `points` with a circular arc: an arc tangent to
/// both legs, of radius `radius` where the legs allow it. Two limits make it smaller: an arc may
/// use at most half of each leg (so neighboring corners' arcs never overlap; the first and last
/// legs too), and its middle may stray at most `max_off` from the corner's legs.
/// Arcs are drawn with a point every 10° or less, so the heading changes by at most 10° at
/// every point. U-turns and turns under 1° are left as they are.
///
/// Offsetting the result by `d` gives concentric arcs of radius `r − d` on the inside of a turn:
/// with `radius` at least the bundle's half-width plus a margin, no line folds.
///
/// The arc's points come from halving the angle between unit vectors (square roots and
/// divisions only, no platform trigonometry), so native and WebAssembly agree to the bit.
/// Bad input (a non-finite point, fewer than two distinct points, `radius` or `max_off` not a
/// number above 0) comes back unchanged.
#[must_use]
pub fn round_corners(points: &[Point], radius: f64, max_off: f64) -> Vec<Point> {
    let usable = |v: f64| v.is_finite() && v > 0.0;
    let Some(pts) = distinct(points) else {
        return points.to_vec();
    };
    if !usable(radius) || !usable(max_off) || pts.len() < 3 {
        return points.to_vec();
    }
    let unit = |a: Point, b: Point| {
        let len = a.distance(b);
        (Point::new((b.x - a.x) / len, (b.y - a.y) / len), len)
    };
    let mut out = Vec::with_capacity(pts.len() * 4);
    out.push(pts[0]);
    for i in 1..pts.len() - 1 {
        let (u, len_in) = unit(pts[i - 1], pts[i]);
        let (v, len_out) = unit(pts[i], pts[i + 1]);
        let cos = u.x * v.x + u.y * v.y;
        let cross = u.x * v.y - u.y * v.x;
        if cos > COS_STRAIGHT || 1.0 + cos < 1e-9 {
            out.push(pts[i]);
            continue;
        }
        let tan_half = cross.abs() / (1.0 + cos);
        let cos_half = ((1.0 + cos) / 2.0).sqrt();
        // The arc's middle strays r · (1 − cos(θ/2)) from the legs.
        let r = radius.min(max_off / (1.0 - cos_half));
        let t = (r * tan_half).min(len_in / 2.0).min(len_out / 2.0);
        let r = t / tan_half;
        let a = Point::new(pts[i].x - u.x * t, pts[i].y - u.y * t);
        let b = Point::new(pts[i].x + v.x * t, pts[i].y + v.y * t);
        let side = if cross > 0.0 { 1.0 } else { -1.0 };
        let center = Point::new(a.x - u.y * r * side, a.y + u.x * r * side);
        let from = Point::new((a.x - center.x) / r, (a.y - center.y) / r);
        let to = Point::new((b.x - center.x) / r, (b.y - center.y) / r);
        out.push(a);
        let mut arc = Vec::new();
        bisect_arc(from, to, &mut arc, 0);
        out.extend(
            arc.into_iter()
                .map(|d| Point::new(center.x + d.x * r, center.y + d.y * r)),
        );
        out.push(b);
    }
    out.push(pts[pts.len() - 1]);
    // Two arcs meeting halfway along a short leg end and start at the same point, up to rounding.
    out.dedup_by(|b, a| a.distance(*b) < 1e-9);
    out
}

/// A **biarc** from `p0`, leaving along unit tangent `t0`, to `p1`, arriving along unit tangent
/// `t1`: two circular arcs that meet with the same tangent, so the line's heading never jumps.
/// Equal-distance construction: control points `p0 + α·t0` and
/// `p1 − α·t1`, joined halfway between them. Points after `p0` up to `p1`, at most 10° apart
/// along each arc (trig-free, like [`round_corners`]). `None` for a shape a biarc draws badly or
/// not at all: a non-finite or non-unit input, `p0` and `p1` together, an arc over 180°, or a
/// bulge (α) more than twice the gap; the caller then draws something else.
#[must_use]
pub fn biarc(p0: Point, t0: Point, p1: Point, t1: Point) -> Option<Vec<Point>> {
    let finite = [p0.x, p0.y, t0.x, t0.y, p1.x, p1.y, t1.x, t1.y]
        .iter()
        .all(|v| v.is_finite());
    let unit = |t: Point| (t.x * t.x + t.y * t.y - 1.0).abs() < 1e-9;
    if !finite || !unit(t0) || !unit(t1) {
        return None;
    }
    let v = Point::new(p1.x - p0.x, p1.y - p0.y);
    let vv = v.x * v.x + v.y * v.y;
    if vv < 1e-18 {
        return None;
    }
    let t = Point::new(t0.x + t1.x, t0.y + t1.y);
    let vt = v.x * t.x + v.y * t.y;
    let denom = 2.0 * (1.0 - (t0.x * t1.x + t0.y * t1.y));
    let alpha = if denom.abs() < 1e-12 {
        // Parallel tangents: α = |v|² / (4 v·t1).
        let vt1 = v.x * t1.x + v.y * t1.y;
        if vt1.abs() < 1e-12 {
            return None;
        }
        vv / (4.0 * vt1)
    } else {
        (-vt + (vt * vt + denom * vv).sqrt()) / denom
    };
    // NaN fails both tests, so it is refused too.
    if alpha.is_nan() || alpha <= 0.0 || alpha > 2.0 * vv.sqrt() {
        return None;
    }
    let q0 = Point::new(p0.x + t0.x * alpha, p0.y + t0.y * alpha);
    let q1 = Point::new(p1.x - t1.x * alpha, p1.y - t1.y * alpha);
    let joint = Point::new((q0.x + q1.x) / 2.0, (q0.y + q1.y) / 2.0);
    let (dx, dy) = (q1.x - q0.x, q1.y - q0.y);
    let len = length(dx, dy);
    if len < 1e-12 {
        return None;
    }
    let tm = Point::new(dx / len, dy / len);
    let mut out = arc_to(p0, t0, joint)?;
    out.extend(arc_to(joint, tm, p1)?);
    out.dedup_by(|b, a| a.distance(*b) < 1e-9);
    Some(out)
}

/// The circular arc from `a`, leaving along unit tangent `ta`, to `b`: its points after `a` up to
/// `b`, at most 10° apart; just `b` when `b` lies straight ahead. `None` for an arc over 180°.
fn arc_to(a: Point, ta: Point, b: Point) -> Option<Vec<Point>> {
    let chord = Point::new(b.x - a.x, b.y - a.y);
    let cc = chord.x * chord.x + chord.y * chord.y;
    if cc < 1e-18 {
        return Some(Vec::new());
    }
    // An arc sweeps twice the angle between its start tangent and its chord.
    let ahead = ta.x * chord.x + ta.y * chord.y;
    if ahead <= 0.0 {
        return None;
    }
    let n = Point::new(-ta.y, ta.x);
    let side = n.x * chord.x + n.y * chord.y;
    if side.abs() < 1e-9 * cc.sqrt() {
        return Some(vec![b]);
    }
    let r = cc / (2.0 * side); // signed: positive turns left
    let center = Point::new(a.x + n.x * r, a.y + n.y * r);
    let radius = r.abs();
    let from = Point::new((a.x - center.x) / radius, (a.y - center.y) / radius);
    let to = Point::new((b.x - center.x) / radius, (b.y - center.y) / radius);
    let mut dirs = Vec::new();
    bisect_arc(from, to, &mut dirs, 0);
    let mut out: Vec<Point> = dirs
        .into_iter()
        .map(|d| Point::new(center.x + d.x * radius, center.y + d.y * radius))
        .collect();
    out.push(b);
    Some(out)
}

/// The unit directions strictly between unit vectors `a` and `b` (less than 180° apart), in
/// order, so that consecutive ones are at most 10° apart: halves the angle until it is small.
fn bisect_arc(a: Point, b: Point, out: &mut Vec<Point>, depth: u32) {
    if a.x * b.x + a.y * b.y >= COS_ARC_STEP || depth > 8 {
        return;
    }
    let (sx, sy) = (a.x + b.x, a.y + b.y);
    let len = length(sx, sy);
    if len == 0.0 {
        return;
    }
    let m = Point::new(sx / len, sy / len);
    bisect_arc(a, m, out, depth + 1);
    out.push(m);
    bisect_arc(m, b, out, depth + 1);
}

/// Drops consecutive duplicates. `None` if a value is non-finite or < 2 points remain.
fn distinct(points: &[Point]) -> Option<Vec<Point>> {
    let mut pts: Vec<Point> = Vec::with_capacity(points.len());
    for &p in points {
        if !p.x.is_finite() || !p.y.is_finite() {
            return None;
        }
        if pts.last().is_none_or(|q| q.distance(p) > 0.0) {
            pts.push(p);
        }
    }
    (pts.len() >= 2).then_some(pts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const EPS: f64 = 1e-9;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    fn close(a: Point, b: Point) -> bool {
        a.distance(b) < EPS
    }

    /// Signed distance from `q` to the infinite line through a→b (positive = left side).
    fn signed_distance(a: Point, b: Point, q: Point) -> f64 {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        (dx * (q.y - a.y) - dy * (q.x - a.x)) / length(dx, dy)
    }

    #[test]
    fn straight_line_is_shifted_with_no_extra_points() {
        let out = offset_polyline(&[p(0.0, 0.0), p(5.0, 0.0), p(10.0, 0.0)], 2.0, 4.0).unwrap();
        assert_eq!(out.len(), 3);
        assert!(close(out[0], p(0.0, 2.0)));
        assert!(close(out[1], p(5.0, 2.0)));
        assert!(close(out[2], p(10.0, 2.0)));
    }

    #[test]
    fn right_angle_turn_gets_a_square_corner() {
        // East then north: the left side is the inside of the turn.
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)];
        let inside = offset_polyline(&pts, 2.0, 4.0).unwrap();
        assert!(close(inside[1], p(8.0, 2.0)));
        let outside = offset_polyline(&pts, -2.0, 4.0).unwrap();
        assert!(close(outside[1], p(12.0, -2.0)));
    }

    #[test]
    fn sharp_turn_past_the_limit_is_beveled() {
        // Almost a U-turn: the miter would be very long, so the corner is cut (2 points).
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(0.0, 1.0)];
        let out = offset_polyline(&pts, -2.0, 4.0).unwrap();
        assert_eq!(out.len(), 4);
        assert!(close(out[1], p(10.0, -2.0)));
    }

    #[test]
    fn repeated_points_are_skipped_and_degenerate_input_rejected() {
        let out = offset_polyline(&[p(0.0, 0.0), p(0.0, 0.0), p(4.0, 0.0)], 1.0, 4.0).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(offset_polyline(&[p(1.0, 1.0), p(1.0, 1.0)], 1.0, 4.0), None);
        assert_eq!(
            offset_polyline(&[p(0.0, 0.0), p(f64::NAN, 1.0)], 1.0, 4.0),
            None
        );
        assert_eq!(offset_polyline(&[p(0.0, 0.0), p(1.0, 0.0)], 1.0, 0.5), None);
    }

    #[test]
    fn empty_and_single_point_input_is_rejected() {
        assert_eq!(offset_polyline(&[], 1.0, 4.0), None);
        assert_eq!(offset_polyline(&[p(3.0, 4.0)], 1.0, 4.0), None);
    }

    #[test]
    fn any_non_finite_coordinate_or_option_is_rejected() {
        let line = [p(0.0, 0.0), p(1.0, 0.0)];
        // Each coordinate on its own: x bad with y fine, and y bad with x fine.
        assert_eq!(
            offset_polyline(&[p(0.0, 0.0), p(f64::NAN, 0.0)], 1.0, 4.0),
            None
        );
        assert_eq!(
            offset_polyline(&[p(0.0, 0.0), p(1.0, f64::NAN)], 1.0, 4.0),
            None
        );
        assert_eq!(
            offset_polyline(&[p(f64::INFINITY, 0.0), p(1.0, 0.0)], 1.0, 4.0),
            None
        );
        // Each option on its own, with the other one valid.
        assert_eq!(offset_polyline(&line, f64::NAN, 4.0), None);
        assert_eq!(offset_polyline(&line, f64::INFINITY, 4.0), None);
        assert_eq!(offset_polyline(&line, 1.0, f64::NAN), None);
        assert_eq!(offset_polyline(&line, 1.0, f64::INFINITY), None);
    }

    #[test]
    fn miter_limit_of_exactly_one_is_allowed() {
        let out = offset_polyline(&[p(0.0, 0.0), p(1.0, 0.0)], 1.0, 1.0).unwrap();
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn miter_limit_compares_lengths_not_squared_vs_doubled() {
        // A turn whose miter is sqrt(10) ≈ 3.16 times the offset (cos of the turn = -0.8).
        let pts = [p(0.0, 0.0), p(10.0, 0.0), p(2.0, 6.0)];
        // Limit 4 > 3.16: one miter point at the corner.
        assert_eq!(offset_polyline(&pts, 1.0, 4.0).unwrap().len(), 3);
        // Limit 3 < 3.16: beveled, two points at the corner.
        assert_eq!(offset_polyline(&pts, 1.0, 3.0).unwrap().len(), 4);
    }

    /// Random polylines: 2–7 points, coordinates in ±1000.
    fn polyline() -> impl Strategy<Value = Vec<Point>> {
        prop::collection::vec((-1000.0..1000.0_f64, -1000.0..1000.0_f64), 2..8)
            .prop_map(|v| v.into_iter().map(|(x, y)| p(x, y)).collect())
    }

    // Property tests: each rule below runs on 256 random polylines; a failure is shrunk to the
    // smallest polyline that still breaks it.
    /// Heading change in degrees at every inner point of `pts` (via the cosine; tests only).
    fn turns_deg(pts: &[Point]) -> Vec<f64> {
        pts.windows(3)
            .map(|w| {
                let (ux, uy) = (w[1].x - w[0].x, w[1].y - w[0].y);
                let (vx, vy) = (w[2].x - w[1].x, w[2].y - w[1].y);
                let c = (ux * vx + uy * vy) / (length(ux, uy) * length(vx, vy));
                c.clamp(-1.0, 1.0).acos().to_degrees()
            })
            .collect()
    }

    /// Distance from `q` to the polyline `pts`.
    fn off_polyline(q: Point, pts: &[Point]) -> f64 {
        pts.windows(2)
            .map(|w| crate::topology::distance_to_segment(q, w[0], w[1]))
            .fold(f64::INFINITY, f64::min)
    }

    /// Radius of the circle through three points (infinite when they are in line).
    fn circumradius(a: Point, b: Point, c: Point) -> f64 {
        let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        if cross.abs() < 1e-12 {
            return f64::INFINITY;
        }
        a.distance(b) * b.distance(c) * c.distance(a) / (2.0 * cross.abs())
    }

    /// Angle in degrees between direction `d` and the step a→b.
    fn off_deg(d: Point, a: Point, b: Point) -> f64 {
        let (sx, sy) = (b.x - a.x, b.y - a.y);
        ((d.x * sx + d.y * sy) / (length(d.x, d.y) * length(sx, sy)))
            .clamp(-1.0, 1.0)
            .acos()
            .to_degrees()
    }

    #[test]
    fn a_biarc_of_an_s_bend_meets_itself_with_one_tangent() {
        // East to east, 20 m on and 10 m across: two mirrored arcs.
        let (p0, p1) = (p(0.0, 0.0), p(20.0, 10.0));
        let out = biarc(p0, p(1.0, 0.0), p1, p(1.0, 0.0)).unwrap();
        assert_eq!(*out.last().unwrap(), p1);
        let mut all = vec![p0];
        all.extend(&out);
        assert!(
            turns_deg(&all).iter().all(|&t| t <= 10.0 + 1e-9),
            "{:?}",
            turns_deg(&all)
        );
        assert!(off_deg(p(1.0, 0.0), all[0], all[1]) <= 5.0 + 1e-9);
        assert!(off_deg(p(1.0, 0.0), all[all.len() - 2], p1) <= 5.0 + 1e-9);
        // Straight ahead: nothing to bend.
        let straight = biarc(p0, p(1.0, 0.0), p(10.0, 0.0), p(1.0, 0.0)).unwrap();
        assert!(
            straight.iter().all(|q| q.y.abs() < 1e-12) && straight.last() == Some(&p(10.0, 0.0))
        );
    }

    #[test]
    fn a_biarc_refuses_what_it_cannot_draw() {
        let (o, e, n) = (p(0.0, 0.0), p(1.0, 0.0), p(0.0, 1.0));
        for (p0, t0, p1, t1) in [
            (o, e, o, e),                               // no gap
            (o, p(2.0, 0.0), p(5.0, 5.0), n),           // not a unit tangent
            (p(f64::NAN, 0.0), e, p(5.0, 5.0), n),      // not a number
            (o, e, p(5.0, 5.0), p(f64::INFINITY, 0.0)), // not a number
            (o, n, p(300.0, 0.0), n),                   // parallel, square to the gap
            (o, e, p(-10.0, 0.0), e),                   // the target behind: a loop
        ] {
            assert_eq!(biarc(p0, t0, p1, t1), None, "{p0:?} {t0:?} {p1:?} {t1:?}");
        }
    }

    #[test]
    fn a_right_angle_becomes_a_quarter_circle_of_the_radius() {
        let out = round_corners(&[p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], 20.0, 1e9);
        assert_eq!(
            (out[0], *out.last().unwrap()),
            (p(0.0, 0.0), p(100.0, 100.0))
        );
        // Tangent points 20 m before and after the corner; every arc point 20 m from (80, 20).
        assert!(out.iter().any(|&q| close(q, p(80.0, 0.0))));
        assert!(out.iter().any(|&q| close(q, p(100.0, 20.0))));
        let arc: Vec<Point> = out[1..out.len() - 1].to_vec();
        assert!(
            arc.iter()
                .all(|q| (q.distance(p(80.0, 20.0)) - 20.0).abs() < 1e-9)
        );
        assert!(
            turns_deg(&out).iter().all(|&t| t <= 10.0 + 1e-9),
            "{:?}",
            turns_deg(&out)
        );
        // The room: with 3 m, the radius shrinks so the arc's middle stays 3 m off.
        let tight = round_corners(&[p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)], 20.0, 3.0);
        let worst = tight
            .iter()
            .map(|&q| off_polyline(q, &[p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)]))
            .fold(0.0, f64::max);
        assert!((worst - 3.0).abs() < 1e-9, "{worst}");
    }

    #[test]
    fn short_legs_shrink_the_arc_and_bad_input_comes_back_unchanged() {
        // Legs of 10 m: an arc may use 5 m of each, whatever the radius asked for.
        let out = round_corners(&[p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)], 50.0, 1e9);
        assert!(
            out.iter().any(|&q| close(q, p(5.0, 0.0)))
                && out.iter().any(|&q| close(q, p(10.0, 5.0)))
        );
        let line = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0)];
        for (pts, r, m) in [
            (&line[..], 0.0, 5.0),
            (&line[..], -1.0, 5.0),
            (&line[..], f64::NAN, 5.0),
            (&line[..], 10.0, 0.0),
            (&line[..], 10.0, f64::INFINITY),
            (&line[..2], 10.0, 5.0),
            (&[p(f64::NAN, 0.0), p(1.0, 0.0), p(1.0, 1.0)][..], 10.0, 5.0),
            (&[p(1.0, 1.0), p(1.0, 1.0)][..], 10.0, 5.0),
            (&[][..], 10.0, 5.0),
        ] {
            // Compared as text: NaN is never equal to itself.
            assert_eq!(
                format!("{:?}", round_corners(pts, r, m)),
                format!("{pts:?}"),
                "{r} {m}"
            );
        }
        // A U-turn is left as it is.
        let u = [p(0.0, 0.0), p(10.0, 0.0), p(0.0, 0.0)];
        assert_eq!(round_corners(&u, 5.0, 5.0), u.to_vec());
    }

    proptest! {
        /// A biarc is tangent-continuous at every join. It leaves `p0` within 5°
        /// of `t0` (half a 10° step), arrives at `p1` within 5° of `t1`, ends exactly at `p1`, and
        /// its heading never changes by more than 10° at a point, also where its two arcs meet.
        #[test]
        fn biarcs_are_tangent_continuous(
            to in (5.0f64..200.0, -80.0f64..80.0),
            start in -60.0f64..60.0,
            end in -60.0f64..60.0,
        ) {
            let (dist, bearing) = to;
            let dir = |deg: f64| p(deg.to_radians().cos(), deg.to_radians().sin());
            let (p0, p1) = (p(0.0, 0.0), p(dist * bearing.to_radians().cos(), dist * bearing.to_radians().sin()));
            let (t0, t1) = (dir(bearing + start), dir(bearing + end));
            if let Some(out) = biarc(p0, t0, p1, t1) {
                prop_assert_eq!(out[out.len() - 1], p1);
                let mut all = vec![p0];
                all.extend(&out);
                prop_assert!(off_deg(t0, all[0], all[1]) <= 5.0 + 1e-6);
                prop_assert!(off_deg(t1, all[all.len() - 2], p1) <= 5.0 + 1e-6);
                for t in turns_deg(&all) {
                    prop_assert!(t <= 10.0 + 1e-6, "turn of {} degrees", t);
                }
            }
        }

        /// After rounding, the heading changes by at
        /// most 10° at every point (no U-turns in the input), the line stays within `max_off`
        /// of the input, and its ends stay where they were.
        #[test]
        fn rounding_turns_at_most_ten_degrees_per_point_and_stays_close(
            legs in prop::collection::vec((5.0f64..300.0, -170.0f64..170.0), 2..8),
            radius in 0.5f64..80.0,
            max_off in 0.5f64..10.0,
        ) {
            let mut pts = vec![p(0.0, 0.0)];
            let mut heading = 0.0_f64;
            for (len, turn) in &legs {
                heading += turn.to_radians();
                let last = pts[pts.len() - 1];
                pts.push(p(last.x + len * heading.cos(), last.y + len * heading.sin()));
            }
            let out = round_corners(&pts, radius, max_off);
            prop_assert_eq!(out[0], pts[0]);
            prop_assert_eq!(out[out.len() - 1], pts[pts.len() - 1]);
            for t in turns_deg(&out) {
                prop_assert!(t <= 10.0 + 1e-6, "turn of {} degrees", t);
            }
            for q in &out {
                prop_assert!(off_polyline(*q, &pts) <= max_off + 1e-6);
            }
            prop_assert_eq!(&out, &round_corners(&pts, radius, max_off));
        }

        /// With legs long enough for the full radius, offsetting the
        /// rounded line by up to `radius − margin` either way never bends tighter than `margin`
        /// (less the chords' 0.4%): every line of a bundle as wide as that keeps a radius of at
        /// least W/2 + margin.
        #[test]
        fn offset_arcs_keep_at_least_the_margin(
            turns in prop::collection::vec(-160.0f64..160.0, 1..5),
            radius in 5.0f64..60.0,
            margin in 1.0f64..4.0,
            side in prop_oneof![Just(1.0f64), Just(-1.0)],
        ) {
            prop_assume!(margin < radius);
            let mut pts = vec![p(0.0, 0.0)];
            let mut heading = 0.0_f64;
            for turn in std::iter::once(&0.0).chain(&turns) {
                heading += turn.to_radians();
                let last = pts[pts.len() - 1];
                // Each leg 12 × radius: an arc uses at most radius × tan(80°) < 6 × radius of it.
                pts.push(p(last.x + 12.0 * radius * heading.cos(), last.y + 12.0 * radius * heading.sin()));
            }
            let rounded = round_corners(&pts, radius, 1e9);
            let d = radius - margin;
            let inner = offset_polyline(&rounded, side * d, DEFAULT_MITER_LIMIT).unwrap();
            // Arcs are drawn as chords at most 10° apart, which pulls an inner offset in by
            // d × (1 / cos 5° − 1): under 0.4% of the offset.
            let bound = margin - d * (1.0 / 5.0_f64.to_radians().cos() - 1.0);
            for w in inner.windows(3) {
                prop_assert!(circumradius(w[0], w[1], w[2]) >= bound - 1e-6);
            }
        }

        /// With a huge miter limit (miters everywhere, unless the line doubles back exactly),
        /// every output point lies exactly `distance` to the left of the segments next to it.
        #[test]
        fn miter_points_sit_at_the_offset_from_both_neighbors(
            pts in polyline(), d in -50.0..50.0_f64,
        ) {
            let pts = distinct(&pts).unwrap_or_default();
            prop_assume!(pts.len() >= 2);
            prop_assume!(pts.windows(2).all(|w| w[0].distance(w[1]) > 1e-3));
            let out = offset_polyline(&pts, d, 1e9).unwrap();
            prop_assume!(out.len() == pts.len()); // skip exact U-turns (beveled)
            for (i, q) in out.iter().enumerate() {
                // Rounding error grows with how far the miter point sits from its vertex.
                let tol = 1e-7 * (1.0 + q.distance(pts[i]));
                if i > 0 {
                    let s = signed_distance(pts[i - 1], pts[i], *q);
                    prop_assert!((s - d).abs() < tol, "segment before {i}: {s} vs {d}");
                }
                if i + 1 < pts.len() {
                    let s = signed_distance(pts[i], pts[i + 1], *q);
                    prop_assert!((s - d).abs() < tol, "segment after {i}: {s} vs {d}");
                }
            }
        }

        /// Ends are shifted straight sideways, like offset_segment does.
        #[test]
        fn ends_match_offset_segment(pts in polyline(), d in -50.0..50.0_f64) {
            let pts = distinct(&pts).unwrap_or_default();
            prop_assume!(pts.len() >= 2);
            let out = offset_polyline(&pts, d, DEFAULT_MITER_LIMIT).unwrap();
            let first = crate::geom::offset_segment(crate::geom::Segment::new(pts[0], pts[1]), d).unwrap();
            let n = pts.len();
            let last = crate::geom::offset_segment(crate::geom::Segment::new(pts[n - 2], pts[n - 1]), d).unwrap();
            prop_assert!(out[0].distance(first.a) < 1e-6);
            prop_assert!(out[out.len() - 1].distance(last.b) < 1e-6);
        }
    }

    #[test]
    fn an_outside_corner_past_the_overshoot_is_beveled_and_an_inside_one_is_not() {
        // East then north, offset 10 m to the right: the right side is the outside of this left
        // turn. Its miter tip would be 10 × (√2 − 1) ≈ 4.14 m beyond the offset.
        let pts = [p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0)];
        let mitered = offset_polyline_within(&pts, -10.0, 4.0, 5.0).unwrap();
        assert_eq!(mitered.len(), 3, "4.14 m ≤ 5 m: a miter");
        let beveled = offset_polyline_within(&pts, -10.0, 4.0, 4.0).unwrap();
        assert_eq!(beveled.len(), 4, "4.14 m > 4 m: a bevel");
        assert!(close(beveled[1], p(100.0, -10.0)) && close(beveled[2], p(110.0, 0.0)));
        for q in &beveled {
            let d = [(pts[0], pts[1]), (pts[1], pts[2])]
                .iter()
                .map(|&(a, b)| crate::topology::distance_to_segment(*q, a, b))
                .fold(f64::INFINITY, f64::min);
            assert!(
                d <= 10.0 + EPS,
                "a bevel point stays at the offset: {q:?} is {d}"
            );
        }
        // The same turn offset to the left (the inside): still a miter, whatever the limit.
        assert_eq!(
            offset_polyline_within(&pts, 10.0, 4.0, 0.0).unwrap().len(),
            3
        );
        // No limit: exactly offset_polyline.
        assert_eq!(
            offset_polyline_within(&pts, -10.0, 4.0, f64::INFINITY),
            offset_polyline(&pts, -10.0, 4.0)
        );
    }

    #[test]
    fn a_bad_overshoot_is_refused() {
        let pts = [p(0.0, 0.0), p(1.0, 0.0)];
        assert!(offset_polyline_within(&pts, 1.0, 4.0, -1.0).is_none());
        assert!(offset_polyline_within(&pts, 1.0, 4.0, f64::NAN).is_none());
        assert!(offset_polyline_within(&pts, 1.0, 4.0, 0.0).is_some());
    }
}
