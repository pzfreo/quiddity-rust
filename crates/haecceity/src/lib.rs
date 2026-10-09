//! Haecceity: an exact B-rep geometry kernel in pure Rust — what OpenCascade is to the Python
//! implementations of quiddity and draftwright. STEP import, the B-rep model, exact geometry,
//! faces in parameter space, rays and point classification, shared volumes and face coverage,
//! and analytic surfaces recovered from freeform ones.
//!
//! *Haecceity* is "thisness": the particular thing itself, as *quiddity* is "whatness". The
//! kernel holds the exact geometry of this part; quiddity says what kind of features it has.

pub mod anchor;
pub mod brep;
pub mod classify;
pub mod cloud;
pub mod cover;
pub mod express;
pub mod geom;
pub mod hlr;
pub mod mass;
pub mod mesh;
pub mod nurbs;
pub mod overlap;
pub mod p21;
pub mod pmi;
pub mod poly;
pub mod py;
pub mod rays;
pub mod recover;
pub mod sampling;
pub mod step;
pub mod sweep;
pub mod uv;
pub mod volume;

pub use brep::Part;
pub use step::{read_step, read_step_file};
pub mod express_rules;
pub mod removal;
