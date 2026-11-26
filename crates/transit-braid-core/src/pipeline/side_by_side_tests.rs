//! Drawing: lines in their slots at a zoom, the projection, station bars.

use std::f64::consts::PI;

use proptest::prelude::*;

use super::projection::{EARTH_RADIUS_M, TILE_PX};
use super::test_support::*;
use super::*;
use crate::ids::LineId;
use crate::topology::{self, Params};

/// Meters north of `(lng, lat)` at each point of `line`.
fn north_m(line: &[Point], lng: f64, lat: f64) -> Vec<f64> {
    let proj = Projection::at(Point::new(lng, lat));
    line.iter().map(|&p| proj.forward(p).y).collect()
}

#[test]
fn no_routes_give_no_routes() {
    assert_eq!(
        side_by_side(&[], &ONE_PX_PER_M).unwrap(),
        Vec::<Vec<Point>>::new()
    );
}

#[test]
fn a_single_line_stays_where_it_is() {
    let line = east(34.78, 32.07, 400.0);
    let out = side_by_side(std::slice::from_ref(&line), &ONE_PX_PER_M).unwrap();
    assert_eq!(out.len(), 1);
    // On its own street (no slot to move to), from its start to its end.
    for y in north_m(&out[0], 34.78, 32.07) {
        assert_near(y, 0.0, 1e-6);
    }
    let (first, last) = (out[0][0], out[0][out[0].len() - 1]);
    assert_near(first.x, line[0].x, 1e-9);
    assert_near(last.x, line[4].x, 1e-9);
}

#[test]
fn two_lines_on_one_street_sit_one_pitch_apart() {
    // Same way: one slot each side of the street, 2.5 m out (pitch 4 + 1 px at 1 m/px).
    let a = east(10.013, 40.69, 400.0);
    let out = side_by_side(&[a.clone(), a.clone()], &ONE_PX_PER_M).unwrap();
    let (y0, y1) = (
        north_m(&out[0], 10.013, 40.69),
        north_m(&out[1], 10.013, 40.69),
    );
    for (p, q) in y0.iter().zip(&y1) {
        assert_near(p.abs(), 2.5, 0.01);
        assert_near(p + q, 0.0, 0.01);
    }
    // Opposite ways: still two separate slots, 5 m apart.
    let mut b = a.clone();
    b.reverse();
    let out = side_by_side(&[a, b], &ONE_PX_PER_M).unwrap();
    let (ya, yb) = (
        north_m(&out[0], 10.013, 40.69),
        north_m(&out[1], 10.013, 40.69),
    );
    assert_near((ya[0] - yb[0]).abs(), 5.0, 0.01);
}

#[test]
fn zoom_sets_the_pitch() {
    // Zoom 16 at the equator: 2π · 6378137 / (512 · 2^16) ≈ 1.194 m per pixel; pitch 5 px.
    let a = east(0.0, 0.0, 400.0);
    let settings = Settings {
        scale: Scale::Zoom(16.0),
        ..ONE_PX_PER_M
    };
    let out = side_by_side(&[a.clone(), a], &settings).unwrap();
    let gap = north_m(&out[1], 0.0, 0.0)[0] - north_m(&out[0], 0.0, 0.0)[0];
    assert_near(gap, 5.0 * 1.194_329_5, 1e-3);
}

#[test]
fn the_zoom_scale_halves_each_whole_zoom_and_shrinks_in_between() {
    let proj = Projection::at(Point::new(-6.26, 53.35));
    let at = |z: f64| proj.meters_per_pixel(Scale::Zoom(z));
    for z in 0..24 {
        assert_eq!(at(f64::from(z + 1)), at(f64::from(z)) / 2.0, "zoom {z}");
    }
    // Every hundredth of a zoom from 0 to 24: never bigger than the zoom before it.
    let mut last = at(0.0);
    for k in 1..=2400 {
        let now = at(f64::from(k) / 100.0);
        assert!(now <= last, "zoom {}", f64::from(k) / 100.0);
        last = now;
    }
    let platform = 2.0 * PI * EARTH_RADIUS_M * proj.cos_lat0 / (TILE_PX * 15.37_f64.exp2());
    assert!((at(15.37) - platform).abs() <= 4.0 * f64::EPSILON * platform);
}

