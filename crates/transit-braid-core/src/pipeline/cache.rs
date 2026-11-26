//! What the pipeline keeps between calls ([`Cache`]) and the cached entry points
//! ([`side_by_side_cached`], [`side_by_side_cached_with_ids`]): the same routes reuse their
//! graph and order, and an edit rebuilds them incrementally.

use super::error::from_topology;
use super::limits::{check_call, check_station_work};
use super::{Drawn, PipelineError, Projection, Settings};
use crate::budget::Bounds;
use crate::geom::Point;
use crate::geometry;
use crate::ids::{IdVec, LineId};
use crate::ordering::{self, ExhaustiveLimit, Weights};
use crate::topology::{self, Params};

/// How many times each stage of the pipeline ran in one [`Cache`]: a call with the same
/// routes as the cache's last call runs geometry only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StageRuns {
    /// Topology builds (routes → line graph and walks).
    pub topology: u64,
    /// Ordering solves.
    pub ordering: u64,
    /// Geometry runs (lines and bars at a zoom).
    pub geometry: u64,
}

/// What the pipeline keeps between calls, so a zoom change or an edit is quick: the last
/// routes with their projection, line graph, walks and order. These depend only on the
/// routes, so a call with the same routes (exactly, any zoom, widths or stations) reuses
/// them and runs geometry only. Other routes rebuild the topology incrementally
/// ([`topology::build_incremental`]): the same graph a fresh call builds, in less time when few
/// routes changed and the projection stayed the same. With a lane cap
/// ([`side_by_side_capped`](super::side_by_side_capped)) the cache also keeps the graph of every
/// route, which decides what the cap drops, and what the last capped call dropped, while the
/// stages above run on the kept routes.
#[derive(Debug, Clone, Default)]
pub struct Cache {
    pub(super) last: Option<Prepared>,
    runs: StageRuns,
    /// The topology's own state for edits.
    topology: topology::Incremental,
    /// The last ordering's (kept, solved) parts when it was warm-started.
    warm: Option<(usize, usize)>,
    /// The lane cap's graph of every route; `None` until a capped call.
    pub(super) cap: Option<Box<CapState>>,
    /// The work budget's bounds: the defaults, or smaller ones in tests.
    pub(super) bounds: Bounds,
}

/// What a lane cap keeps between calls: the graph of every route of the last capped call,
/// built incrementally like the main one, so a zoom change or an edit doesn't build it anew, and
/// what the last call with these routes dropped in the end, so a zoom change starts there.
#[derive(Debug, Clone, Default)]
pub(super) struct CapState {
    pub(super) topology: topology::Incremental,
    pub(super) lines: Vec<Vec<Point>>,
    pub(super) ids: Vec<u32>,
    pub(super) graph: topology::Topology,
    /// The cap of the last call on these routes that succeeded, and the routes it dropped
    /// after its last pass; `None` before one.
    pub(super) dropped: Option<(usize, Vec<usize>)>,
}

impl Cache {
    /// An empty cache: the first call runs every stage.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// An empty cache with other work bounds, for tests.
    #[cfg(test)]
    pub(crate) fn with_bounds(bounds: Bounds) -> Self {
        Self {
            bounds,
            ..Self::default()
        }
    }

    /// How many times each stage has run in this cache.
    #[must_use]
    pub fn runs(&self) -> StageRuns {
        self.runs
    }

    /// How many of the topology's sample keys its last run computed, of all it used: an
    /// edit computes only those near the routes it changed.
    #[must_use]
    pub fn topology_keys(&self) -> topology::KeyCounts {
        self.topology.last_keys()
    }

    /// When the order in use was warm-started from the order before an edit: how many
    /// parts kept their order untouched, and how many were solved again. `None` when it was
    /// solved from scratch.
    #[must_use]
    pub fn warm_parts(&self) -> Option<(usize, usize)> {
        self.warm
    }
}

/// Everything that depends only on the routes.
#[derive(Debug, Clone)]
pub(super) struct Prepared {
    pub(super) lines: Vec<Vec<Point>>,
    /// The ids the routes came with; empty when none were given.
    pub(super) ids: Vec<u32>,
    pub(super) proj: Projection,
    pub(super) meters: Vec<Vec<Point>>,
    pub(super) topo: topology::Topology,
    pub(super) walks: IdVec<LineId, topology::Traversal>,
    pub(super) order: ordering::Order,
}

/// [`side_by_side_with_named_stations`](super::side_by_side_with_named_stations) that keeps the
/// line graph and order in `cache`: when `lines` equal the cache's last routes exactly,
/// topology and ordering are reused and only geometry runs (a zoom change). The result is the
/// same as without the cache.
///
/// # Errors
/// As [`side_by_side_with_named_stations`](super::side_by_side_with_named_stations); a refused
/// call leaves the cache as it was.
pub fn side_by_side_cached(
    cache: &mut Cache,
    lines: &[Vec<Point>],
    stations: &[Point],
    groups: &[u32],
    settings: &Settings,
) -> Result<Drawn, PipelineError> {
    side_by_side_cached_with_ids(cache, &[], lines, stations, groups, settings)
}

