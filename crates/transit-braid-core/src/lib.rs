//! Core algorithms for transit-braid: overlapping bus routes in, one side-by-side line per route
//! out ([`pipeline::side_by_side`]).
//!
//! The pipeline: [`topology`] finds which routes run together where (a line graph),
//! [`ordering`] picks each line's side on every street, and [`geometry`] draws every line in its
//! slot. Pure Rust with no WebAssembly or JavaScript dependencies, so everything here can be
//! unit-tested natively, and deterministic: native and WebAssembly builds give the same bits
//! ([`det`]).

// Tests compare `det`'s functions with the platform's and build inputs with platform math; the
// ban on it (clippy.toml) is for the shipped code, which the library build still checks.
#![cfg_attr(
    test,
    expect(
        clippy::disallowed_methods,
        reason = "tests use platform math as a reference and to build inputs"
    )
)]

pub mod budget;
pub mod det;
pub mod geom;
pub mod geometry;
pub mod ids;
pub mod ordering;
pub mod pipeline;
pub mod polyline;
pub mod topology;
