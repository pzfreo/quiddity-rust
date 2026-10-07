//! The geometry engine the recognisers stand on — what OpenCascade is to the Python
//! implementation: STEP import, the B-rep model, exact geometry, faces in parameter space, and
//! point-in-solid classification.

pub mod brep;
pub mod classify;
pub mod geom;
pub mod nurbs;
pub mod py;
pub mod sampling;
pub mod step;
pub mod uv;
