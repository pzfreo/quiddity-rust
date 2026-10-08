//! What the two edge-open recess families share (`edge_open_circular_recesses` and
//! `edge_open_prismatic_recesses`): the span tolerance both import from `quiddity._rings`, and
//! the readings each module keeps its own copy of in Python — a face's principal plane, the
//! mouth face capping a wall chain, and the floor proof by swept-face volume probes. The
//! `FaceGraph` readings they make are in [`super::graph`].
//!
//! `_rings`' ring walk (`rings`, closed prismatic rings for passages and pockets) is not here:
//! neither family calls it; they import only [`SPAN_EPS`].

use super::Context;
use super::graph::{common_neighbours, normal};
use super::policy::AXIS_ZERO_COS;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::V3;

/// Spans agree to this to count as one (`_rings.SPAN_EPS`): a coordinate comparison, not a size.
pub const SPAN_EPS: f64 = 1e-6;

/// The principal axis a flat face lies across and its coordinate there (`_principal_plane`):
/// its normal has exactly one component of at least `1 - AXIS_ZERO_COS`, and its box is no
/// thicker than [`SPAN_EPS`] along it. Asked of any face, as Python asks it of mouth candidates.
pub fn principal_plane(part: &Part, face: usize) -> Option<(usize, f64)> {
    let n = normal(part, face)?;
    let axes: Vec<usize> = (0..3)
        .filter(|&a| n[a].abs() >= 1.0 - AXIS_ZERO_COS)
        .collect();
    let &[axis] = axes.as_slice() else {
        return None;
    };
    let b = part.face_bounds(face);
    let (low, high) = (b.min[axis], b.max[axis]);
    (high - low <= SPAN_EPS).then_some((axis, (low + high) / 2.0))
}

/// The faces capping a wall chain's far end: among the faces every wall meets, those lying
/// across *axis* at *at* that turn convex or smooth to every wall.
pub fn mouths(part: &Part, walls: &[usize], axis: usize, at: f64) -> Vec<usize> {
    common_neighbours(part, walls)
        .into_iter()
        .filter(|&node| {
            principal_plane(part, node)
                .is_some_and(|(a, coord)| a == axis && (coord - at).abs() <= SPAN_EPS)
                && walls
                    .iter()
                    .all(|&w| matches!(part.arc(node, w), Some(Arc::Convex) | Some(Arc::Smooth)))
        })
        .collect()
}

/// Whether the floor is a real floor (`_floor_proof` / `_exact_floor_proof`): the floor face
/// swept to the mouth holds no material of *solid*, and a thin slab behind it is all material.
///
/// The sweep is exact for floors bounded by lines and by circles and arcs about the run axis,
/// holes through the floor included. A floor edge of any other curve (an ellipse where a hole
/// meets the floor obliquely, a B-spline) cannot be swept, the floor is not proved and the
/// recess is declined, where Python's `Solid.extrude` would sweep it: a port limitation
/// (rust-wrong), reached by no captured call or corpus part.
pub fn floor_proof(
    ctx: &Context<'_>,
    solid: usize,
    floor: usize,
    axis: usize,
    floor_at: f64,
    mouth_at: f64,
) -> bool {
    let distance = mouth_at - floor_at;
    let thickness = 2e-5f64.max(distance.abs() * 1e-4);
    let (mut toward, mut behind) = ([0.0; 3], [0.0; 3]);
    toward[axis] = distance;
    behind[axis] = -thickness.copysign(distance);
    let fraction = |sweep: V3| ctx.swept_face_fraction(solid, floor, [0.0; 3], sweep);
    fraction(toward).is_some_and(|f| f <= 1e-9) && fraction(behind).is_some_and(|f| f >= 1.0 - 1e-9)
}

/// `pairwise` distance in the section plane (`math.dist`).
pub fn dist2(a: [f64; 2], b: [f64; 2]) -> f64 {
    crate::kernel::py::dist(&a, &b)
}
