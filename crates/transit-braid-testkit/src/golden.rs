//! Golden digests: a fingerprint of everything the pipeline draws for a fixture, so a
//! refactor can prove "same output, bit for bit", or show exactly what changed on purpose.
//!
//! For each fixture the pipeline runs, as `sideBySide()` runs it (4 px lines, 1 px apart, each
//! route with its own key, the fixture's stations given), and each output gets two digests:
//!
//! - **`zooms`**: every whole zoom from 10 to 19 and the fractional zooms [`FRACTIONAL_ZOOMS`],
//!   each from a fresh cache. A fractional zoom's meters per pixel come from the core's own
//!   `det::exp2`, so they have the same bits on every machine too.
//! - **`caps`**: the lane cap `maxSideBySide` 1, 2 and 3 at zoom 15, each from a fresh cache.
//! - **`edits`**: one fresh cache through a scripted edit sequence ([`EDIT_STEPS`]): an edit
//!   through the cache warm-starts the order (Q5), so this covers that path too.
//!
//! The digests are 64-bit FNV-1a ([`Fnv64`]) over a canonical byte form of the output; nothing
//! is rounded. **`lines`** covers the drawn lines in output order (each route's id, then every
//! coordinate's bits) and the dropped routes' ids; **`bars`** covers one entry per station, in
//! input order (none, or the bar's two ends). Every number is written as 8 little-endian bytes:
//! counts and lengths as `u64`, coordinates as `f64::to_bits`. `packages/transit-braid`'s
//! `golden.test.ts` computes the same digests from `sideBySide()` in WebAssembly.
//!
//! While digesting, the harness also checks two determinism rules and fails if one breaks: a
//! zoom-only rerun through the cache gives a fresh run's bits, and the topology an edit builds
//! incrementally equals a full build of the same routes.
//!
//! Fixtures in meters are placed around 0°, 0° ([`crate::place::lng_lat_lines`]). Stations are the
//! fixture's own stops (same-name stops share a name number, as `sideBySide()` numbers
//! `properties.name`), or, for a fixture without stops, one in the middle of every input segment.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use transit_braid_core::geom::Point;
use transit_braid_core::pipeline::{self, Cache, Drawn, MAX_STATIONS, Projection, Scale, Settings};
use transit_braid_core::topology::{self, Params};

use crate::{Fixture, Units};

/// Width of each line in pixels: `sideBySide()`'s default.
pub const LINE_WIDTH_PX: f64 = 4.0;
/// Gap between neighboring lines in pixels: `sideBySide()`'s default.
pub const SPACING_PX: f64 = 1.0;
/// The whole zooms digested, each from a fresh cache.
pub const ZOOMS: std::ops::RangeInclusive<u8> = 10..=19;
/// The fractional zooms digested, each from a fresh cache: a half, an uneven fraction and three
/// quarters, between the whole zooms.
pub const FRACTIONAL_ZOOMS: [f64; 3] = [12.5, 15.37, 17.75];
/// The lane caps digested (`maxSideBySide`).
pub const CAPS: [usize; 3] = [1, 2, 3];
/// The zoom of the capped runs.
pub const CAP_ZOOM: u8 = 15;
/// The zoom of every edit step but the last.
pub const EDIT_ZOOM: u8 = 15;
/// The zoom of the last edit step (a zoom-only change after the edits).
pub const EDIT_ZOOM_AFTER: u8 = 17;
/// How far the `2-move` step moves the middle point of the first route, in degrees (east, north).
pub const MOVE_BY_DEG: (f64, f64) = (0.0001, 0.000_05);
/// Meters per degree on both axes for fixtures in meters, placed around 0°, 0°
/// ([`crate::place::lng_lat_lines`]).
pub const PLACE_M_PER_DEG: f64 = 111_319.490_793_273_57;

/// The edit steps, in order, all through one fresh cache, each with the fixture's stations:
/// 1. `1-full`: every route, at [`EDIT_ZOOM`].
/// 2. `2-move`: the first route's middle point (index `len / 2`) moved by [`MOVE_BY_DEG`].
/// 3. `3-remove`: the last route removed.
/// 4. `4-add`: it is added back, at the end.
/// 5. `5-reverse`: the routes in reverse order (matched by id).
/// 6. `6-zoom`: the same at [`EDIT_ZOOM_AFTER`].
pub const EDIT_STEPS: [&str; 6] = [
    "1-full",
    "2-move",
    "3-remove",
    "4-add",
    "5-reverse",
    "6-zoom",
];

