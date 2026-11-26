//! Input limits: bad, oversized or adversarial input is refused with its place, never a panic,
//! and the same way after other calls.

use proptest::prelude::*;

use super::limits::{check_call, check_size};
use super::test_support::*;
use super::*;

#[test]
fn more_than_seventy_routes_are_refused() {
    let lines = vec![east(0.0, 0.0, 100.0); MAX_ROUTES + 1];
    let err = side_by_side(&lines, &ONE_PX_PER_M).unwrap_err();
    assert_eq!(err, PipelineError::TooMany { count: 71 });
    assert!(err.to_string().contains("at most 70"));
    assert!(side_by_side(&lines[..MAX_ROUTES], &ONE_PX_PER_M).is_ok());
}

#[test]
fn bad_coordinates_are_refused_with_their_place() {
    let good = east(0.0, 0.0, 100.0);
    for bad in [
        Point::new(f64::NAN, 0.0),
        Point::new(0.0, f64::INFINITY),
        Point::new(180.5, 0.0),
        Point::new(-181.0, 0.0),
        Point::new(0.0, 86.0),
        Point::new(0.0, -90.0),
    ] {
        let mut line = good.clone();
        line[3] = bad;
        let err = side_by_side(&[good.clone(), line], &ONE_PX_PER_M).unwrap_err();
        assert_eq!(
            err,
            PipelineError::BadCoordinate { route: 1, point: 3 },
            "{bad:?}"
        );
        assert!(err.to_string().starts_with("route 1, point 3"));
    }
}

/// More than a million points, all routes together, are refused before any work;
/// a million pass the checks.
#[test]
fn more_than_a_million_points_are_refused() {
    // Two streets of half a million points each, 2 mm apart, 1 km long.
    let street = |lat: f64| -> Vec<Point> {
        let proj = Projection::at(Point::new(-6.26, lat));
        (0..500_000)
            .map(|k| proj.inverse(Point::new(f64::from(k) * 0.002, 0.0)))
            .collect()
    };
    let mut lines = vec![street(53.35), street(53.351)];
    assert_eq!(check_call(&lines, &[], &[], &ONE_PX_PER_M), Ok(()));
    let last = lines[1][lines[1].len() - 1];
    lines[1].push(last);
    let err = side_by_side(&lines, &ONE_PX_PER_M).unwrap_err();
    assert_eq!(err, PipelineError::TooManyPoints { count: 1_000_001 });
    assert_eq!(
        err.to_string(),
        "too many points: 1000001 given, at most 1000000 allowed (all routes together)"
    );
}

/// A route whose neighboring points are more than 180° of longitude apart crosses the date
/// line (GeoJSON writes Fiji's 2 km from 179.99 to -179.99 that way); it is refused with its
/// place, before any work. Without this check, two lines from -170 to 170 ran out of memory in
/// WebAssembly after 3 s.
#[test]
fn a_jump_across_the_date_line_is_refused_with_its_place() {
    let good = east(179.0, -16.8, 500.0);
    let across = vec![
        Point::new(179.98, -16.8),
        Point::new(179.99, -16.8),
        Point::new(-179.99, -16.8),
    ];
    let err = side_by_side(&[good.clone(), across], &ONE_PX_PER_M).unwrap_err();
    assert_eq!(
        err,
        PipelineError::CrossesAntimeridian { route: 1, point: 2 }
    );
    assert!(
        err.to_string()
            .starts_with("route 1, point 2: more than 180° of longitude"),
        "{err}"
    );
    let globe = vec![
        vec![Point::new(-170.0, 0.0), Point::new(170.0, 0.0)],
        vec![Point::new(-170.0, 1.0), Point::new(170.0, 1.0)],
    ];
    assert_eq!(
        side_by_side(&globe, &ONE_PX_PER_M).unwrap_err(),
        PipelineError::CrossesAntimeridian { route: 0, point: 1 }
    );
    // Exactly 180° is no jump; such a line is far too wide anyway.
    let half = vec![vec![Point::new(-90.0, 0.0), Point::new(90.0, 0.0)]];
    assert!(matches!(
        side_by_side(&half, &ONE_PX_PER_M).unwrap_err(),
        PipelineError::TooWide { .. }
    ));
    // Near the date line without crossing it is fine.
    assert!(side_by_side(&[good], &ONE_PX_PER_M).is_ok());
}