/// [`side_by_side_cached`] where each route comes with an id: the
/// same number for the same route from call to call (`sideBySide()` gives each GeoJSON id one),
/// so an edit is matched by id rather than by place in the list. Reordering the routes, or
/// removing one and adding another in one call, stays incremental and warm-ordered. Ids that
/// aren't one per route, or repeat, are ignored (matched by place, as without them). The
/// drawing for the same routes is the same either way.
///
/// # Errors
/// As [`side_by_side_cached`].
pub fn side_by_side_cached_with_ids(
    cache: &mut Cache,
    ids: &[u32],
    lines: &[Vec<Point>],
    stations: &[Point],
    groups: &[u32],
    settings: &Settings,
) -> Result<Drawn, PipelineError> {
    check_call(lines, stations, groups, settings)?;
    if lines.is_empty() {
        return Ok(Drawn {
            lines: Vec::new(),
            bars: vec![None; stations.len()],
        });
    }

    // The same routes with the same ids (or no usable ids) reuse everything. Equal routes
    // with other ids are matched by id: two copies of one route can trade places.
    let reuse = cache.last.as_ref().is_some_and(|p| {
        p.lines == lines
            && (p.ids == ids
                || !topology::usable_ids(&p.ids, lines.len())
                || !topology::usable_ids(ids, lines.len()))
    });
    if reuse {
        if let Some(last) = cache.last.as_ref() {
            check_station_work(&last.topo, stations.len(), &cache.bounds)?;
        }
        if let Some(last) = cache.last.as_mut() {
            last.ids = ids.to_vec();
        }
    }
    if !reuse {
        let proj = Projection::around(lines);
        let meters = proj.forward_all(lines);
        // Kept in the cache only when the whole call succeeds.
        let (topo, walks, pending) = topology::build_incremental_within(
            &cache.topology,
            ids,
            &meters,
            &Params::default(),
            &cache.bounds,
        )
        .map_err(from_topology)?;
        // An edit (the topology was rebuilt incrementally) is ordered warm, from the
        // last order; anything else from scratch.
        let keys = pending.keys();
        let edit = cache
            .last
            .as_ref()
            .filter(|_| keys.computed < keys.total && cache.runs.topology > 0);
        let (order, warm) = match edit {
            Some(old) => {
                let edit = topology::route_edit_by_ids(&old.ids, &old.lines, ids, lines);
                let w = ordering::solve_warm(
                    &topo,
                    &walks,
                    &Weights::default(),
                    ExhaustiveLimit::default(),
                    &ordering::Previous {
                        topo: &old.topo,
                        order: &old.order,
                        edit: &edit,
                    },
                );
                (w.solution.order, Some((w.kept_parts, w.solved_parts)))
            }
            None => (
                ordering::solve(
                    &topo,
                    &walks,
                    &Weights::default(),
                    ExhaustiveLimit::default(),
                )
                .order,
                None,
            ),
        };
        check_station_work(&topo, stations.len(), &cache.bounds)?;
        cache.topology.keep(pending);
        cache.runs.topology += 1;
        cache.warm = warm;
        cache.runs.ordering += 1;
        cache.last = Some(Prepared {
            lines: lines.to_vec(),
            ids: ids.to_vec(),
            proj,
            meters,
            topo,
            walks,
            order,
        });
    }
    let Some(p) = cache.last.as_ref() else {
        // Set just above when it was missing; never a panic.
        return Err(PipelineError::InvalidSettings(
            "internal: no cached graph".to_owned(),
        ));
    };
    cache.runs.geometry += 1;

    let proj = p.proj;
    let m_per_px = proj.meters_per_pixel(settings.scale);
    let pitch_m = (settings.line_width_px + settings.spacing_px) * m_per_px;
    let back = |l: Vec<Point>| l.into_iter().map(|q| proj.inverse(q)).collect();
    let room_m = geometry::room_at(m_per_px);
    let out = geometry::lines_with_room(&p.topo, &p.walks, &p.order, pitch_m, room_m, &p.meters)
        .into_iter()
        .map(back)
        .collect();
    let stations_m: Vec<Point> = stations.iter().map(|&q| proj.forward(q)).collect();
    let stations_m =
        geometry::merge_named_stations_on_edges(&p.topo, &p.order, &stations_m, groups);
    let bars = geometry::station_bars(
        &p.topo,
        &p.order,
        pitch_m,
        settings.line_width_px * m_per_px,
        &stations_m,
    )
    .into_iter()
    .map(|b| b.map(|(a, c)| (proj.inverse(a), proj.inverse(c))))
    .collect();
    Ok(Drawn { lines: out, bars })
}
