//! What the two edge-open recess families share (`edge_open_circular_recesses` and
//! `edge_open_prismatic_recesses`): the span tolerance both import from `quiddity._rings`, and
//! the face-graph readings each module keeps its own copy of in Python — a face's principal
//! plane, the mouth face capping a wall chain, the paired shared-edge occurrences between two
//! faces, and the floor proof by swept-face volume probes.
//!
//! `_rings`' ring walk (`rings`, closed prismatic rings for passages and pockets) is not here:
//! neither family calls it; they import only [`SPAN_EPS`].

use super::Context;
use super::planes::normalized;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{AXIS_ZERO_COS, Surface, V3};

/// Spans agree to this to count as one (`_rings.SPAN_EPS`): a coordinate comparison, not a size.
pub const SPAN_EPS: f64 = 1e-6;

/// The face's unit normal as `FaceGraph.normal` reads it (`face.normal_at()`): at the centre of
/// its parameter range, flipped on a reversed face. `None` for a face without one there.
pub fn normal(part: &Part, face: usize) -> Option<V3> {
    let (u, v) = match part.faces[face].surface {
        Surface::Plane { .. } => (0.0, 0.0),
        _ => {
            let (u0, u1, v0, v1) = part.uv_bounds(face)?;
            (u0 + 0.5 * (u1 - u0), v0 + 0.5 * (v1 - v0))
        }
    };
    let n = normalized(part.face_normal(face, u, v)?);
    n.iter().all(|c| c.is_finite()).then_some(n)
}

/// Whether the face is a native plane (`FaceGraph.is_planar`).
pub fn is_planar(part: &Part, face: usize) -> bool {
    matches!(part.faces[face].surface, Surface::Plane { .. })
}

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

/// The face's box as `(low, high)` along *axis* (`FaceGraph.bounds(node)[axis]`).
pub fn span(part: &Part, face: usize, axis: usize) -> (f64, f64) {
    let b = part.face_bounds(face);
    (b.min[axis], b.max[axis])
}

/// The neighbours every one of *faces* shares, in the first face's neighbour order.
pub fn common_neighbours(part: &Part, faces: &[usize]) -> Vec<usize> {
    let Some((&first, rest)) = faces.split_first() else {
        return Vec::new();
    };
    let others: Vec<Vec<usize>> = rest.iter().map(|&f| part.neighbours(f)).collect();
    part.neighbours(first)
        .into_iter()
        .filter(|n| others.iter().all(|o| o.contains(n)))
        .collect()
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

/// The edges two faces share whose uses pair up uniquely in opposite directions
/// (`FaceGraph.shared_occurrences`): an edge read a different number of times by the two
/// faces, or without one opposite partner per use, has no traversal-independent pairing and is
/// left out.
pub fn shared_occurrences(part: &Part, a: usize, b: usize) -> Vec<usize> {
    if a == b {
        return Vec::new();
    }
    let uses = |face: usize, edge: usize| -> Vec<bool> {
        part.faces[face]
            .loops
            .iter()
            .flat_map(|l| &l.edges)
            .filter(|(e, _)| *e == edge)
            .map(|(_, forward)| *forward)
            .collect()
    };
    let mut out = Vec::new();
    for edge in part.shared_edges(a, b) {
        let (left, right) = (uses(a, edge), uses(b, edge));
        if left.len() != right.len() {
            continue;
        }
        let unique = left
            .iter()
            .all(|l| right.iter().filter(|r| *r != l).count() == 1)
            && right
                .iter()
                .all(|r| left.iter().filter(|l| *l != r).count() == 1);
        if unique {
            // One occurrence per pair: as many as the edge has uses on either side.
            out.extend(std::iter::repeat_n(edge, left.len()));
        }
    }
    out
}

/// Whether the floor is a real floor (`_floor_proof` / `_exact_floor_proof`): the floor face
/// swept to the mouth holds no material of *solid*, and a thin slab behind it is all material.
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
