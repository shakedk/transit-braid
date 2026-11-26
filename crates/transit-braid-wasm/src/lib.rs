//! WebAssembly bindings for `transit-braid-core`.
//!
//! Data passes between JavaScript and this WebAssembly module as flat `Float64Array`s, never
//! as JSON, so large inputs copy once and need no per-object work.

use transit_braid_core::geom::Point;
use transit_braid_core::pipeline::{self, Scale, Settings};
use wasm_bindgen::prelude::*;

mod refusal;

pub use refusal::Refusal;

/// The pipeline's answer ([`side_by_side`]), flat like its input. JavaScript takes each array
/// out once (`takeCoords()`…): the array moves out of the result, so Rust never copies it.
#[wasm_bindgen]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SideBySideResult {
    coords: Vec<f64>,
    starts: Vec<u32>,
    bars: Vec<f64>,
    dropped: Vec<u32>,
}

#[wasm_bindgen]
impl SideBySideResult {
    /// Every point, flat: `[lng, lat, lng, lat, ...]`. Moved out: a second take gives none.
    #[wasm_bindgen(js_name = takeCoords)]
    pub fn take_coords(&mut self) -> Vec<f64> {
        std::mem::take(&mut self.coords)
    }

    /// Where each route starts in `coords`, counted in points (not numbers). Moved out.
    #[wasm_bindgen(js_name = takeStarts)]
    pub fn take_starts(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.starts)
    }

    /// One bar per station, flat: `[lng, lat, lng, lat]` for its two ends, or four NaNs for a
    /// station with no route within 25 m. Moved out.
    #[wasm_bindgen(js_name = takeBars)]
    pub fn take_bars(&mut self) -> Vec<f64> {
        std::mem::take(&mut self.bars)
    }

    /// The routes a lane cap left out, as input indices, ascending; empty without a cap.
    /// `coords` and `starts` then hold the kept routes only, in input order. Moved out.
    #[wasm_bindgen(js_name = takeDropped)]
    pub fn take_dropped(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.dropped)
    }
}

/// What the pipeline keeps between calls ([`pipeline::Cache`]): the line graph and the order of
/// the last routes, so a zoom change reruns geometry only and an edit rebuilds incrementally and
/// orders warm. JavaScript makes a fresh one for a call that must not depend on earlier calls,
/// and keeps one for each `createSession()` (an editor). Its memory is freed by `free()`.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct EngineCache {
    cache: pipeline::Cache,
}

#[wasm_bindgen]
impl EngineCache {
    /// An empty cache: its first call runs every stage.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The whole pipeline (`transit_braid_core::pipeline`), the one entry point: routes of
    /// `[lng, lat]` in, one side-by-side route each out, same order and direction, using and
    /// updating this cache.
    ///
    /// `coords` is flat `[lng, lat, lng, lat, ...]`; `starts` gives where each route starts,
    /// counted in points (the first is 0). `route_keys` gives each route the same number for the
    /// same GeoJSON id from call to call (JavaScript numbers them), so an edit is matched by id
    /// rather than by place in the list; keys that aren't one per route, or repeat, are ignored
    /// (matched by place). `stations` is flat `[lng, lat, ...]`, one point each; `station_groups`
    /// gives each station's name as a number, so same-name stops share a bar (0 = no name; empty
    /// = no names). Give exactly one of `zoom` and `meters_per_pixel`; `line_width` and `spacing`
    /// are in pixels. `max_side_by_side` is the lane cap: at most that many lines side by side on
    /// any street; routes are kept in input order while every street they run along has room, the
    /// others are left out whole and listed in the result's `dropped`, and the kept routes are
    /// drawn on their own.
    ///
    /// # Errors
    /// A [`Refusal`] (thrown to JavaScript): its code says why (more than 70 routes, bad
    /// coordinates, a route with fewer than two distinct points, bad settings, a cap of 0, a
    /// bad layout…) and its fields which route, point or station. A refused call leaves the
    /// cache as it was.
    #[expect(
        clippy::too_many_arguments,
        reason = "flat typed arrays and plain numbers cross the WASM boundary; one struct would need serde"
    )]
    #[wasm_bindgen(js_name = sideBySide)]
    pub fn side_by_side(
        &mut self,
        coords: &[f64],
        starts: &[u32],
        route_keys: &[u32],
        stations: &[f64],
        station_groups: &[u32],
        zoom: Option<f64>,
        meters_per_pixel: Option<f64>,
        line_width: f64,
        spacing: f64,
        max_side_by_side: Option<u32>,
    ) -> Result<SideBySideResult, Refusal> {
        let call = Call {
            coords,
            starts,
            route_keys,
            stations,
            station_groups,
            zoom,
            meters_per_pixel,
            line_width,
            spacing,
        };
        let cap = max_side_by_side.map(|m| usize::try_from(m).unwrap_or(usize::MAX));
        run_flat(&mut self.cache, &call, cap)
    }
}

