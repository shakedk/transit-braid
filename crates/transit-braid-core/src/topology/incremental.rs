//! The incremental build: it keeps each route's samples and keys between calls
//! ([`Incremental`]) and, after an edit, computes again only the keys that can change, then
//! assembles the graph as a full build does, so the result is exactly the full build's.

use super::assemble::assemble;
use super::grid::{Grid, Reach, segments};
use super::keys::{Key, Prepared, key};
use super::matching::Matching;
use super::{Params, Topology, TopologyError, Traversal, check_input};
use crate::budget::{Bounds, Counter, Work};
use crate::geom::Point;
use crate::ids::{IdVec, LineId};

/// What [`build_incremental`] keeps between calls, so an edit is quick: the last routes, each
/// one's spikes removed and samples, and every sample's key. Keys are the expensive part of a
/// build (about 80% on a 50-line city), and an edit changes few of them.
#[derive(Debug, Clone, Default)]
pub struct Incremental {
    params: Option<Params>,
    inputs: Vec<Vec<Point>>,
    /// The last call's route ids ([`build_incremental_with_ids`]); empty when none were given.
    ids: Vec<u32>,
    prepared: IdVec<LineId, Prepared>,
    keys: IdVec<LineId, Vec<Key>>,
    /// How many segments each sample's key looked at, so an edit counts the nearby checks
    /// of the whole graph, as a full build does, not only of the keys it computes again.
    checks: IdVec<LineId, Vec<u32>>,
    last: KeyCounts,
}

/// How many sample keys the last [`build_incremental`] call computed, of all it used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyCounts {
    /// Keys computed in that call.
    pub computed: usize,
    /// Keys in the graph (one per sample).
    pub total: usize,
}

impl Incremental {
    /// Nothing kept: the first call builds everything.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many keys the last call computed, of all it used.
    #[must_use]
    pub fn last_keys(&self) -> KeyCounts {
        self.last
    }

    /// Keeps `pending` as the last call's state: the old routes it reuses move into it.
    pub(crate) fn keep(&mut self, pending: Pending) {
        let mut old_prepared: IdVec<LineId, Option<Prepared>> = std::mem::take(&mut self.prepared)
            .into_iter()
            .map(Some)
            .collect();
        let mut old_keys: IdVec<LineId, Option<Vec<Key>>> = std::mem::take(&mut self.keys)
            .into_iter()
            .map(Some)
            .collect();
        let Pending {
            params,
            inputs,
            ids,
            reused,
            own,
            keys,
            checks,
            last,
        } = pending;
        // Each route has its own preparation or reuses one (never both, never neither).
        let prepared = own
            .into_iter()
            .zip(&reused)
            .filter_map(|(own, &r)| {
                own.or_else(|| r.and_then(|o| old_prepared.get_mut(o).and_then(Option::take)))
            })
            .collect();
        let keys = keys
            .into_iter()
            .zip(&reused)
            .map(
                |(new, &r)| match r.and_then(|o| old_keys.get_mut(o).and_then(Option::take)) {
                    Some(old) => new
                        .into_iter()
                        .zip(old)
                        .map(|(n, o)| n.unwrap_or(o))
                        .collect(),
                    None => new.into_iter().flatten().collect(),
                },
            )
            .collect();
        *self = Self {
            params: Some(params),
            inputs,
            ids,
            prepared,
            keys,
            checks,
            last,
        };
    }
}

/// A build of [`build_incremental_within`] not yet kept in its [`Incremental`]: the caller
/// keeps it ([`Incremental::keep`]) only when its whole call succeeds, so a call refused later
/// (by the pipeline's station checks) leaves the state as it was.
#[derive(Debug)]
pub(crate) struct Pending {
    params: Params,
    inputs: Vec<Vec<Point>>,
    ids: Vec<u32>,
    /// Per route: the last call's route whose preparation and keys it reuses.
    reused: IdVec<LineId, Option<LineId>>,
    /// Per route: its own preparation, when it reuses none.
    own: IdVec<LineId, Option<Prepared>>,
    /// Per route and sample: a new key, or `None` to keep the reused route's.
    keys: IdVec<LineId, Vec<Option<Key>>>,
    checks: IdVec<LineId, Vec<u32>>,
    last: KeyCounts,
}