#[test]
fn projection_is_in_ground_meters() {
    // One degree of latitude is about 111 km anywhere; one of longitude shrinks with cos(lat).
    let proj = Projection::at(Point::new(10.013, 40.69));
    let north = proj.forward(Point::new(10.013, 40.70)).y;
    assert_near(north, 1_111.9, 2.0);
    let east = proj.forward(Point::new(10.023, 40.69)).x;
    assert_near(east, 1_111.9 * 40.69_f64.to_radians().cos(), 2.0);
    assert_eq!(
        Projection::around(&[]),
        Projection::at(Point::new(0.0, 0.0))
    );
}

/// How far each output point is from the edges its route's walk runs along (meters, the
/// pipeline's own projection); for a route on no edge, from its input.
fn distances_to_walks(lines: &[Vec<Point>], out: &[Vec<Point>]) -> Vec<Vec<f64>> {
    let proj = Projection::around(lines);
    let m: Vec<Vec<Point>> = lines
        .iter()
        .map(|l| l.iter().map(|&q| proj.forward(q)).collect())
        .collect();
    let (t, walks) = topology::build_with_traversals(&m, &Params::default()).unwrap();
    out.iter()
        .enumerate()
        .map(|(i, o)| {
            let walk = &walks[LineId::new(i)];
            let paths: Vec<&[Point]> = if walk.is_empty() {
                vec![&m[i][..]]
            } else {
                walk.iter().map(|&(e, _)| &t.edges[e].path[..]).collect()
            };
            o.iter()
                .map(|&q| {
                    let q = proj.forward(q);
                    paths
                        .iter()
                        .flat_map(|p| p.windows(2))
                        .map(|w| crate::topology::distance_to_segment(q, w[0], w[1]))
                        .fold(f64::INFINITY, f64::min)
                })
                .collect()
        })
        .collect()
}