/// One call's flat input, as the exports take it.
struct Call<'a> {
    coords: &'a [f64],
    starts: &'a [u32],
    route_keys: &'a [u32],
    stations: &'a [f64],
    station_groups: &'a [u32],
    zoom: Option<f64>,
    meters_per_pixel: Option<f64>,
    line_width: f64,
    spacing: f64,
}

/// Runs the pipeline on `call` with `cache`, capped when `cap` is given: the flat result, with the
/// dropped routes (none without a cap).
fn run_flat(
    cache: &mut pipeline::Cache,
    call: &Call<'_>,
    cap: Option<usize>,
) -> Result<SideBySideResult, Refusal> {
    let scale = match (call.zoom, call.meters_per_pixel) {
        (Some(z), None) => Scale::Zoom(z),
        (None, Some(m)) => Scale::MetersPerPixel(m),
        _ => {
            return Err(Refusal::of_call(
                "BAD_OPTION",
                "give exactly one of zoom and metersPerPixel".to_owned(),
            ));
        }
    };
    let lines = split(call.coords, call.starts)?;
    let settings = Settings {
        scale,
        line_width_px: call.line_width,
        spacing_px: call.spacing,
    };
    if !call.stations.len().is_multiple_of(2) {
        return Err(Refusal::layout(format!(
            "stations length must be even (lng, lat pairs), got {}",
            call.stations.len()
        )));
    }
    let (pairs, _) = call.stations.as_chunks::<2>();
    let stations: Vec<Point> = pairs.iter().map(|&[x, y]| Point::new(x, y)).collect();
    let (out, dropped) = match cap {
        None => pipeline::side_by_side_cached_with_ids(
            cache,
            call.route_keys,
            &lines,
            &stations,
            call.station_groups,
            &settings,
        )
        .map(|drawn| (drawn, Vec::new())),
        Some(max) => pipeline::side_by_side_capped(
            cache,
            call.route_keys,
            &lines,
            &stations,
            call.station_groups,
            &settings,
            max,
        )
        .map(|capped| (capped.drawn, capped.dropped)),
    }
    .map_err(Refusal::from)?;

    let mut flat = Vec::with_capacity(out.lines.iter().map(|l| 2 * l.len()).sum());
    let mut out_starts = Vec::with_capacity(out.lines.len());
    for line in &out.lines {
        let start = u32::try_from(flat.len() / 2)
            .map_err(|_| Refusal::layout("result too large".to_owned()))?;
        out_starts.push(start);
        flat.extend(line.iter().flat_map(|p| [p.x, p.y]));
    }
    let bars = out
        .bars
        .iter()
        .flat_map(|b| match b {
            Some((a, c)) => [a.x, a.y, c.x, c.y],
            None => [f64::NAN; 4],
        })
        .collect();
    let dropped = dropped
        .into_iter()
        .map(|i| u32::try_from(i).map_err(|_| Refusal::layout("result too large".to_owned())))
        .collect::<Result<Vec<u32>, Refusal>>()?;
    Ok(SideBySideResult {
        coords: flat,
        starts: out_starts,
        bars,
        dropped,
    })
}

