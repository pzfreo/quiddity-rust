//! The convex-corner probe shared by the bevel and blend families (`quiddity._bevel`).

use std::collections::BTreeMap;

use super::Context;
use crate::kernel::classify::State;
use crate::kernel::geom::{INTERIOR_PROBE_FRAC, V3};

/// The point just off the virtual sharp corner a bevel replaces: the corner sits where the two
/// neighbour planes cross, at the bevel's own position along its edge; *toward* 1 nudges it to
/// the bevel's side, -1 to the far side (`_near_corner`).
pub fn near_corner(centre: V3, edge_axis: usize, planes: &BTreeMap<usize, f64>, toward: f64) -> V3 {
    let mut corner = [0.0; 3];
    corner[edge_axis] = centre[edge_axis];
    for (&axis, &coord) in planes {
        corner[axis] = coord;
    }
    let step = toward * INTERIOR_PROBE_FRAC;
    [0, 1, 2].map(|i| corner[i] + step * (centre[i] - corner[i]))
}

/// Does the virtual sharp corner the bevel replaces lie outside the solid (`convex_bevel`)?
/// Only a clean `IN` counts as material, as with `BRepClass3d`.
pub fn convex_bevel(
    ctx: &Context<'_>,
    centre: V3,
    edge_axis: usize,
    planes: &BTreeMap<usize, f64>,
) -> bool {
    ctx.classifier()
        .classify(near_corner(centre, edge_axis, planes, 1.0))
        != State::In
}
