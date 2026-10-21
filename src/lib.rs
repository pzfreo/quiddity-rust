//! Quiddity: deterministic, geometry-only feature recognition for STEP B-Rep — Rust port.
//!
//! The recognisers ([`features`]) stand on the geometry kernel, [`haecceity`], re-exported as
//! [`kernel`]. [`pmi_json`] is the versioned JSON form of the kernel's semantic PMI model, the
//! `quiddity parts` / `quiddity pmi` process boundary.

include!("engine.rs");

#[cfg(test)]
extern crate self as quiddity;
#[cfg(test)]
#[path = "../tests/internal.rs"]
mod regression_tests;