/// 64-bit FNV-1a, byte by byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fnv64(u64);

impl Default for Fnv64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    /// The empty input's state.
    #[must_use]
    pub fn new() -> Self {
        Self(Self::OFFSET)
    }

    /// Adds bytes.
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    /// Adds a number as 8 little-endian bytes.
    pub fn u64(&mut self, n: u64) {
        self.bytes(&n.to_le_bytes());
    }

    /// Adds a count or length as a `u64`.
    pub fn count(&mut self, n: usize) {
        self.u64(u64::try_from(n).unwrap_or(u64::MAX));
    }

    /// Adds a coordinate's exact bits.
    pub fn f64(&mut self, x: f64) {
        self.u64(x.to_bits());
    }

    /// Adds a text: its length in bytes, then its UTF-8 bytes.
    pub fn text(&mut self, s: &str) {
        self.count(s.len());
        self.bytes(s.as_bytes());
    }

    /// The digest so far.
    #[must_use]
    pub fn finish(&self) -> u64 {
        self.0
    }

    /// The digest as 16 lowercase hex digits.
    #[must_use]
    pub fn hex(&self) -> String {
        format!("{:016x}", self.0)
    }
}

/// The two digests of one output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pair {
    /// Lines in output order (id, then every point) and the dropped routes' ids.
    pub lines: String,
    /// One entry per station, in input order: none, or the bar's two ends.
    pub bars: String,
}

/// Digests one output: `ids` name `drawn.lines` in order, `dropped` the routes a cap left out.
#[must_use]
pub fn digest(ids: &[&str], drawn: &Drawn, dropped: &[&str]) -> Pair {
    let mut lines = Fnv64::new();
    lines.count(drawn.lines.len());
    for (k, line) in drawn.lines.iter().enumerate() {
        lines.text(ids.get(k).copied().unwrap_or_default());
        lines.count(line.len());
        for p in line {
            lines.f64(p.x);
            lines.f64(p.y);
        }
    }
    lines.count(dropped.len());
    for id in dropped {
        lines.text(id);
    }
    let mut bars = Fnv64::new();
    bars.count(drawn.bars.len());
    for bar in &drawn.bars {
        match bar {
            None => bars.bytes(&[0]),
            Some((a, b)) => {
                bars.bytes(&[1]);
                for x in [a.x, a.y, b.x, b.y] {
                    bars.f64(x);
                }
            }
        }
    }
    Pair {
        lines: lines.hex(),
        bars: bars.hex(),
    }
}

/// A fixture as the harness gives it to the pipeline, all `[lng, lat]`.
#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    /// Route ids, in fixture order.
    pub ids: Vec<String>,
    /// Each route's key (1, 2, …), as `sideBySide()` gives each id a number.
    pub keys: Vec<u32>,
    /// The routes.
    pub lines: Vec<Vec<Point>>,
    /// The stations.
    pub stations: Vec<Point>,
    /// Each station's name number (0 = no name), one per station, as `sideBySide()` sends them.
    pub groups: Vec<u32>,
}