/// Two 100 m streets whose outer ends are `apart_m` meters apart, east-west (`east`) or
/// north-south, around `(lng, lat)`.
fn two_streets_apart(lng: f64, lat: f64, apart_m: f64, east: bool) -> Vec<Vec<Point>> {
    let proj = Projection::at(Point::new(lng, lat));
    let half = apart_m / 2.0;
    let at = |d: f64| {
        if east {
            proj.inverse(Point::new(d, 0.0))
        } else {
            proj.inverse(Point::new(0.0, d))
        }
    };
    vec![
        vec![at(-half), at(-half + 100.0)],
        vec![at(half - 100.0), at(half)],
    ]
}

/// The routes must fit in 200 km each way, measured in the pipeline's projection.
#[test]
fn routes_wider_than_200_km_are_refused() {
    for (lng, lat, east) in [
        (-6.26, 53.35, true),
        (-6.26, 53.35, false),
        (-113.5, 70.0, true),
    ] {
        let near = two_streets_apart(lng, lat, 199_900.0, east);
        assert!(side_by_side(&near, &ONE_PX_PER_M).is_ok(), "{lat} {east}");
        let far = two_streets_apart(lng, lat, 200_100.0, east);
        let err = side_by_side(&far, &ONE_PX_PER_M).unwrap_err();
        let PipelineError::TooWide {
            width_km,
            height_km,
        } = err
        else {
            panic!("{err:?}");
        };
        let (wide, other) = if east {
            (width_km, height_km)
        } else {
            (height_km, width_km)
        };
        assert_near(wide, 200.1, 1e-6);
        assert_near(other, 0.0, 1e-6);
        assert!(
            err.to_string().ends_with("at most 200 km each way"),
            "{err}"
        );
    }
}

/// One route about `km` kilometers long, in laps of 190 km east and west 1 km apart, so it
/// stays inside the 200 km box.
fn laps(km: f64) -> Vec<Point> {
    let proj = Projection::at(Point::new(10.0, 45.0));
    let mut points = vec![Point::new(0.0, 0.0)];
    let mut left = km * 1000.0;
    while left > 0.0 {
        let Point { x, y } = points[points.len() - 1];
        let next = if points.len() % 2 == 1 {
            let run = left.min(190_000.0);
            Point::new(if x == 0.0 { run } else { x - run }, y)
        } else {
            Point::new(x, y + 1000.0f64.min(left))
        };
        left -= next.distance(Point::new(x, y));
        points.push(next);
    }
    points.into_iter().map(|q| proj.inverse(q)).collect()
}

/// At most 2000 km of routes together. Refused before any work (a full draw of 2000 km
/// is too slow for a debug test, so the length under the bound is checked on its own).
#[test]
fn routes_longer_than_2000_km_together_are_refused() {
    // `laps` measures in its own projection, the check in one centered on the laps' middle:
    // the two differ by about 0.1%.
    let err = side_by_side(&[laps(2_010.0)], &ONE_PX_PER_M).unwrap_err();
    let PipelineError::TooLong { km } = err else {
        panic!("{err:?}");
    };
    assert_near(km, 2_010.0, 5.0);
    assert!(err.to_string().ends_with("at most 2000 km"), "{err}");
    // Two routes count together.
    let halves = vec![laps(1_005.0), laps(1_005.0)];
    assert!(matches!(
        side_by_side(&halves, &ONE_PX_PER_M),
        Err(PipelineError::TooLong { .. })
    ));
    assert_eq!(check_size(&[laps(1_990.0)]), Ok(()));
    assert_eq!(check_size(&[]), Ok(()));
}