impl Pending {
    /// How many keys this build computed, of all it uses.
    pub(crate) fn keys(&self) -> KeyCounts {
        self.last
    }

    /// The nearby checks of the whole graph (what a full build counts).
    #[cfg(test)]
    pub(super) fn nearby_checks(&self) -> u64 {
        self.checks.iter().flatten().map(|&n| u64::from(n)).sum()
    }
}

/// [`build_with_traversals`](super::build_with_traversals) that reuses `state` from the last
/// call: the routes are compared with the last call's (a run of them added, removed or changed
/// between an unchanged start and end), and only keys that can differ are computed again: every
/// sample of an added or changed route, and a kept route's samples whose key named a
/// removed or changed route or that lie within reach of an added or changed one. The rest of
/// the build runs as usual, so the result is exactly the full build's. Builds everything when the
/// settings changed or more than half of the routes are new.
///
/// # Errors
/// Same as [`build`](super::build); a refused call leaves `state` as it was.
pub fn build_incremental(
    state: &mut Incremental,
    lines: &[Vec<Point>],
    params: &Params,
) -> Result<(Topology, IdVec<LineId, Traversal>), TopologyError> {
    build_incremental_with_ids(state, &[], lines, params)
}

/// [`build_incremental`] that matches routes with the last call's by `ids` (one per route,
/// all different): a route with the same id and the same points is
/// kept wherever it moved in the list, so reordering routes, or removing one and adding
/// another in one call, stays incremental. Ids that aren't one per route, or repeat (this call
/// or the last), match by position as [`build_incremental`] does. The result is the full build's
/// either way.
///
/// # Errors
/// Same as [`build`](super::build); a refused call leaves `state` as it was.
pub fn build_incremental_with_ids(
    state: &mut Incremental,
    ids: &[u32],
    lines: &[Vec<Point>],
    params: &Params,
) -> Result<(Topology, IdVec<LineId, Traversal>), TopologyError> {
    let (topology, traversals, pending) =
        build_incremental_within(state, ids, lines, params, &Bounds::default())?;
    state.keep(pending);
    Ok((topology, traversals))
}