/// The fixture's routes and stations in `[lng, lat]`: degrees as they are, meters placed around
/// 0°, 0° ([`PLACE_M_PER_DEG`]). Stations are the fixture's stops, or the middle of every input
/// segment, every `ceil(n / MAX_STATIONS)`-th when there are more than the pipeline takes.
#[must_use]
pub fn input(fixture: &Fixture) -> Input {
    let place = |p: Point| match fixture.units {
        Units::Degrees => p,
        Units::Meters => Point::new(p.x / PLACE_M_PER_DEG, p.y / PLACE_M_PER_DEG),
    };
    let lines: Vec<Vec<Point>> = fixture
        .lines
        .iter()
        .map(|l| l.points.iter().map(|&p| place(p)).collect())
        .collect();
    let (stations, groups) = if fixture.stations.is_empty() {
        let mids: Vec<Point> = lines
            .iter()
            .flat_map(|l| {
                l.windows(2)
                    .map(|w| Point::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5))
            })
            .collect();
        let stride = mids.len().div_ceil(MAX_STATIONS).max(1);
        let kept: Vec<Point> = mids.into_iter().step_by(stride).collect();
        let groups = vec![0; kept.len()];
        (kept, groups)
    } else {
        // Name numbers from 1, in order of first appearance (a lookup only, never iterated).
        let mut numbers: BTreeMap<&str, u32> = BTreeMap::new();
        let mut groups = Vec::with_capacity(fixture.stations.len());
        for s in &fixture.stations {
            groups.push(s.name.as_deref().map_or(0, |n| {
                let next = u32::try_from(numbers.len() + 1).unwrap_or(u32::MAX);
                *numbers.entry(n).or_insert(next)
            }));
        }
        (
            fixture.stations.iter().map(|s| place(s.at)).collect(),
            groups,
        )
    };
    Input {
        ids: fixture.lines.iter().map(|l| l.id.clone()).collect(),
        keys: (1..).take(fixture.lines.len()).collect(),
        lines,
        stations,
        groups,
    }
}

/// Every digest of one fixture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The fixture file, relative to the folder digested.
    pub file: String,
    /// How many stations the runs got.
    pub stations: usize,
    /// By zoom (`"10"` … `"19"`, then [`FRACTIONAL_ZOOMS`] as `"12.5"` and so on).
    pub zooms: BTreeMap<String, Pair>,
    /// By lane cap (`"1"`, `"2"`, `"3"`).
    pub caps: BTreeMap<String, Pair>,
    /// By edit step ([`EDIT_STEPS`]).
    pub edits: BTreeMap<String, Pair>,
}

/// The settings the digests were made with, in words for the reader (a change shows in the file's
/// diff and fails the comparison).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoldenSettings {
    /// [`LINE_WIDTH_PX`] and [`SPACING_PX`].
    pub widths: String,
    /// [`ZOOMS`].
    pub zooms: String,
    /// [`FRACTIONAL_ZOOMS`] (empty in a file made before them).
    #[serde(default)]
    pub fractional_zooms: String,
    /// [`CAPS`] and [`CAP_ZOOM`].
    pub caps: String,
    /// [`EDIT_STEPS`], [`EDIT_ZOOM`], [`EDIT_ZOOM_AFTER`] and [`MOVE_BY_DEG`].
    pub edits: String,
    /// [`PLACE_M_PER_DEG`].
    pub meters_per_degree: String,
}

impl GoldenSettings {
    /// The settings of this build of the harness.
    #[must_use]
    pub fn current() -> Self {
        Self {
            widths: format!("{LINE_WIDTH_PX} px lines, {SPACING_PX} px apart"),
            zooms: format!("{} to {}, each fresh", ZOOMS.start(), ZOOMS.end()),
            fractional_zooms: format!("{FRACTIONAL_ZOOMS:?}, each fresh"),
            caps: format!("{CAPS:?} at zoom {CAP_ZOOM}, each fresh"),
            edits: format!(
                "{} through one fresh cache, zoom {EDIT_ZOOM} then {EDIT_ZOOM_AFTER}, move by \
                 ({}, {}) degrees",
                EDIT_STEPS.join(", "),
                MOVE_BY_DEG.0,
                MOVE_BY_DEG.1
            ),
            meters_per_degree: PLACE_M_PER_DEG.to_string(),
        }
    }
}

/// A golden file: `fixtures/golden.json`, or the digests of other fixtures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldenFile {
    /// What the file is and how to update it.
    pub about: String,
    /// See [`GoldenSettings`].
    pub settings: GoldenSettings,
    /// By fixture name.
    pub fixtures: BTreeMap<String, Record>,
}

impl GoldenFile {
    /// A file of `fixtures` under the current settings; `about` says how to update it.
    #[must_use]
    pub fn new(about: &str, fixtures: BTreeMap<String, Record>) -> Self {
        Self {
            about: about.to_owned(),
            settings: GoldenSettings::current(),
            fixtures,
        }
    }

    /// Pretty JSON, keys sorted, ending with a newline.
    ///
    /// # Errors
    /// Never in practice (every value is a string or a number).
    pub fn to_json(&self) -> Result<String, String> {
        let mut text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        text.push('\n');
        Ok(text)
    }

