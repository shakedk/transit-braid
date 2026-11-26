//! A copy of the core topology's spike removal ([`despike`]): the core's is internal, and
//! the criteria ([`crate::criteria`]) remove the same spikes before measuring. Keep it the same
//! as the core's.

use transit_braid_core::geom::Point;

/// A spike's legs may be at most this many merge distances long (the core's `SPIKE_LEGS`).
const SPIKE_LEGS: f64 = 4.0;

/// Removes spikes, as the core's topology does before it builds: a point whose neighbors are
/// within `merge_m` of each other and at most 4 × `merge_m` away from it (the route goes out a
/// little and comes straight back). Repeats until none is left; keeps both ends. A route that
/// would lose all its length is returned unchanged, and so is bad input (`merge_m` not a finite
/// number above 0, or any point not finite).
#[must_use]
pub fn despike(line: &[Point], merge_m: f64) -> Vec<Point> {
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

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use transit_braid_core::topology::{Params, build};

    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    #[test]
    fn despike_removes_only_short_out_and_backs() {
        // Out 10 m and back: removed.
        let line = [
            p(0.0, 0.0),
            p(50.0, 0.0),
            p(50.0, 10.0),
            p(50.0, 0.0),
            p(90.0, 0.0),
        ];
        assert_eq!(
            despike(&line, 5.0),
            vec![p(0.0, 0.0), p(50.0, 0.0), p(50.0, 0.0), p(90.0, 0.0)]
        );
        // Out 30 m and back (a real dead end): kept.
        let long = [p(0.0, 0.0), p(50.0, 0.0), p(50.0, 30.0), p(50.0, 0.0)];
        assert_eq!(despike(&long, 5.0), long.to_vec());
        // Ends are kept; a line that would lose all its length is unchanged.
        let tiny = [p(0.0, 0.0), p(3.0, 0.0), p(0.0, 0.0)];
        assert_eq!(despike(&tiny, 5.0), tiny.to_vec());
        assert_eq!(despike(&[], 5.0), Vec::<Point>::new());
    }

    #[test]
    fn despike_returns_bad_input_unchanged() {
        let spike = [
            p(0.0, 0.0),
            p(50.0, 0.0),
            p(50.0, 10.0),
            p(50.0, 0.0),
            p(90.0, 0.0),
        ];
        for merge_m in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -5.0] {
            assert_eq!(
                despike(&spike, merge_m),
                spike.to_vec(),
                "merge_m {merge_m}"
            );
        }
        // Compared bit for bit, since NaN never equals itself.
        let bits = |l: &[Point]| -> Vec<(u64, u64)> {
            l.iter().map(|q| (q.x.to_bits(), q.y.to_bits())).collect()
        };
        for bad in [f64::NAN, f64::INFINITY] {
            let mut line = spike;
            line[2] = p(50.0, bad);
            assert_eq!(bits(&despike(&line, 5.0)), bits(&line), "a point at {bad}");
        }
    }

    /// The copy removes what the core's topology removes: a spike in a shared stretch leaves it
    /// one edge, despiked here or not (the core despikes internally before it builds).
    #[test]
    fn the_core_builds_the_same_graph_from_despiked_input() {
        let spiky = vec![
            vec![
                p(0.0, 0.0),
                p(50.0, 0.0),
                p(50.0, 10.0),
                p(50.0, 0.0),
                p(100.0, 0.0),
            ],
            vec![p(0.0, 2.0), p(100.0, 2.0)],
        ];
        let clean: Vec<Vec<Point>> = spiky.iter().map(|l| despike(l, 5.0)).collect();
        let params = Params::default();
        assert_eq!(
            build(&spiky, &params).unwrap(),
            build(&clean, &params).unwrap()
        );
    }

    proptest! {
        /// Despiking only drops points: what is left is the input in order, with both ends.
        #[test]
        fn despike_keeps_ends_and_order(
            pts in prop::collection::vec((-100.0..100.0_f64, -100.0..100.0_f64), 0..12),
            merge_m in 0.1f64..50.0,
        ) {
            let line: Vec<Point> = pts.iter().map(|&(x, y)| p(x, y)).collect();
            let out = despike(&line, merge_m);
            prop_assert!(out.len() <= line.len());
            prop_assert_eq!(out.first(), line.first());
            prop_assert_eq!(out.last(), line.last());
            let mut rest = line.iter();
            for q in &out {
                prop_assert!(rest.any(|r| r == q), "out of order or not from the input");
            }
        }
    }
}