#[test]
fn routes_without_two_distinct_points_are_refused() {
    let good = east(0.0, 0.0, 100.0);
    let p = Point::new(1.0, 1.0);
    for short in [vec![], vec![p], vec![p, p, p]] {
        let err = side_by_side(&[good.clone(), short], &ONE_PX_PER_M).unwrap_err();
        assert_eq!(err, PipelineError::TooShort { route: 1 });
    }
}

#[test]
fn bad_settings_are_refused() {
    let line = [east(0.0, 0.0, 100.0)];
    let with = |scale, line_width_px, spacing_px| Settings {
        scale,
        line_width_px,
        spacing_px,
    };
    for s in [
        with(Scale::Zoom(f64::NAN), 4.0, 1.0),
        with(Scale::Zoom(-1.0), 4.0, 1.0),
        with(Scale::Zoom(24.5), 4.0, 1.0),
        with(Scale::MetersPerPixel(0.0), 4.0, 1.0),
        with(Scale::MetersPerPixel(f64::INFINITY), 4.0, 1.0),
        with(Scale::Zoom(14.0), 0.0, 1.0),
        with(Scale::Zoom(14.0), f64::NAN, 1.0),
        with(Scale::Zoom(14.0), 4.0, -1.0),
        with(Scale::Zoom(14.0), 4.0, f64::INFINITY),
    ] {
        let err = side_by_side(&line, &s).unwrap_err();
        assert!(matches!(err, PipelineError::InvalidSettings(_)), "{s:?}");
    }
    // Bad settings are reported even with no routes.
    assert!(side_by_side(&[], &with(Scale::Zoom(-1.0), 4.0, 1.0)).is_err());
    assert!(side_by_side(&[], &with(Scale::Zoom(0.0), 4.0, 0.0)).is_ok());
}

#[test]
fn bad_stations_are_refused_with_their_index() {
    let line = [east(0.0, 0.0, 100.0)];
    for bad in [
        Point::new(f64::NAN, 0.0),
        Point::new(0.0, 89.0),
        Point::new(181.0, 0.0),
    ] {
        let err = side_by_side_with_stations(&line, &[Point::new(0.0, 0.0), bad], &ONE_PX_PER_M)
            .unwrap_err();
        assert_eq!(err, PipelineError::BadStation { station: 1 });
        assert!(
            err.to_string()
                .starts_with("station 1: needs a finite longitude")
        );
    }
    let many = vec![Point::new(0.0, 0.0); MAX_STATIONS + 1];
    let err = side_by_side_with_stations(&line, &many, &ONE_PX_PER_M).unwrap_err();
    assert_eq!(
        err,
        PipelineError::TooManyStations {
            count: MAX_STATIONS + 1
        }
    );
    assert!(err.to_string().contains("at most 5000"));
    assert!(side_by_side_with_stations(&line, &many[..MAX_STATIONS], &ONE_PX_PER_M).is_ok());
    // No routes: no bars, but stations are still checked.
    let none = side_by_side_with_stations(&[], &[Point::new(1.0, 1.0)], &ONE_PX_PER_M).unwrap();
    assert_eq!(
        none,
        Drawn {
            lines: vec![],
            bars: vec![None]
        }
    );
    assert!(side_by_side_with_stations(&[], &[Point::new(f64::NAN, 1.0)], &ONE_PX_PER_M).is_err());
}

/// One point of [`any_lines`]: a step from the point before (meters), or a place anywhere on
/// the map, or next to the date line on either side.
#[derive(Debug, Clone)]
enum Place {
    Step(f64, f64),
    Anywhere(f64, f64),
    DateLine(f64, f64),
}