    /// Reads a golden file.
    ///
    /// # Errors
    /// If the JSON is malformed or not a golden file.
    pub fn parse(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| format!("not a golden file: {e}"))
    }

    /// Every difference from `expected`, one line each (`fixture part.key what: expected X, got
    /// Y`); empty when the digests are the same. `about` is not compared.
    #[must_use]
    pub fn differences(&self, expected: &Self) -> Vec<String> {
        let mut out = Vec::new();
        if self.settings != expected.settings {
            out.push(format!(
                "settings: expected {:?}, got {:?}",
                expected.settings, self.settings
            ));
        }
        for name in expected.fixtures.keys() {
            if !self.fixtures.contains_key(name) {
                out.push(format!("{name}: in the file, not digested now"));
            }
        }
        for (name, got) in &self.fixtures {
            let Some(want) = expected.fixtures.get(name) else {
                out.push(format!("{name}: digested now, not in the file"));
                continue;
            };
            if got.file != want.file || got.stations != want.stations {
                out.push(format!(
                    "{name}: expected {} with {} stations, got {} with {}",
                    want.file, want.stations, got.file, got.stations
                ));
            }
            for (part, g, w) in [
                ("zooms", &got.zooms, &want.zooms),
                ("caps", &got.caps, &want.caps),
                ("edits", &got.edits, &want.edits),
            ] {
                let keys: std::collections::BTreeSet<&String> = g.keys().chain(w.keys()).collect();
                for key in keys {
                    match (w.get(key), g.get(key)) {
                        (Some(w), Some(g)) => {
                            for (what, wd, gd) in
                                [("lines", &w.lines, &g.lines), ("bars", &w.bars, &g.bars)]
                            {
                                if wd != gd {
                                    out.push(format!(
                                        "{name} {part}.{key} {what}: expected {wd}, got {gd}"
                                    ));
                                }
                            }
                        }
                        (Some(_), None) => out.push(format!("{name} {part}.{key}: missing now")),
                        (None, _) => out.push(format!("{name} {part}.{key}: not in the file")),
                    }
                }
            }
        }
        out
    }
}

/// Every digest of `fixtures` (as [`crate::load_dir`] gives them), by fixture name; each record
/// names its file relative to `root`, with `/` between folders.
///
/// # Errors
/// As [`record`], or when two fixtures have the same name.
pub fn record_all(
    root: &Path,
    fixtures: &[(PathBuf, Fixture)],
) -> Result<BTreeMap<String, Record>, String> {
    let mut records = BTreeMap::new();
    for (path, fixture) in fixtures {
        let file = path
            .strip_prefix(root)
            .unwrap_or(path)
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        if records
            .insert(fixture.name.clone(), record(fixture, &file)?)
            .is_some()
        {
            return Err(format!("two fixtures are named {}", fixture.name));
        }
    }
    Ok(records)
}

/// Every zoom digested with its key: [`ZOOMS`], then [`FRACTIONAL_ZOOMS`]. A key is the zoom as
/// both Rust and JavaScript write it (`"15"`, `"15.37"`).
#[must_use]
pub fn zooms() -> Vec<(String, f64)> {
    ZOOMS
        .map(f64::from)
        .chain(FRACTIONAL_ZOOMS)
        .map(|z| (z.to_string(), z))
        .collect()
}

/// `sideBySide()`'s settings at `zoom`.
fn at_zoom(zoom: f64) -> Settings {
    Settings {
        scale: Scale::Zoom(zoom),
        line_width_px: LINE_WIDTH_PX,
        spacing_px: SPACING_PX,
    }
}

