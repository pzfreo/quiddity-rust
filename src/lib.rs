//! Quiddity: deterministic, geometry-only feature recognition for STEP B-Rep — Rust port.
//!
//! Two layers: [`kernel`] is the geometry engine (STEP import, B-rep, exact geometry,
//! parameter-space trimming, point classification) and [`features`] holds the recognisers.

pub mod features;
pub mod kernel;

pub use features::angled_steps::{AngledStep, recognise_angled_steps};
pub use features::bosses::{BossRecord, recognise_bosses};
pub use features::chamfers::{Chamfer, ChamferOptions, recognise_chamfers};
pub use features::countersinks::{CounterSink, recognise_countersinks};
pub use features::fillets::{Fillet, FilletOptions, recognise_fillets};
pub use features::flats::{Flat, recognise_flats};
pub use features::hole_patterns::{HolePattern, recognise_hole_patterns};
pub use features::holes::{HoleOptions, HoleRecord, recognise_holes};
pub use features::paired_ramp_steps::{PairedRampStep, recognise_paired_ramp_steps};
pub use kernel::brep::Part;
pub use kernel::step::{read_step, read_step_file};
