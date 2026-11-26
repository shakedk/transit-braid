//! The work budget: bounds on the work one call may do, so valid but adversarial input
//! (route points piled up in one place, routes crossing in thousands of short pieces, thousands of
//! stations along very detailed routes) is refused fast with a clear error instead of running for
//! minutes. Each bound counts steps, not time, so an input is refused the same way on every
//! machine, native or WebAssembly, and whatever calls came before. The bounds sit far above what
//! real cities need: the 50 busiest routes of Dublin, the largest of our cities, take 2.5 million
//! nearby checks, 6.7 million graph steps and 12.5 million station checks.

use std::fmt;

/// The most nearby-segment checks one topology build makes: summed over every sample, how many
/// segments the grid finds near it (`topology`'s keys). Points piled up in one place make many.
pub const MAX_NEARBY_CHECKS: u64 = 20_000_000;

/// The most graph steps one topology build takes: the steps whose number grows faster than the
/// input (smoothing flicker, comparing runs along one street, joining edge ends into nodes).
/// Routes that cross and overlap in many short pieces take many.
pub const MAX_GRAPH_STEPS: u64 = 50_000_000;

/// The most station checks one call makes: stations × the line graph's centerline segments, as
/// each station looks for its nearest edge.
pub const MAX_STATION_CHECKS: u64 = 50_000_000;

/// The kinds of work the budget bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Work {
    /// Nearby-segment checks ([`MAX_NEARBY_CHECKS`]).
    NearbyChecks,
    /// Graph steps ([`MAX_GRAPH_STEPS`]).
    GraphSteps,
    /// Station checks ([`MAX_STATION_CHECKS`]).
    StationChecks,
}

/// A bound a call ran into: which work, and its bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Exceeded {
    pub(crate) work: Work,
    pub(crate) max: u64,
}

/// The message for a call that needs more than `max` of `work`.
pub(crate) fn describe(f: &mut fmt::Formatter<'_>, work: Work, max: u64) -> fmt::Result {
    match work {
        Work::NearbyChecks => write!(
            f,
            "too complex: too many points lie close together (more than {max} nearby-segment \
             checks)"
        ),
        Work::GraphSteps => write!(
            f,
            "too complex: the routes cross and overlap in too many short pieces (more than \
             {max} graph steps)"
        ),
        Work::StationChecks => write!(
            f,
            "too complex: too many stations along such detailed routes (more than {max} \
             station checks, stations × centerline segments)"
        ),
    }
}

/// The bounds one call works within: the constants above, or smaller ones in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bounds {
    pub(crate) nearby_checks: u64,
    pub(crate) graph_steps: u64,
    pub(crate) station_checks: u64,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            nearby_checks: MAX_NEARBY_CHECKS,
            graph_steps: MAX_GRAPH_STEPS,
            station_checks: MAX_STATION_CHECKS,
        }
    }
}

/// Counts one kind of work against its bound.
#[derive(Debug, Clone)]
pub(crate) struct Counter {
    work: Work,
    used: u64,
    max: u64,
}

impl Counter {
    pub(crate) fn new(work: Work, max: u64) -> Self {
        Self { work, used: 0, max }
    }

    /// Adds `steps`; an error once the total passes the bound.
    pub(crate) fn add(&mut self, steps: u64) -> Result<(), Exceeded> {
        self.used = self.used.saturating_add(steps);
        if self.used > self.max {
            return Err(Exceeded {
                work: self.work,
                max: self.max,
            });
        }
        Ok(())
    }

    /// The steps counted so far.
    #[cfg(test)]
    pub(crate) fn used(&self) -> u64 {
        self.used
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_counter_refuses_only_past_its_bound() {
        let mut c = Counter::new(Work::GraphSteps, 10);
        assert_eq!(c.add(4), Ok(()));
        assert_eq!(c.add(6), Ok(()), "at the bound is fine");
        assert_eq!(
            c.add(1),
            Err(Exceeded {
                work: Work::GraphSteps,
                max: 10
            })
        );
        let mut big = Counter::new(Work::NearbyChecks, u64::MAX);
        assert_eq!(big.add(u64::MAX), Ok(()));
        assert_eq!(big.add(u64::MAX), Ok(()), "saturates, never wraps");
        assert_eq!(big.used(), u64::MAX);
        let mut none = Counter::new(Work::StationChecks, 0);
        assert_eq!(none.add(0), Ok(()));
        assert!(none.add(1).is_err());
    }

    #[test]
    fn the_default_bounds_are_the_approved_ones() {
        assert_eq!(
            Bounds::default(),
            Bounds {
                nearby_checks: 20_000_000,
                graph_steps: 50_000_000,
                station_checks: 50_000_000,
            }
        );
    }
}