/// 1–5 routes of 2–8 points around a city anywhere: mostly steps of up to 300 m, now
/// and then a jump to anywhere on the map or next to the date line.
fn any_lines() -> impl Strategy<Value = Vec<Vec<Point>>> {
    let place = prop_oneof![
        6 => (-300.0f64..300.0, -300.0f64..300.0).prop_map(|(x, y)| Place::Step(x, y)),
        1 => (-180.0f64..=180.0, -MAX_LATITUDE..=MAX_LATITUDE).prop_map(|(x, y)| Place::Anywhere(x, y)),
        1 => (-1.0f64..1.0, -60.0f64..60.0).prop_map(|(x, y)| Place::DateLine(x, y)),
    ];
    let center = (-180.0f64..180.0, -80.0f64..80.0).prop_map(|(x, y)| Point::new(x, y));
    let line = prop::collection::vec(place, 2..9);
    (center, prop::collection::vec(line, 1..6)).prop_map(|(c, ls)| {
        let proj = Projection::at(c);
        ls.into_iter()
            .map(|places| {
                let mut at = Point::new(0.0, 0.0);
                places
                    .into_iter()
                    .map(|p| match p {
                        Place::Step(x, y) => {
                            at = Point::new(at.x + x, at.y + y);
                            proj.inverse(at)
                        }
                        Place::Anywhere(x, y) => Point::new(x, y),
                        // Just east or west of 180°: -179.x or 179.x.
                        Place::DateLine(x, y) => {
                            Point::new(if x < 0.0 { 180.0 + x } else { x - 180.0 }, y)
                        }
                    })
                    .collect()
            })
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    /// Any routes (anywhere on the map, across the date line, spread over the globe)
    /// are drawn or refused with an error, never a panic; and the same routes get the same
    /// answer through a cache after other calls as in a call of their own.
    #[test]
    fn any_input_is_drawn_or_refused_the_same_way_after_other_calls(
        lines in any_lines(),
        before in city_lines(),
        stations in prop::collection::vec((-180.0f64..180.0, -80.0f64..80.0), 0..4),
    ) {
        let mut stations: Vec<Point> = stations.into_iter().map(|(x, y)| Point::new(x, y)).collect();
        stations.extend(lines.first().and_then(|l| l.get(1)).copied());
        let z = at_zoom(15.0);
        let alone = side_by_side_with_stations(&lines, &stations, &z);
        if let Ok(d) = &alone {
            prop_assert_eq!(d.lines.len(), lines.len());
            prop_assert_eq!(d.bars.len(), stations.len());
            prop_assert!(d.lines.iter().flatten().all(|q| q.x.is_finite() && q.y.is_finite()));
        }
        let mut cache = Cache::new();
        let _ = side_by_side_cached(&mut cache, &before, &[], &[], &z);
        let after = side_by_side_cached(&mut cache, &lines, &stations, &[], &z);
        prop_assert_eq!(after.map(|_| ()), alone.map(|_| ()));
    }

    /// Routes at city scale (up to 8 routes of up to 12 points, steps up to 500 m, a few
    /// stations) are always drawn: no limit or work bound refuses them.
    #[test]
    fn city_scale_routes_are_always_drawn(
        center in (-170.0f64..170.0, -75.0f64..75.0),
        walks in prop::collection::vec(prop::collection::vec((-500.0f64..500.0, -500.0f64..500.0), 1..12), 1..9),
        stops in prop::collection::vec((-2000.0f64..2000.0, -2000.0f64..2000.0), 0..6),
    ) {
        let proj = Projection::at(Point::new(center.0, center.1));
        let lines: Vec<Vec<Point>> = walks
            .iter()
            .map(|steps| {
                let mut at = Point::new(0.0, 0.0);
                let mut l = vec![proj.inverse(at)];
                for &(x, y) in steps {
                    at = Point::new(at.x + x, at.y + y);
                    l.push(proj.inverse(at));
                }
                l
            })
            .collect();
        let valid = lines.iter().all(|l| l.iter().any(|p| p != &l[0]));
        prop_assume!(valid);
        let stations: Vec<Point> = stops.iter().map(|&(x, y)| proj.inverse(Point::new(x, y))).collect();
        let drawn = side_by_side_with_stations(&lines, &stations, &at_zoom(16.0));
        prop_assert!(drawn.is_ok(), "{:?}", drawn.err());
    }
}
