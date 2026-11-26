//! Planar geometry primitives in a metric coordinate system (meters, y up).

/// Length of the vector (dx, dy).
///
/// Uses `sqrt(dx² + dy²)` instead of `f64::hypot`: in WebAssembly `hypot` is a slow
/// software routine, while `sqrt` is one instruction. `hypot` only matters for values
/// near 1e154, far beyond any map coordinate in meters. (Measured: offsets about 2× faster.)
#[must_use]
#[inline]
pub fn length(dx: f64, dy: f64) -> f64 {
    // Not `mul_add`: WebAssembly has no fused multiply-add, so it would be a slow call too.
    (dx * dx + dy * dy).sqrt()
}

/// Length of a polyline: the sum of its segments' lengths, 0 for fewer than two points.
#[must_use]
pub(crate) fn path_length(path: &[Point]) -> f64 {
    path.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// A point (or vector) in the plane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// East–west coordinate.
    pub x: f64,
    /// North–south coordinate.
    pub y: f64,
}

impl Point {
    /// Creates a point from its coordinates.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to `other`.
    #[must_use]
    pub fn distance(self, other: Self) -> f64 {
        length(other.x - self.x, other.y - self.y)
    }
}

/// A straight segment from `a` to `b`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// Start point.
    pub a: Point,
    /// End point.
    pub b: Point,
}

impl Segment {
    /// Creates a segment from `a` to `b`.
    #[must_use]
    pub const fn new(a: Point, b: Point) -> Self {
        Self { a, b }
    }

    /// Length of the segment.
    #[must_use]
    pub fn length(self) -> f64 {
        self.a.distance(self.b)
    }
}

/// Shifts `seg` sideways by `distance`, keeping its direction and length.
///
/// Positive distances move the segment to the **left** of its direction of travel (a→b),
/// negative distances to the right. This is how each line gets its own slot in a bundle.
///
/// Returns `None` for a degenerate (zero-length or non-finite) segment, which has no direction.
#[must_use]
pub fn offset_segment(seg: Segment, distance: f64) -> Option<Segment> {
    let dx = seg.b.x - seg.a.x;
    let dy = seg.b.y - seg.a.y;
    let len = length(dx, dy);
    if !len.is_finite() || len == 0.0 || !distance.is_finite() {
        return None;
    }
    // Unit left normal of (dx, dy) is (-dy, dx) / len.
    let nx = -dy / len * distance;
    let ny = dx / len * distance;
    Some(Segment::new(
        Point::new(seg.a.x + nx, seg.a.y + ny),
        Point::new(seg.b.x + nx, seg.b.y + ny),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const EPS: f64 = 1e-9;

    fn close(p: Point, q: Point) -> bool {
        p.distance(q) < EPS
    }

    #[test]
    fn eastward_segment_moves_north_for_positive_offset() {
        let seg = Segment::new(Point::new(0.0, 0.0), Point::new(10.0, 0.0));
        let out = offset_segment(seg, 2.0).unwrap();
        assert!(close(out.a, Point::new(0.0, 2.0)));
        assert!(close(out.b, Point::new(10.0, 2.0)));
    }

    #[test]
    fn negative_offset_moves_right() {
        let seg = Segment::new(Point::new(0.0, 0.0), Point::new(0.0, 5.0)); // northward
        let out = offset_segment(seg, -3.0).unwrap();
        assert!(close(out.a, Point::new(3.0, 0.0)));
        assert!(close(out.b, Point::new(3.0, 5.0)));
    }

    #[test]
    fn segment_length_is_euclidean() {
        let seg = Segment::new(Point::new(1.0, 1.0), Point::new(4.0, 5.0)); // 3-4-5 triangle
        assert!((seg.length() - 5.0).abs() < EPS);
        let p = Point::new(7.0, -2.0);
        assert!(Segment::new(p, p).length().abs() < EPS);
        assert!(Segment::new(Point::new(f64::NAN, 0.0), p).length().is_nan());
    }

    #[test]
    fn zero_offset_is_identity() {
        let seg = Segment::new(Point::new(1.0, 2.0), Point::new(4.0, 6.0));
        assert_eq!(offset_segment(seg, 0.0), Some(seg));
    }

    #[test]
    fn degenerate_segments_have_no_offset() {
        let p = Point::new(3.0, 3.0);
        assert_eq!(offset_segment(Segment::new(p, p), 1.0), None);
        let nan = Point::new(f64::NAN, 0.0);
        assert_eq!(offset_segment(Segment::new(nan, p), 1.0), None);
        let seg = Segment::new(Point::new(0.0, 0.0), Point::new(1.0, 0.0));
        assert_eq!(offset_segment(seg, f64::INFINITY), None);
    }

    fn coord() -> impl Strategy<Value = f64> {
        -10_000.0..10_000.0_f64
    }

    proptest! {
        /// For any segment and offset: the result is parallel, equally long, exactly
        /// |distance| away, and on the requested side.
        #[test]
        fn offset_is_parallel_same_length_and_at_distance(
            ax in coord(), ay in coord(), bx in coord(), by in coord(), d in -100.0..100.0_f64,
        ) {
            let seg = Segment::new(Point::new(ax, ay), Point::new(bx, by));
            prop_assume!(seg.length() > 1e-6);
            let out = offset_segment(seg, d).unwrap();
            let tol = 1e-6 * (1.0 + seg.length() + d.abs());

            // Same length.
            prop_assert!((out.length() - seg.length()).abs() < tol);
            // Both endpoints moved by exactly |d|.
            prop_assert!((out.a.distance(seg.a) - d.abs()).abs() < tol);
            prop_assert!((out.b.distance(seg.b) - d.abs()).abs() < tol);
            // Parallel: cross product of the two directions is ~0.
            let (ux, uy) = (seg.b.x - seg.a.x, seg.b.y - seg.a.y);
            let (vx, vy) = (out.b.x - out.a.x, out.b.y - out.a.y);
            prop_assert!((ux * vy - uy * vx).abs() < tol * seg.length());
            // Correct side: cross(direction, shift) has the sign of d.
            let side = ux * (out.a.y - seg.a.y) - uy * (out.a.x - seg.a.x);
            prop_assert!(d.abs() < 1e-9 || side.signum() == d.signum());
        }
    }
}
