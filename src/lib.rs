//! Quiddity: deterministic, geometry-only feature recognition for STEP B-Rep — Rust port.

pub mod adjacency;
pub mod brep;
pub mod classify;
pub mod cylinders;
pub mod fillets;
pub mod geom;
pub mod nurbs;
pub mod step;
pub mod trim;

pub use brep::Part;
pub use fillets::{Fillet, FilletOptions, recognise_fillets};
pub use step::{read_step, read_step_file};