/// Found by the property test: two lines crossing at a shallow angle, opposite ways. The
/// topology used to put a node about 70 m off line 0's straight path, and the output followed
/// that graph (fixed: runs read their direction by their ends). Now each line stays near its own
/// input, not only near its walk.
#[test]
fn a_shallow_crossing_stays_near_its_input() {
    let p = |x, y| Point::new(x, y);
    let lines = vec![
        vec![
            p(0.000_960_511_790_084_857, 0.000_316_136_235_505_819_93),
            p(0.002_310_180_717_106_394, 0.002_260_053_060_085_07),
        ],
        vec![
            p(0.002_455_126_500_586_028, 0.001_723_927_413_081_493_2),
            p(-0.000_613_596_688_626_369_5, -0.000_308_907_678_952_956_17),
            p(-0.000_486_827_753_188_205_7, 0.002_114_414_941_535_428),
        ],
    ];
    let out = side_by_side(&lines, &ONE_PX_PER_M).unwrap();
    // Two lines, slots ±2.5 m; connectors and fronts add a few meters at most.
    for (i, d) in distances_to_walks(&lines, &out).iter().enumerate() {
        let worst = d.iter().copied().fold(0.0, f64::max);
        assert!(worst < 15.0, "line {i}: {worst} m from its walk");
    }
    let proj = Projection::around(&lines);
    for (i, (o, l)) in out.iter().zip(&lines).enumerate() {
        let input: Vec<Point> = l.iter().map(|&q| proj.forward(q)).collect();
        for &q in o {
            let q = proj.forward(q);
            let d = input
                .windows(2)
                .map(|w| crate::topology::distance_to_segment(q, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            assert!(d < 15.0, "line {i}: a point {d} m from its input");
        }
    }
}

#[test]
fn stations_get_a_bar_across_their_street() {
    // Two lines on one street (5 m pitch, 4 m wide at 1 m per pixel): the bar spans both,
    // 2.5 + 2 m either side of the street, square to it.
    let a = east(10.013, 40.69, 400.0);
    let proj = Projection::at(Point::new(10.013, 40.69));
    let on_street = proj.inverse(Point::new(200.0, 3.0));
    let far = proj.inverse(Point::new(200.0, 300.0));
    let d = side_by_side_with_stations(&[a.clone(), a.clone()], &[on_street, far], &ONE_PX_PER_M)
        .unwrap();
    assert_eq!(
        d.lines,
        side_by_side(&[a.clone(), a], &ONE_PX_PER_M).unwrap()
    );
    let (p, q) = d.bars[0].unwrap();
    let (p, q) = (proj.forward(p), proj.forward(q));
    assert_near(p.x, 200.0, 1e-6);
    assert_near(q.x, 200.0, 1e-6);
    assert_near(p.y.min(q.y), -4.5, 1e-6);
    assert_near(p.y.max(q.y), 4.5, 1e-6);
    assert_eq!(d.bars[1], None, "300 m away: no route there");
}

/// Two stops named alike on both sides of the street share one bar, at their average;
/// unnamed or differently named stops keep their own; groups must be one per station.
#[test]
fn same_name_stops_share_a_bar() {
    let a = east(10.013, 40.69, 400.0);
    let proj = Projection::at(Point::new(10.013, 40.69));
    let pair = [
        proj.inverse(Point::new(190.0, 8.0)),
        proj.inverse(Point::new(210.0, -8.0)),
    ];
    let lines = [a.clone(), a];
    let x_of = |d: &Drawn| -> Vec<f64> {
        d.bars
            .iter()
            .map(|b| {
                let (p, q) = b.unwrap();
                (proj.forward(p).x + proj.forward(q).x) / 2.0
            })
            .collect()
    };
    let named = side_by_side_with_named_stations(&lines, &pair, &[7, 7], &ONE_PX_PER_M).unwrap();
    let x = x_of(&named);
    assert_near(x[0], 200.0, 1e-6);
    assert_eq!(
        named.bars[0], named.bars[1],
        "one bar, drawn for each station"
    );
    for groups in [&[0, 0][..], &[1, 2], &[]] {
        let own = side_by_side_with_named_stations(&lines, &pair, groups, &ONE_PX_PER_M).unwrap();
        let x = x_of(&own);
        assert_near(x[0], 190.0, 1e-6);
        assert_near(x[1], 210.0, 1e-6);
        assert_eq!(own.lines, named.lines, "stations never move the lines");
    }
    assert_eq!(
        side_by_side_with_named_stations(&lines, &pair, &[7], &ONE_PX_PER_M),
        Err(PipelineError::StationGroups {
            stations: 2,
            groups: 1
        })
    );
    let err = PipelineError::StationGroups {
        stations: 2,
        groups: 3,
    };
    assert!(err.to_string().contains("one per station"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn projection_round_trips(center in (-179.0f64..179.0, -85.0f64..85.0),
                              d in (-0.05f64..0.05, -0.05f64..0.05)) {
        let proj = Projection::at(Point::new(center.0, center.1));
        let p = Point::new(center.0 + d.0, (center.1 + d.1).clamp(-MAX_LATITUDE, MAX_LATITUDE));
        let back = proj.inverse(proj.forward(p));
        prop_assert!((back.x - p.x).abs() < 1e-9 && (back.y - p.y).abs() < 1e-9);
    }

    /// One output route per input, finite, deterministic, and every point near the edges
    /// its walk runs along: within the widest slot (here (6 − 1) / 2 pitches of 5 m) times the
    /// miter limit, plus room for fronts and connectors. (How close the graph is to the input
    /// is the topology's job, tested there.)
    #[test]
    fn output_follows_its_walk(lines in city_lines()) {
        let valid = lines.iter().all(|l| l.iter().any(|p| p != &l[0]));
        prop_assume!(valid);
        let out = side_by_side(&lines, &ONE_PX_PER_M).unwrap();
        prop_assert_eq!(out.len(), lines.len());
        prop_assert_eq!(&out, &side_by_side(&lines, &ONE_PX_PER_M).unwrap());
        prop_assert!(out.iter().all(|o| o.len() >= 2));
        prop_assert!(out.iter().flatten().all(|q| q.x.is_finite() && q.y.is_finite()));
        let reach = 2.5 * 5.0 * crate::polyline::DEFAULT_MITER_LIMIT + 2.0 * Params::default().merge_m;
        for (i, d) in distances_to_walks(&lines, &out).iter().enumerate() {
            let worst = d.iter().copied().fold(0.0, f64::max);
            prop_assert!(worst <= reach, "line {} has a point {} m from its walk", i, worst);
        }
    }
}
