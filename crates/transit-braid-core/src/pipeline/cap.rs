//! The lane cap ([`side_by_side_capped`]): at most a given number of lines side by side on any
//! street; later routes that don't fit are dropped whole.

use super::cache::{Cache, CapState};
use super::error::from_topology;
use super::limits::check_call;
use super::{Drawn, PipelineError, Projection, Settings, side_by_side_cached_with_ids};
use crate::geom::Point;
use crate::ids::{EdgeId, IdVec, LineId};
use crate::topology::{self, Params};

/// What [`side_by_side_capped`] returns.
#[derive(Debug, Clone, PartialEq)]
pub struct Capped {
    /// The kept routes drawn on their own, exactly as [`side_by_side_cached_with_ids`] draws
    /// them without the dropped ones: their lines in input order, and one bar per station.
    pub drawn: Drawn,
    /// The routes the cap left out, as input indices, ascending.
    pub dropped: Vec<usize>,
}

/// [`side_by_side_cached_with_ids`] with a lane cap: at most
/// `max_side_by_side` lines side by side on any edge. The routes are taken in input order on
/// the graph of all of them, and one is kept only if every edge it runs along still has fewer
/// than `max_side_by_side` kept routes (a route that runs an edge twice counts once
/// there); the others are dropped whole. Then the whole pipeline runs on the kept routes
/// alone, so their order and corners are the ones they get without the dropped ones. If the kept
/// routes' own graph still has an edge over the cap (their graph is built anew), the same rule
/// runs again on it until none is. The graph doesn't depend on the zoom, so the same routes
/// are dropped at every zoom. A call with the cache's last capped routes, ids and cap (a zoom
/// change) starts from what that call dropped in the end, so it runs geometry only and draws
/// what a fresh call draws, however many passes the cap took. A cap at or above the number of
/// routes drops nothing and draws exactly what the uncapped call draws.
///
/// # Errors
/// As [`side_by_side_cached`](super::side_by_side_cached), plus
/// [`PipelineError::InvalidSettings`] for a cap of 0. A refused call leaves the cache as it was.
pub fn side_by_side_capped(
    cache: &mut Cache,
    ids: &[u32],
    lines: &[Vec<Point>],
    stations: &[Point],
    groups: &[u32],
    settings: &Settings,
    max_side_by_side: usize,
) -> Result<Capped, PipelineError> {
    if max_side_by_side == 0 {
        return Err(PipelineError::InvalidSettings(
            "max side by side must be 1 or more".to_owned(),
        ));
    }
    check_call(lines, stations, groups, settings)?;
    let capping = lines.len() > max_side_by_side;
    let mut dropped = if capping {
        let state = cap_state(cache, ids, lines)?;
        match &state.dropped {
            Some((cap, dropped)) if *cap == max_side_by_side => dropped.clone(),
            _ => over_cap(&state.graph, lines.len(), max_side_by_side),
        }
    } else {
        Vec::new()
    };
    loop {
        let kept: Vec<usize> = (0..lines.len())
            .filter(|i| dropped.binary_search(i).is_err())
            .collect();
        let kept_lines: Vec<Vec<Point>> = kept.iter().map(|&i| lines[i].clone()).collect();
        // Ids given one per route follow their routes; any others are ignored anyway.
        let kept_ids: Vec<u32> = if ids.len() == lines.len() {
            kept.iter().map(|&i| ids[i]).collect()
        } else {
            Vec::new()
        };
        let drawn = side_by_side_cached_with_ids(
            cache,
            &kept_ids,
            &kept_lines,
            stations,
            groups,
            settings,
        )?;
        let again = match cache.last.as_ref() {
            Some(p) if kept.len() > max_side_by_side => {
                over_cap(&p.topo, kept.len(), max_side_by_side)
            }
            _ => Vec::new(),
        };
        if again.is_empty() {
            if capping && let Some(state) = cache.cap.as_mut() {
                state.dropped = Some((max_side_by_side, dropped.clone()));
            }
            return Ok(Capped { drawn, dropped });
        }
        dropped.extend(again.iter().map(|&k| kept[k]));
        dropped.sort_unstable();
    }
}

/// The cap's state for `lines` and `ids`: the graph of every route, from the cache when the
/// routes and ids are the last capped call's (a zoom change), else built incrementally, which
/// also forgets what the last call dropped.
/// Kept even when the rest of the call is refused: it only decides what the cap drops, the same
/// routes whatever graph state it starts from.
fn cap_state<'a>(
    cache: &'a mut Cache,
    ids: &[u32],
    lines: &[Vec<Point>],
) -> Result<&'a CapState, PipelineError> {
    let bounds = cache.bounds;
    let state = cache.cap.get_or_insert_with(Box::default);
    if state.lines != lines || state.ids != ids {
        let proj = Projection::around(lines);
        let meters = proj.forward_all(lines);
        let (graph, _, pending) = topology::build_incremental_within(
            &state.topology,
            ids,
            &meters,
            &Params::default(),
            &bounds,
        )
        .map_err(from_topology)?;
        state.topology.keep(pending);
        state.lines = lines.to_vec();
        state.ids = ids.to_vec();
        state.graph = graph;
        state.dropped = None;
    }
    Ok(state)
}

/// The routes (of `n`) the lane cap drops on `topo`, ascending: in input order, a route
/// is kept when every edge it runs along has fewer than `max` kept routes so far.
fn over_cap(topo: &topology::Topology, n: usize, max: usize) -> Vec<usize> {
    let mut edges_of: IdVec<LineId, Vec<EdgeId>> = vec![Vec::new(); n].into();
    for (e, edge) in topo.edges.iter_enumerated() {
        for &(l, _) in &edge.lines {
            if let Some(mine) = edges_of.get_mut(l)
                && mine.last() != Some(&e)
            {
                mine.push(e);
            }
        }
    }
    let mut kept_on: IdVec<EdgeId, usize> = vec![0; topo.edges.len()].into();
    let mut dropped = Vec::new();
    for (l, mine) in edges_of.iter_enumerated() {
        if mine.iter().all(|&e| kept_on[e] < max) {
            for &e in mine {
                kept_on[e] += 1;
            }
        } else {
            dropped.push(l.index());
        }
    }
    dropped
}
