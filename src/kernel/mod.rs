//! The geometry engine the recognisers stand on — what OpenCascade is to the Python
//! implementation: STEP import, the B-rep model, exact geometry, faces in parameter space, and
//! point-in-solid classification.

pub mod brep;
pub mod classify;
pub mod cloud;
pub mod geom;
pub mod mass;
pub mod nurbs;
pub mod poly;
pub mod py;
pub mod rays;
pub mod sampling;
pub mod step;
pub mod uv;