/// [`build_incremental_with_ids`] within `bounds`, reading `state` without changing it: the
/// result comes with the new state, [`Pending`] until the caller keeps it. The nearby checks are
/// those of the whole graph, as a full build counts them, so the same routes are refused or
/// not whatever calls came before.
pub(crate) fn build_incremental_within(
    state: &Incremental,
    ids: &[u32],
    lines: &[Vec<Point>],
    params: &Params,
    bounds: &Bounds,
) -> Result<(Topology, IdVec<LineId, Traversal>, Pending), TopologyError> {
    check_input(lines, params)?;
    let m = Matching::new(&state.ids, &state.inputs, ids, lines);
    let fresh_count = m.old_of.iter().filter(|o| o.is_none()).count();
    let full = state.params != Some(*params) || 2 * fresh_count > lines.len();
    let is_fresh = |new: LineId| m.old_of.get(new).is_none_or(Option::is_none);
    let is_gone = |old: LineId| m.new_of.get(old).is_none_or(Option::is_none);
    // The last call's route each one reuses (none on a full build), and the routes that
    // reuse none, prepared anew.
    let reused: IdVec<LineId, Option<LineId>> = (0..lines.len())
        .map(|new| {
            m.old_of
                .get(LineId::new(new))
                .copied()
                .flatten()
                .filter(|&o| {
                    !full
                        && state.prepared.get(o).is_some()
                        && state.keys.get(o).is_some()
                        && state.checks.get(o).is_some()
                })
        })
        .collect();
    let own: IdVec<LineId, Option<Prepared>> = reused
        .iter()
        .zip(lines)
        .map(|(r, line)| r.is_none().then(|| Prepared::new(line, params)))
        .collect();

    let mut keys: IdVec<LineId, Vec<Option<Key>>> = IdVec::with_capacity(lines.len());
    let mut checks: IdVec<LineId, Vec<u32>> = IdVec::with_capacity(lines.len());
    let mut computed = 0;
    let (topology, traversals, total) = {
        let prepared: IdVec<LineId, &Prepared> = own
            .iter()
            .zip(&reused)
            .filter_map(|(own, r)| {
                own.as_ref()
                    .or_else(|| r.and_then(|o| state.prepared.get(o)))
            })
            .collect();
        let segs = segments(prepared.iter().map(|p| &p.line[..]));
        let grid = Grid::new(&segs, params.merge_m);
        let min_cos = crate::det::cos(params.angle_deg.to_radians()); // the same bits on every platform
        // A kept route's sample is keyed again where an added or changed route is near it
        // now, or a removed or changed one was: there its key or its count of nearby segments can
        // change. Read only for kept routes, so a full build needs neither (building them was
        // most of a first draw of long, crossing routes).
        let empty = || Reach::new(std::iter::empty(), params.merge_m);
        let (fresh_reach, gone_reach) = if full {
            (empty(), empty())
        } else {
            let gone = segments(
                state
                    .prepared
                    .iter_enumerated()
                    .filter(|&(o, _)| is_gone(o))
                    .map(|(_, p)| &p.line[..]),
            );
            (
                Reach::new(segs.iter().filter(|g| is_fresh(g.line)), params.merge_m),
                Reach::new(gone.iter(), params.merge_m),
            )
        };

        let mut nearby = Counter::new(Work::NearbyChecks, bounds.nearby_checks);
        for ((line, p), r) in prepared.iter_enumerated().zip(&reused) {
            let mut line_keys = Vec::with_capacity(p.samples.len());
            let mut line_checks = Vec::with_capacity(p.samples.len());
            let old = r.and_then(|o| Some((state.keys.get(o)?, state.checks.get(o)?)));
            let Some((old_keys, old_checks)) = old else {
                for s in &p.samples {
                    let (k, n) = key(line, s, &segs, &grid, params.merge_m, min_cos);
                    computed += 1;
                    nearby.add(u64::from(n))?;
                    line_keys.push(Some(k));
                    line_checks.push(n);
                }
                keys.push(line_keys);
                checks.push(line_checks);
                continue;
            };
            for ((s, (set, twice)), &n) in p.samples.iter().zip(old_keys).zip(old_checks) {
                if set.iter().any(|&o| is_gone(o))
                    || fresh_reach.near(s.at)
                    || gone_reach.near(s.at)
                {
                    let (k, n) = key(line, s, &segs, &grid, params.merge_m, min_cos);
                    computed += 1;
                    nearby.add(u64::from(n))?;
                    line_keys.push(Some(k));
                    line_checks.push(n);
                } else {
                    nearby.add(u64::from(n))?;
                    line_checks.push(n);
                    line_keys.push((!m.identity).then(|| {
                        let renumbered = set
                            .iter()
                            .filter_map(|&o| m.new_of.get(o).copied().flatten())
                            .collect();
                        (renumbered, *twice)
                    }));
                }
            }
            keys.push(line_keys);
            checks.push(line_checks);
        }

        // Each sample's key: its new one, or the reused route's.
        let key_refs: IdVec<LineId, Vec<&Key>> = keys
            .iter()
            .zip(&reused)
            .map(|(new, r)| {
                let old = r.and_then(|o| state.keys.get(o));
                new.iter()
                    .enumerate()
                    .filter_map(|(s, k)| k.as_ref().or_else(|| old.and_then(|v| v.get(s))))
                    .collect()
            })
            .collect();
        let total = key_refs.iter().map(Vec::len).sum();
        let (topology, traversals) = assemble(&prepared, &key_refs, params, bounds)?;
        (topology, traversals, total)
    };
    let pending = Pending {
        params: *params,
        inputs: lines.to_vec(),
        ids: ids.to_vec(),
        reused,
        own,
        keys,
        checks,
        last: KeyCounts { computed, total },
    };
    Ok((topology, traversals, pending))
}