/// Cuts flat `coords` into routes at `starts` (in points), checking the layout.
fn split(coords: &[f64], starts: &[u32]) -> Result<Vec<Vec<Point>>, Refusal> {
    if !coords.len().is_multiple_of(2) {
        return Err(Refusal::layout(format!(
            "coords length must be even (lng, lat pairs), got {}",
            coords.len()
        )));
    }
    let (pairs, _) = coords.as_chunks::<2>();
    let points: Vec<Point> = pairs.iter().map(|&[x, y]| Point::new(x, y)).collect();
    if starts.is_empty() {
        return if points.is_empty() {
            Ok(Vec::new())
        } else {
            Err(Refusal::layout(
                "coords given but starts is empty".to_owned(),
            ))
        };
    }
    if starts[0] != 0 {
        return Err(Refusal::layout(format!(
            "starts must begin at 0, got {}",
            starts[0]
        )));
    }
    for (i, &start) in starts.iter().enumerate() {
        let before = if i == 0 { 0 } else { starts[i - 1] };
        if start < before || start as usize > points.len() {
            return Err(Refusal::layout(format!(
                "starts[{i}] = {start} is out of order or past the {} points",
                points.len()
            )));
        }
    }
    let mut lines = Vec::with_capacity(starts.len());
    for (i, &start) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map_or(points.len(), |&e| e as usize);
        lines.push(points[start as usize..end].to_vec());
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    /// The flat result's `(coords, starts, bars)`.
    type Flat = (Vec<f64>, Vec<u32>, Vec<f64>);

    thread_local! {
        /// One cache per test thread: the helpers below call through it, so a test's calls
        /// see each other's cache, as an editor's calls do.
        static ENGINE: RefCell<EngineCache> = RefCell::new(EngineCache::new());
    }

    /// [`EngineCache::side_by_side`] on this thread's cache.
    #[expect(
        clippy::too_many_arguments,
        reason = "the same flat arguments as the export it calls"
    )]
    fn side_by_side(
        coords: &[f64],
        starts: &[u32],
        route_keys: &[u32],
        stations: &[f64],
        station_groups: &[u32],
        zoom: Option<f64>,
        meters_per_pixel: Option<f64>,
        line_width: f64,
        spacing: f64,
        max_side_by_side: Option<u32>,
    ) -> Result<SideBySideResult, Refusal> {
        ENGINE.with(|engine| {
            engine.borrow_mut().side_by_side(
                coords,
                starts,
                route_keys,
                stations,
                station_groups,
                zoom,
                meters_per_pixel,
                line_width,
                spacing,
                max_side_by_side,
            )
        })
    }

    /// [`side_by_side`] uncapped, its arrays taken out as JavaScript does.
    #[expect(
        clippy::too_many_arguments,
        reason = "the same flat arguments as the export it calls"
    )]
    fn side_by_side_flat_with_ids(
        coords: &[f64],
        starts: &[u32],
        route_keys: &[u32],
        stations: &[f64],
        station_groups: &[u32],
        zoom: Option<f64>,
        meters_per_pixel: Option<f64>,
        line_width: f64,
        spacing: f64,
    ) -> Result<Flat, Refusal> {
        let mut out = side_by_side(
            coords,
            starts,
            route_keys,
            stations,
            station_groups,
            zoom,
            meters_per_pixel,
            line_width,
            spacing,
            None,
        )?;
        Ok((out.take_coords(), out.take_starts(), out.take_bars()))
    }

    /// [`side_by_side`] with a lane cap: the flat result and the dropped routes.
    #[expect(
        clippy::too_many_arguments,
        reason = "the same flat arguments as the export it calls"
    )]
    fn side_by_side_flat_capped(
        coords: &[f64],
        starts: &[u32],
        route_keys: &[u32],
        stations: &[f64],
        station_groups: &[u32],
        zoom: Option<f64>,
        meters_per_pixel: Option<f64>,
        line_width: f64,
        spacing: f64,
        max_side_by_side: u32,
    ) -> Result<(Flat, Vec<u32>), Refusal> {
        let mut out = side_by_side(
            coords,
            starts,
            route_keys,
            stations,
            station_groups,
            zoom,
            meters_per_pixel,
            line_width,
            spacing,
            Some(max_side_by_side),
        )?;
        let dropped = out.take_dropped();
        Ok((
            (out.take_coords(), out.take_starts(), out.take_bars()),
            dropped,
        ))
    }

    /// [`side_by_side_flat_with_ids`] without route keys (matched by place).
    #[expect(
        clippy::too_many_arguments,
        reason = "the same flat arguments as the export it calls"
    )]
    fn side_by_side_flat(
        coords: &[f64],
        starts: &[u32],
        stations: &[f64],
        station_groups: &[u32],
        zoom: Option<f64>,
        meters_per_pixel: Option<f64>,
        line_width: f64,
        spacing: f64,
    ) -> Result<Flat, Refusal> {
        side_by_side_flat_with_ids(
            coords,
            starts,
            &[],
            stations,
            station_groups,
            zoom,
            meters_per_pixel,
            line_width,
            spacing,
        )
    }

    /// How many times each pipeline stage has run in this thread's cache:
    /// `[topology, ordering, geometry]`.
    fn stage_runs() -> [u64; 3] {
        ENGINE.with(|engine| {
            let runs = engine.borrow().cache.runs();
            [runs.topology, runs.ordering, runs.geometry]
        })
    }

    /// Two copies of a 400 m street running east at the equator, flat.
    fn two_lines() -> (Vec<f64>, Vec<u32>) {
        let street = [0.0, 0.0, 0.003_6, 0.0];
        ([street, street].concat(), vec![0, 2])
    }

    /// With route keys, the same routes listed the other way round come back as the same
    /// lines, each still where it was (matched by key, not by place); bad keys are ignored.
    #[test]
    fn side_by_side_with_keys_keeps_each_line_through_a_reorder() {
        // Two streets 100 m apart, a route on each, plus a third sharing the first street.
        let a = [0.0, 0.0, 0.003_6, 0.0];
        let b = [0.0, 0.000_9, 0.003_6, 0.000_9];
        let coords = [a, b, a].concat();
        let starts = [0, 2, 4];
        let run = |coords: &[f64], keys: &[u32]| {
            side_by_side_flat_with_ids(coords, &starts, keys, &[], &[], None, Some(1.0), 4.0, 1.0)
                .unwrap()
        };
        let (first, _, _) = run(&coords, &[10, 20, 30]);
        let line = |flat: &[f64], k: usize| flat[4 * k..4 * k + 4].to_vec();
        let reversed = [a, b, a].into_iter().rev().collect::<Vec<_>>().concat();
        let (second, _, _) = run(&reversed, &[30, 20, 10]);
        assert_eq!(line(&second, 0), line(&first, 2));
        assert_eq!(line(&second, 1), line(&first, 1));
        assert_eq!(line(&second, 2), line(&first, 0));
        // Repeated or too few keys: matched by place, still a good answer.
        for bad in [&[1_u32, 1, 1][..], &[1][..]] {
            let (out, out_starts, _) = run(&coords, bad);
            assert_eq!((out.len(), out_starts.len()), (12, 3));
        }
        // Same errors as without keys.
        assert!(
            side_by_side_flat_with_ids(&[0.0], &[0], &[1], &[], &[], Some(15.0), None, 4.0, 1.0)
                .is_err()
        );
    }

    #[test]
    fn side_by_side_returns_one_line_each_in_the_same_layout() {
        let (coords, starts) = two_lines();
        let (out, out_starts, _) =
            side_by_side_flat(&coords, &starts, &[], &[], None, Some(1.0), 4.0, 1.0).unwrap();
        assert_eq!(out_starts, vec![0, 2]);
        assert_eq!(out.len(), 8);
        // 2.5 m either side of the street: about 2.25e-5 degrees of latitude.
        let (a, b) = (out[1], out[5]);
        assert!(
            (a + b).abs() < 1e-9 && (a.abs() - 2.5 / 111_320.0).abs() < 1e-7,
            "{a} {b}"
        );
        let (zoomed, _, _) =
            side_by_side_flat(&coords, &starts, &[], &[], Some(16.0), None, 4.0, 1.0).unwrap();
        assert!(
            zoomed[1].abs() > a.abs(),
            "zoom 16 is about 1.19 m per pixel"
        );
    }

    #[test]
    fn stations_come_back_as_bars_four_numbers_each() {
        let (coords, starts) = two_lines();
        // One station on the street, one 500 m north of it.
        let stations = [0.001_8, 0.000_01, 0.001_8, 0.004_5];
        let (_, _, bars) =
            side_by_side_flat(&coords, &starts, &stations, &[], None, Some(1.0), 4.0, 1.0).unwrap();
        assert_eq!(bars.len(), 8);
        assert!(bars[..4].iter().all(|v| v.is_finite()), "{bars:?}");
        assert!(
            (bars[0] - 0.001_8).abs() < 1e-9 && (bars[2] - 0.001_8).abs() < 1e-9,
            "square to the street"
        );
        assert!(
            bars[4..].iter().all(|v| v.is_nan()),
            "no route near: no bar"
        );
        let (_, _, none) =
            side_by_side_flat(&coords, &starts, &[], &[], None, Some(1.0), 4.0, 1.0).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn same_name_stations_share_a_bar_and_groups_must_match() {
        let (coords, starts) = two_lines();
        // Two stops 40 m apart along the street (0.000 36°), one name: one bar between them.
        let stations = [0.001_6, 0.000_01, 0.001_96, -0.000_01];
        let run = |g: &[u32]| {
            side_by_side_flat(&coords, &starts, &stations, g, None, Some(1.0), 4.0, 1.0)
        };
        let (_, _, merged) = run(&[1, 1]).unwrap();
        assert_eq!(merged[..4], merged[4..], "one bar for both");
        assert!(
            (merged[0] - 0.001_78).abs() < 1e-6,
            "at their average: {merged:?}"
        );
        let (_, _, own) = run(&[1, 2]).unwrap();
        assert!((own[0] - 0.001_6).abs() < 1e-6 && (own[4] - 0.001_96).abs() < 1e-6);
        assert_eq!(run(&[]).unwrap(), run(&[0, 0]).unwrap());
        assert!(
            run(&[1])
                .unwrap_err()
                .to_string()
                .contains("one per station")
        );
        assert!(
            run(&[1, 1, 1])
                .unwrap_err()
                .to_string()
                .contains("one per station")
        );
    }

    /// A second call with the same routes at another zoom runs geometry only.
    #[test]
    fn a_zoom_change_reruns_geometry_only() {
        let (coords, starts) = two_lines();
        let run = |z: f64| side_by_side_flat(&coords, &starts, &[], &[], Some(z), None, 4.0, 1.0);
        let first = run(14.0).unwrap();
        let before = stage_runs();
        let again = run(17.0).unwrap();
        let after = stage_runs();
        assert_eq!(
            [
                after[0] - before[0],
                after[1] - before[1],
                after[2] - before[2]
            ],
            [0, 0, 1]
        );
        assert_ne!(first.0, again.0, "the lines move apart at the new zoom");
        assert_eq!(run(14.0).unwrap(), first, "and come back the same");
    }

    #[test]
    fn bad_stations_are_refused() {
        let (coords, starts) = two_lines();
        let run =
            |st: &[f64]| side_by_side_flat(&coords, &starts, st, &[], None, Some(1.0), 4.0, 1.0);
        assert!(
            run(&[0.0, 0.0, 1.0])
                .unwrap_err()
                .to_string()
                .contains("stations length must be even")
        );
        assert!(
            run(&[0.0, 0.0, f64::NAN, 0.0])
                .unwrap_err()
                .to_string()
                .contains("station 1: needs a finite")
        );
        assert!(
            run(&[0.0, 95.0])
                .unwrap_err()
                .to_string()
                .contains("station 0")
        );
    }

    #[test]
    fn side_by_side_refuses_a_bad_layout() {
        let (coords, _) = two_lines();
        let run =
            |c: &[f64], s: &[u32]| side_by_side_flat(c, s, &[], &[], None, Some(1.0), 4.0, 1.0);
        assert!(
            run(&coords[..7], &[0, 2])
                .unwrap_err()
                .to_string()
                .contains("even")
        );
        assert!(
            run(&coords, &[])
                .unwrap_err()
                .to_string()
                .contains("starts is empty")
        );
        assert!(
            run(&coords, &[1, 2])
                .unwrap_err()
                .to_string()
                .contains("begin at 0")
        );
        assert!(
            run(&coords, &[0, 3, 2])
                .unwrap_err()
                .to_string()
                .contains("starts[2]")
        );
        assert!(
            run(&coords, &[0, 5])
                .unwrap_err()
                .to_string()
                .contains("starts[1]")
        );
        assert_eq!(run(&[], &[]).unwrap(), (vec![], vec![], vec![]));
    }

    #[test]
    fn side_by_side_passes_pipeline_errors_on() {
        let (coords, starts) = two_lines();
        let both = side_by_side_flat(&coords, &starts, &[], &[], Some(14.0), Some(1.0), 4.0, 1.0);
        assert!(both.unwrap_err().to_string().contains("exactly one"));
        let neither = side_by_side_flat(&coords, &starts, &[], &[], None, None, 4.0, 1.0);
        assert!(neither.unwrap_err().to_string().contains("exactly one"));
        let width = side_by_side_flat(&coords, &starts, &[], &[], Some(14.0), None, 0.0, 1.0);
        assert!(width.unwrap_err().to_string().contains("line width"));
        let mut bad = coords.clone();
        bad[2] = f64::NAN;
        let err = side_by_side_flat(&bad, &starts, &[], &[], Some(14.0), None, 4.0, 1.0)
            .unwrap_err()
            .to_string();
        assert!(err.contains("route 0, point 1"), "{err}");
        // A route of one point (starts 0, 1: the first has one point).
        let err =
            side_by_side_flat(&coords, &[0, 1], &[], &[], Some(14.0), None, 4.0, 1.0).unwrap_err();
        assert_eq!(
            err.to_string(),
            "route 0: needs at least two distinct points"
        );
        let many: Vec<u32> = (0..71).collect();
        let err = side_by_side_flat(&[0.0; 142], &many, &[], &[], Some(14.0), None, 4.0, 1.0)
            .unwrap_err();
        assert!(err.to_string().contains("at most 70"), "{err}");
    }

    /// Each refusal carries its stable code and the indices of what is at fault, apart from the
    /// reason, so JavaScript names the route or station without reading the text.
    #[test]
    fn refusals_carry_a_code_and_the_route_point_or_station() {
        let (coords, starts) = two_lines();
        let run = |coords: &[f64], starts: &[u32], stations: &[f64], groups: &[u32]| {
            side_by_side_flat(coords, starts, stations, groups, Some(15.0), None, 4.0, 1.0)
                .unwrap_err()
        };
        let mut far = coords.clone();
        far[3] = 95.0; // route 0, point 1: latitude off the map
        let bad = run(&far, &starts, &[], &[]);
        assert_eq!(
            (
                bad.code_str(),
                bad.route(),
                bad.point(),
                bad.station(),
                bad.reason()
            ),
            (
                "BAD_COORDINATE",
                Some(0),
                Some(1),
                None,
                "needs a finite longitude from -180 to 180 and latitude from -85.05 to 85.05"
                    .to_owned()
            )
        );
        let short = run(&coords, &[0, 1], &[], &[]);
        assert_eq!(
            (short.code_str(), short.route(), short.point()),
            ("TOO_FEW_POINTS", Some(0), None)
        );
        let station = run(&coords, &starts, &[0.0, 0.0, 0.0, 95.0], &[]);
        assert_eq!(
            (station.code_str(), station.route(), station.station()),
            ("BAD_STATION", None, Some(1))
        );
        let jump = [0.0, 0.0, 179.9, 0.0, -179.9, 0.0];
        let date_line = run(&jump, &[0], &[], &[]);
        assert_eq!(
            (date_line.code_str(), date_line.route(), date_line.point()),
            ("CROSSES_ANTIMERIDIAN", Some(0), Some(2))
        );
        let many: Vec<u32> = (0..71).collect();
        assert_eq!(
            run(&[0.0; 142], &many, &[], &[]).code_str(),
            "TOO_MANY_ROUTES"
        );
        let wide = [0.0, 0.0, 3.0, 0.0];
        assert_eq!(run(&wide, &[0], &[], &[]).code_str(), "TOO_WIDE");
        let both = side_by_side_flat(&coords, &starts, &[], &[], Some(1.0), Some(1.0), 4.0, 1.0);
        assert_eq!(both.unwrap_err().code_str(), "BAD_OPTION");
        // The flat layout itself: the bindings' own refusals, never about a route.
        for layout in [
            run(&coords[..3], &starts, &[], &[]),
            run(&coords, &[1, 2], &[], &[]),
            run(&coords, &starts, &[0.0], &[]),
            run(&coords, &starts, &[0.0, 0.0], &[1, 2]),
        ] {
            assert_eq!(layout.code_str(), "BAD_LAYOUT", "{layout}");
            assert_eq!(
                (layout.route(), layout.point(), layout.station()),
                (None, None, None)
            );
        }
        // The code getter gives JavaScript the same text.
        assert_eq!(bad.code(), "BAD_COORDINATE");
    }

    /// Three routes on one street with a cap of 2 drop the third; the two kept come back
    /// laid out as the uncapped call draws them alone. A cap at or above the routes drops none.
    #[test]
    fn a_cap_drops_routes_and_lists_them() {
        let street = [0.0, 0.0, 0.003_6, 0.0];
        let three = [street, street, street].concat();
        let ((out, out_starts, bars), dropped) = side_by_side_flat_capped(
            &three,
            &[0, 2, 4],
            &[],
            &[0.001, 0.000_01],
            &[],
            Some(16.0),
            None,
            4.0,
            1.0,
            2,
        )
        .unwrap();
        assert_eq!(dropped, vec![2]);
        assert_eq!(out_starts, vec![0, 2]);
        let (coords, starts) = two_lines();
        let alone = side_by_side_flat(
            &coords,
            &starts,
            &[0.001, 0.000_01],
            &[],
            Some(16.0),
            None,
            4.0,
            1.0,
        )
        .unwrap();
        assert_eq!((out, out_starts, bars), alone);
        for cap in [3, u32::MAX] {
            let (_, none) = side_by_side_flat_capped(
                &three,
                &[0, 2, 4],
                &[7, 8, 9],
                &[],
                &[],
                Some(16.0),
                None,
                4.0,
                1.0,
                cap,
            )
            .unwrap();
            assert!(none.is_empty(), "cap {cap}");
        }
    }

    /// Each array moves out of the result once: a second take gives none, and the others stay.
    /// A cap at or above the number of routes gives the uncapped call's result exactly.
    #[test]
    fn the_one_entry_point_moves_each_array_out_once() {
        let (coords, starts) = two_lines();
        let call = |cap: Option<u32>| {
            side_by_side(
                &coords,
                &starts,
                &[],
                &[0.001_8, 0.000_01],
                &[],
                Some(16.0),
                None,
                4.0,
                1.0,
                cap,
            )
            .unwrap()
        };
        let mut out = call(None);
        let whole = out.clone();
        let taken = out.take_coords();
        assert_eq!(taken.len(), 8);
        assert!(out.take_coords().is_empty(), "moved out, not copied");
        assert_eq!(out.take_starts(), vec![0, 2]);
        assert!(out.take_starts().is_empty());
        assert_eq!(out.take_bars().len(), 4);
        assert!(out.take_dropped().is_empty());
        assert_eq!(out, SideBySideResult::default(), "nothing left behind");
        for cap in [2, 3, u32::MAX] {
            assert_eq!(call(Some(cap)), whole, "cap {cap}");
        }
    }

    /// Two caches are independent: a call on one never changes what the other draws, and a
    /// fresh one draws what a first call draws.
    #[test]
    fn each_cache_is_its_own() {
        let (coords, starts) = two_lines();
        let call = |engine: &mut EngineCache, coords: &[f64], zoom: f64| {
            let mut out = engine
                .side_by_side(
                    coords,
                    &starts,
                    &[],
                    &[],
                    &[],
                    Some(zoom),
                    None,
                    4.0,
                    1.0,
                    None,
                )
                .unwrap();
            out.take_coords()
        };
        let (mut a, mut b) = (EngineCache::new(), EngineCache::new());
        let first = call(&mut a, &coords, 15.0);
        let mut moved = coords.clone();
        moved[3] = 0.000_05;
        call(&mut b, &moved, 15.0);
        call(&mut b, &moved, 17.0);
        assert_eq!(call(&mut a, &coords, 15.0), first);
        assert_eq!(a.cache.runs().topology, 1, "a's cache kept its graph");
        assert_eq!(
            (b.cache.runs().topology, b.cache.runs().geometry),
            (1, 2),
            "b's zoom change reran geometry only"
        );
        assert_eq!(call(&mut EngineCache::new(), &coords, 15.0), first);
    }

    /// A cap of 0 is refused with a message, and every error the uncapped call gives still
    /// comes first (layout, scale, stations).
    #[test]
    fn a_capped_call_refuses_what_the_uncapped_one_does() {
        let (coords, starts) = two_lines();
        let capped = |coords: &[f64], starts: &[u32], stations: &[f64], cap: u32| {
            side_by_side_flat_capped(
                coords,
                starts,
                &[],
                stations,
                &[],
                Some(15.0),
                None,
                4.0,
                1.0,
                cap,
            )
        };
        assert!(
            capped(&coords, &starts, &[], 0)
                .unwrap_err()
                .to_string()
                .contains("1 or more")
        );
        assert!(
            capped(&coords[..3], &starts, &[], 1)
                .unwrap_err()
                .to_string()
                .contains("even")
        );
        assert!(
            capped(&coords, &[1, 2], &[], 1)
                .unwrap_err()
                .to_string()
                .contains("begin at 0")
        );
        assert!(
            capped(&coords, &starts, &[0.0], 1)
                .unwrap_err()
                .to_string()
                .contains("even")
        );
        let bad = [f64::NAN, 0.0, 0.003_6, 0.0, 0.0, 0.0, 0.003_6, 0.0];
        assert!(
            capped(&bad, &starts, &[], 1)
                .unwrap_err()
                .to_string()
                .contains("route 0")
        );
        let both = side_by_side_flat_capped(
            &coords,
            &starts,
            &[],
            &[],
            &[],
            Some(15.0),
            Some(1.0),
            4.0,
            1.0,
            1,
        );
        assert!(both.unwrap_err().to_string().contains("exactly one"));
        let (none, dropped) = capped(&[], &[], &[], 1).unwrap();
        assert!(none.0.is_empty() && none.1.is_empty() && dropped.is_empty());
    }
}
