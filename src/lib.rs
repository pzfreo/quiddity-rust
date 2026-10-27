//! Quiddity: deterministic, geometry-only manufacturing feature recognition for STEP B-rep
//! models. See the README for the command line and the recognition document.
//!
//! The recognisers ([`features`]) stand on the geometry kernel, [`haecceity`], re-exported as
//! [`kernel`]. [`pmi_json`] is the versioned JSON form of the kernel's semantic PMI model, the
//! `quiddity parts` / `quiddity pmi` process boundary.

mod engine;
pub use engine::*;

#[cfg(test)]
extern crate self as quiddity;
#[cfg(test)]
#[path = "../tests/internal/mod.rs"]
mod regression_tests;