/// Every digest of `fixture`; `file` is its path relative to the folder digested. The runs are
/// independent of each other (each has its own cache), so they run side by side on threads.
///
/// # Errors
/// If the pipeline refuses the fixture, or a determinism rule breaks (the message says which).
pub fn record(fixture: &Fixture, file: &str) -> Result<Record, String> {
    let input = &input(fixture);
    let name = fixture.name.as_str();
    let joined = |h: std::thread::ScopedJoinHandle<'_, Result<Pair, String>>| {
        h.join()
            .map_err(|_| format!("{name}: a run panicked"))
            .and_then(|r| r)
    };
    let every_zoom = zooms();
    std::thread::scope(|s| {
        let fresh: Vec<_> = every_zoom
            .iter()
            .map(|&(ref key, z)| (key, s.spawn(move || fresh_at(name, input, z))))
            .collect();
        let capped: Vec<_> = CAPS
            .iter()
            .map(|&c| (c, s.spawn(move || capped_at(name, input, c))))
            .collect();
        let edited = s.spawn(move || edits(name, input));
        let topology = s.spawn(move || incremental_topology(name, input));
        let reruns = reruns(name, input);

        let mut zooms = BTreeMap::new();
        for (key, h) in fresh {
            zooms.insert(key.clone(), joined(h)?);
        }
        if let Some(zoom) = reruns?
            .iter()
            .zip(&every_zoom)
            .find_map(|(pair, (key, _))| (zooms.get(key) != Some(pair)).then_some(key))
        {
            return Err(format!(
                "{name}: a zoom-only rerun at zoom {zoom} differs from a fresh run"
            ));
        }
        let mut caps = BTreeMap::new();
        for (cap, h) in capped {
            caps.insert(cap.to_string(), joined(h)?);
        }
        let edits = edited
            .join()
            .map_err(|_| format!("{name}: the edits panicked"))??;
        topology
            .join()
            .map_err(|_| format!("{name}: the topology check panicked"))??;
        Ok(Record {
            file: file.to_owned(),
            stations: input.stations.len(),
            zooms,
            caps,
            edits,
        })
    })
}

/// The ids of the routes `routes` picks, in that order.
fn ids_of<'a>(input: &'a Input, routes: &[usize]) -> Vec<&'a str> {
    routes.iter().map(|&i| input.ids[i].as_str()).collect()
}

/// Every route at `zoom`, from a fresh cache.
fn fresh_at(name: &str, input: &Input, zoom: f64) -> Result<Pair, String> {
    let drawn = pipeline::side_by_side_cached_with_ids(
        &mut Cache::new(),
        &input.keys,
        &input.lines,
        &input.stations,
        &input.groups,
        &at_zoom(zoom),
    )
    .map_err(|e| format!("{name} zoom {zoom}: {e}"))?;
    let all: Vec<usize> = (0..input.ids.len()).collect();
    Ok(digest(&ids_of(input, &all), &drawn, &[]))
}

/// Every zoom ([`zooms`]) in turn through one cache: the zoom-only reruns, which must run
/// geometry only.
fn reruns(name: &str, input: &Input) -> Result<Vec<Pair>, String> {
    let mut cache = Cache::new();
    let all: Vec<usize> = (0..input.ids.len()).collect();
    let mut out = Vec::new();
    for (_, zoom) in zooms() {
        let drawn = pipeline::side_by_side_cached_with_ids(
            &mut cache,
            &input.keys,
            &input.lines,
            &input.stations,
            &input.groups,
            &at_zoom(zoom),
        )
        .map_err(|e| format!("{name} zoom {zoom} rerun: {e}"))?;
        out.push(digest(&ids_of(input, &all), &drawn, &[]));
    }
    let runs = cache.runs();
    if runs.topology != 1 || runs.ordering != 1 {
        return Err(format!(
            "{name}: zoom changes ran more than geometry: {runs:?}"
        ));
    }
    Ok(out)
}

/// Every route with the lane cap `cap` at [`CAP_ZOOM`], from a fresh cache.
fn capped_at(name: &str, input: &Input, cap: usize) -> Result<Pair, String> {
    let capped = pipeline::side_by_side_capped(
        &mut Cache::new(),
        &input.keys,
        &input.lines,
        &input.stations,
        &input.groups,
        &at_zoom(f64::from(CAP_ZOOM)),
        cap,
    )
    .map_err(|e| format!("{name} cap {cap}: {e}"))?;
    let kept: Vec<usize> = (0..input.ids.len())
        .filter(|i| capped.dropped.binary_search(i).is_err())
        .collect();
    Ok(digest(
        &ids_of(input, &kept),
        &capped.drawn,
        &ids_of(input, &capped.dropped),
    ))
}

