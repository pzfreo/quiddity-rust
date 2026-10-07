//! Quiddity: deterministic, geometry-only feature recognition for STEP B-Rep — Rust port.
//!
//! Two layers: [`kernel`] is the geometry engine (STEP import, B-rep, exact geometry,
//! parameter-space trimming, point classification) and [`features`] holds the recognisers.

pub mod features;
pub mod kernel;

pub use features::countersinks::{CounterSink, recognise_countersinks};
pub use features::fillets::{Fillet, FilletOptions, recognise_fillets};
pub use features::holes::{HoleOptions, HoleRecord, recognise_holes};
pub use kernel::brep::Part;
pub use kernel::step::{read_step, read_step_file};
