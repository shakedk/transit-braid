//! Spike removal and flicker smoothing.

use proptest::prelude::*;

use super::keys::{Key, despike, smooth_islands};
use super::test_support::routes;
use crate::budget::{Counter, Exceeded, Work};
use crate::geom::Point;
use crate::ids::LineId;

#[test]
fn despike_removes_only_short_out_and_backs() {
    let p = |x, y| Point::new(x, y);
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
    let p = |x, y| Point::new(x, y);
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
    for bad in [f64::NAN, f64::INFINITY] {
        let mut line = spike;
        line[2] = p(50.0, bad);
        // Compared bit for bit, since NaN never equals itself.
        let bits = |l: &[Point]| -> Vec<(u64, u64)> {
            l.iter().map(|q| (q.x.to_bits(), q.y.to_bits())).collect()
        };
        assert_eq!(bits(&despike(&line, 5.0)), bits(&line), "a point at {bad}");
    }
}

#[test]
fn short_islands_take_their_neighbors_key() {
    let k = |lines: &[usize]| -> Key { (lines.iter().map(|&l| LineId::new(l)).collect(), false) };
    let mut unbounded = Counter::new(Work::GraphSteps, u64::MAX);
    let mut smooth = |keys: &mut [Key]| {
        smooth_islands(keys, 2.0, 20.0, &mut unbounded).unwrap();
    };
    // 2 m samples: a 3-sample (6 m) island of {0, 1} inside {0} is flicker.
    let mut keys = vec![
        k(&[0]),
        k(&[0]),
        k(&[0, 1]),
        k(&[0, 1]),
        k(&[0, 1]),
        k(&[0]),
        k(&[0]),
    ];
    smooth(&mut keys);
    assert!(keys.iter().all(|x| *x == k(&[0])), "{keys:?}");
    // A long island (12 samples = 24 m) stays; so do runs at the ends and real steps.
    let mut long: Vec<Key> = [vec![k(&[0]); 2], vec![k(&[0, 1]); 12], vec![k(&[0]); 2]].concat();
    let before = long.clone();
    smooth(&mut long);
    assert_eq!(long, before);
    let mut steps = vec![k(&[0, 1, 2]), k(&[0, 1]), k(&[0])];
    let before = steps.clone();
    smooth(&mut steps);
    assert_eq!(steps, before);
    let mut empty: Vec<Key> = vec![];
    smooth(&mut empty);
    // Nested flicker is removed in rounds: {0,1,2} inside {0,1} inside {0}.
    let mut nested = vec![k(&[0]), k(&[0, 1]), k(&[0, 1, 2]), k(&[0, 1]), k(&[0])];
    smooth(&mut nested);
    assert!(nested.iter().all(|x| *x == k(&[0])), "{nested:?}");
    // Each pass reads every key, and counts it; past the bound it stops with an error.
    let mut nested = vec![k(&[0]), k(&[0, 1]), k(&[0, 1, 2]), k(&[0, 1]), k(&[0])];
    let mut steps = Counter::new(Work::GraphSteps, u64::MAX);
    smooth_islands(&mut nested, 2.0, 20.0, &mut steps).unwrap();
    assert_eq!(
        steps.used(),
        3 * 5,
        "two passes that fill an island, one that finds none"
    );
    let mut nested = vec![k(&[0]), k(&[0, 1]), k(&[0, 1, 2]), k(&[0, 1]), k(&[0])];
    let mut tight = Counter::new(Work::GraphSteps, 14);
    assert_eq!(
        smooth_islands(&mut nested, 2.0, 20.0, &mut tight),
        Err(Exceeded {
            work: Work::GraphSteps,
            max: 14
        })
    );
}

proptest! {
    /// `despike` only drops points: what is left is the input in order, with both ends.
    #[test]
    fn despike_keeps_ends_and_order(input in routes(), merge_m in 0.1f64..50.0) {
        for line in &input {
            let out = despike(line, merge_m);
            prop_assert!(out.len() <= line.len());
            prop_assert_eq!(out.first(), line.first());
            prop_assert_eq!(out.last(), line.last());
            let mut rest = line.iter();
            prop_assert!(out.iter().all(|q| rest.any(|p| p == q)), "a subsequence");
        }
    }
}