/// One edit step's input: its name, the routes it gives (as fixture indices, in order), their
/// routes and the zoom.
struct Step {
    name: &'static str,
    routes: Vec<usize>,
    lines: Vec<Vec<Point>>,
    zoom: u8,
}

/// The edit sequence's inputs ([`EDIT_STEPS`]).
fn edit_steps(input: &Input) -> Vec<Step> {
    let n = input.lines.len();
    let mut moved = input.lines.clone();
    if let Some(line) = moved.first_mut() {
        let k = line.len() / 2;
        if let Some(p) = line.get_mut(k) {
            *p = Point::new(p.x + MOVE_BY_DEG.0, p.y + MOVE_BY_DEG.1);
        }
    }
    let all: Vec<usize> = (0..n).collect();
    let reversed: Vec<usize> = (0..n).rev().collect();
    let step = |name: &'static str, source: &[Vec<Point>], routes: Vec<usize>, zoom: u8| Step {
        name,
        lines: routes.iter().map(|&i| source[i].clone()).collect(),
        routes,
        zoom,
    };
    let [full, move_, remove, add, reverse, zoom] = EDIT_STEPS;
    vec![
        step(full, &input.lines, all.clone(), EDIT_ZOOM),
        step(move_, &moved, all.clone(), EDIT_ZOOM),
        step(
            remove,
            &moved,
            (0..n.saturating_sub(1)).collect(),
            EDIT_ZOOM,
        ),
        step(add, &moved, all, EDIT_ZOOM),
        step(reverse, &moved, reversed.clone(), EDIT_ZOOM),
        step(zoom, &moved, reversed, EDIT_ZOOM_AFTER),
    ]
}

/// The edit sequence ([`EDIT_STEPS`]) through one fresh cache.
fn edits(name: &str, input: &Input) -> Result<BTreeMap<String, Pair>, String> {
    let mut cache = Cache::new();
    let mut out = BTreeMap::new();
    for step in edit_steps(input) {
        let keys: Vec<u32> = step.routes.iter().map(|&i| input.keys[i]).collect();
        let drawn = pipeline::side_by_side_cached_with_ids(
            &mut cache,
            &keys,
            &step.lines,
            &input.stations,
            &input.groups,
            &at_zoom(f64::from(step.zoom)),
        )
        .map_err(|e| format!("{name} edit {}: {e}", step.name))?;
        out.insert(
            step.name.to_owned(),
            digest(&ids_of(input, &step.routes), &drawn, &[]),
        );
    }
    Ok(out)
}

