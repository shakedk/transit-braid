//! Which routes of the last call an edit kept, matched by id or by place in the list:
//! [`Matching`] for the incremental build, [`RouteEdit`] for the warm-started ordering.

use crate::geom::Point;
use crate::ids::{IdVec, LineId};

/// Which routes of the last call are kept unchanged in this one, and where: by id when
/// both calls give usable ids ([`usable_ids`]), else by an unchanged start and end of the list.
pub(super) struct Matching {
    /// Per new route: its old line when it is kept with the same points.
    pub(super) old_of: IdVec<LineId, Option<LineId>>,
    /// Per old route: its new line when it is kept with the same points.
    pub(super) new_of: IdVec<LineId, Option<LineId>>,
    /// Same length, and every kept route keeps its index (no renumbering needed).
    pub(super) identity: bool,
}

impl Matching {
    pub(super) fn new(
        old_ids: &[u32],
        old: &[Vec<Point>],
        new_ids: &[u32],
        new: &[Vec<Point>],
    ) -> Self {
        let mut old_of: IdVec<LineId, Option<LineId>> = vec![None; new.len()].into();
        let mut new_of: IdVec<LineId, Option<LineId>> = vec![None; old.len()].into();
        if usable_ids(old_ids, old.len()) && usable_ids(new_ids, new.len()) {
            let by_id: std::collections::HashMap<u32, LineId> = old_ids
                .iter()
                .enumerate()
                .map(|(i, &id)| (id, LineId::new(i)))
                .collect();
            for (n, id) in new_ids.iter().enumerate() {
                let n = LineId::new(n);
                if let Some(&o) = by_id.get(id)
                    && old[o.index()] == new[n.index()]
                {
                    old_of[n] = Some(o);
                    new_of[o] = Some(n);
                }
            }
        } else {
            let (prefix, suffix) = unchanged_ends(old, new);
            for (n, slot) in old_of.iter_mut_enumerated() {
                let i = n.index();
                let o = if i < prefix {
                    Some(i)
                } else if i >= new.len() - suffix {
                    Some(i + old.len() - new.len())
                } else {
                    None
                }
                .map(LineId::new);
                *slot = o;
                if let Some(o) = o {
                    new_of[o] = Some(n);
                }
            }
        }
        // Every kept route keeps its index (changed ones are recomputed anyway).
        let identity = old.len() == new.len()
            && old_of
                .iter_enumerated()
                .all(|(n, o)| o.is_none_or(|o| o == n));
        Self {
            old_of,
            new_of,
            identity,
        }
    }
}

/// Ids a call can be matched by: one per route, all different. None given (empty) is
/// not usable, so matching falls back to positions.
#[must_use]
pub fn usable_ids(ids: &[u32], routes: usize) -> bool {
    if ids.len() != routes || ids.is_empty() {
        return false;
    }
    let mut seen = std::collections::HashSet::with_capacity(ids.len());
    ids.iter().all(|id| seen.insert(*id))
}

/// How many routes at the start and at the end of `new` are equal to `old`'s, the two never
/// overlapping: an edit changes, adds or removes the run between them.
fn unchanged_ends(old: &[Vec<Point>], new: &[Vec<Point>]) -> (usize, usize) {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = (0..room)
        .take_while(|&k| old[old.len() - 1 - k] == new[new.len() - 1 - k])
        .count();
    (prefix, suffix)
}

/// How the routes changed from one call to the next, as
/// [`build_incremental`](super::build_incremental) sees it: an unchanged start and end, and a run
/// between them that changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEdit {
    /// Each old route's line now, or `None` when it was removed. A changed run as long as
    /// before keeps its places (each route in it edited, still the same route).
    pub old_to_new: IdVec<LineId, Option<LineId>>,
    /// The new lines that are new or changed routes, in order.
    pub changed: Vec<LineId>,
}

/// How `old` became `new`, matching routes by `ids`: an old route maps to the new
/// one with its id (edited or not, still the same route), and the changed ones are those with a
/// new id or other points. Ids that aren't one per route, or repeat, fall back to
/// [`route_edit`] (by position).
#[must_use]
pub fn route_edit_by_ids(
    old_ids: &[u32],
    old: &[Vec<Point>],
    new_ids: &[u32],
    new: &[Vec<Point>],
) -> RouteEdit {
    if !(usable_ids(old_ids, old.len()) && usable_ids(new_ids, new.len())) {
        return route_edit(old, new);
    }
    let by_id: std::collections::HashMap<u32, LineId> = new_ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, LineId::new(i)))
        .collect();
    let old_to_new: IdVec<LineId, Option<LineId>> =
        old_ids.iter().map(|id| by_id.get(id).copied()).collect();
    let mut same: IdVec<LineId, bool> = vec![false; new.len()].into();
    for (o, n) in old_to_new.iter_enumerated() {
        if let Some(n) = *n
            && old[o.index()] == new[n.index()]
        {
            same[n] = true;
        }
    }
    RouteEdit {
        old_to_new,
        changed: same.ids().filter(|&n| !same[n]).collect(),
    }
}

/// How `old` became `new` ([`RouteEdit`]).
#[must_use]
pub fn route_edit(old: &[Vec<Point>], new: &[Vec<Point>]) -> RouteEdit {
    let (prefix, suffix) = unchanged_ends(old, new);
    let edited_in_place = old.len() == new.len();
    let old_to_new = (0..old.len())
        .map(|i| {
            if i < prefix || edited_in_place {
                Some(i)
            } else if i >= old.len() - suffix {
                Some(i + new.len() - old.len())
            } else {
                None
            }
            .map(LineId::new)
        })
        .collect();
    RouteEdit {
        old_to_new,
        changed: (prefix..new.len() - suffix).map(LineId::new).collect(),
    }
}