/// Along the edit sequence, the topology built incrementally (as the cache builds it) equals a
/// full build of the same routes at every step.
fn incremental_topology(name: &str, input: &Input) -> Result<(), String> {
    let params = Params::default();
    let mut state = topology::Incremental::new();
    for step in edit_steps(input) {
        if step.lines.is_empty() {
            continue;
        }
        let keys: Vec<u32> = step.routes.iter().map(|&i| input.keys[i]).collect();
        let proj = Projection::around(&step.lines);
        let meters: Vec<Vec<Point>> = step
            .lines
            .iter()
            .map(|l| l.iter().map(|&p| proj.forward(p)).collect())
            .collect();
        let incremental = topology::build_incremental_with_ids(&mut state, &keys, &meters, &params);
        if incremental != topology::build_with_traversals(&meters, &params) {
            return Err(format!(
                "{name} edit {}: the incremental topology differs from a full build",
                step.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fnv(bytes: &[u8]) -> String {
        let mut h = Fnv64::new();
        h.bytes(bytes);
        h.hex()
    }

    #[test]
    fn fnv1a_64_matches_the_published_test_vectors() {
        assert_eq!(fnv(b""), "cbf29ce484222325");
        assert_eq!(fnv(b"a"), "af63dc4c8601ec8c");
        assert_eq!(fnv(b"foobar"), "85944171f73967e8");
    }

    #[test]
    fn numbers_go_in_as_eight_little_endian_bytes() {
        let mut a = Fnv64::new();
        a.f64(-0.0);
        assert_eq!(a.hex(), fnv(&(-0.0_f64).to_bits().to_le_bytes()));
        let mut b = Fnv64::new();
        b.f64(0.0);
        assert_ne!(a, b, "-0 and 0 differ in their bits");
        let mut t = Fnv64::new();
        t.text("é");
        assert_eq!(t.hex(), fnv(&[2, 0, 0, 0, 0, 0, 0, 0, 0xc3, 0xa9]));
    }

    fn drawn(lines: Vec<Vec<Point>>, bars: Vec<Option<(Point, Point)>>) -> Drawn {
        Drawn { lines, bars }
    }

    #[test]
    fn every_bit_and_id_changes_the_digest() {
        let p = Point::new(1.0, 2.0);
        let base = drawn(
            vec![vec![p, Point::new(3.0, 4.0)]],
            vec![None, Some((p, p))],
        );
        let d = digest(&["A"], &base, &[]);

        let mut nudged = base.clone();
        nudged.lines[0][1].x = f64::from_bits(3.0_f64.to_bits() + 1);
        assert_ne!(digest(&["A"], &nudged, &[]).lines, d.lines);
        assert_ne!(digest(&["B"], &base, &[]).lines, d.lines);
        assert_ne!(digest(&["A"], &base, &["C"]).lines, d.lines);

        let mut moved_bar = base.clone();
        moved_bar.bars[1] = Some((p, Point::new(1.0, 2.5)));
        assert_eq!(digest(&["A"], &moved_bar, &[]).lines, d.lines);
        assert_ne!(digest(&["A"], &moved_bar, &[]).bars, d.bars);
        let mut no_bar = base;
        no_bar.bars[1] = None;
        assert_ne!(digest(&["A"], &no_bar, &[]).bars, d.bars);
    }

    #[test]
    fn zoom_keys_are_written_as_javascript_writes_them() {
        let keys: Vec<String> = zooms().into_iter().map(|(k, _)| k).collect();
        assert_eq!(
            keys,
            [
                "10", "11", "12", "13", "14", "15", "16", "17", "18", "19", "12.5", "15.37",
                "17.75"
            ]
        );
        for (key, z) in zooms() {
            assert_eq!(key.parse::<f64>(), Ok(z), "{key}");
        }
    }

    #[test]
    fn a_file_made_before_the_fractional_zooms_still_reads() {
        let text = GoldenFile::new("t", BTreeMap::new())
            .to_json()
            .unwrap()
            .replace(
                r#""fractionalZooms": "[12.5, 15.37, 17.75], each fresh","#,
                "",
            );
        let old = GoldenFile::parse(&text).unwrap();
        assert_eq!(old.settings.fractional_zooms, "");
        assert_eq!(
            old.differences(&GoldenFile::new("t", BTreeMap::new()))
                .len(),
            1,
            "only the settings differ"
        );
    }

    #[test]
    fn empty_output_has_a_digest_of_its_own() {
        let none = digest(&[], &drawn(Vec::new(), Vec::new()), &[]);
        let one_empty_line = digest(&["A"], &drawn(vec![Vec::new()], Vec::new()), &[]);
        assert_ne!(none, one_empty_line);
        assert_eq!(none.lines.len(), 16);
    }

    #[test]
    fn differences_name_each_changed_digest() {
        let pair = |l: &str| Pair {
            lines: l.to_owned(),
            bars: "b".to_owned(),
        };
        let record = |l: &str| Record {
            file: "x.geojson".to_owned(),
            stations: 1,
            zooms: BTreeMap::from([("10".to_owned(), pair(l))]),
            caps: BTreeMap::new(),
            edits: BTreeMap::new(),
        };
        let file = |l: &str| GoldenFile::new("t", BTreeMap::from([("x".to_owned(), record(l))]));
        assert!(file("a").differences(&file("a")).is_empty());
        assert_eq!(
            file("c").differences(&file("a")),
            ["x zooms.10 lines: expected a, got c"]
        );
        let empty = GoldenFile::new("t", BTreeMap::new());
        assert_eq!(
            empty.differences(&file("a")),
            ["x: in the file, not digested now"]
        );
        assert_eq!(
            file("a").differences(&empty),
            ["x: digested now, not in the file"]
        );
        let text = file("a").to_json().unwrap();
        assert_eq!(GoldenFile::parse(&text).unwrap(), file("a"));
        assert!(GoldenFile::parse("{}").is_err());
        assert!(GoldenFile::parse("").is_err());
    }
}
