//! Axis-aligned neighbour planes, the "what does this blend bridge" query shared by the bevel
//! and blend families (`quiddity._adjacency`).

use std::collections::BTreeMap;

use crate::kernel::brep::Part;
use crate::kernel::geom::{self, AXIS_ALIGNED_COS, Curve, Surface, V3, dominant_axis, norm};
use crate::kernel::py;

/// A planar face's plane as `validated_parameters(PLANE, …)` gives it: the unit normal with its
/// dominant component (ties to the later axis) made non-negative, and the plane's offset along
/// it. A B-spline or Bezier face counts when it is certified a plane (`_effective_surfaces`).
pub fn effective_plane(part: &Part, face: usize) -> Option<(V3, f64)> {
    let frame = match &part.faces[face].surface {
        Surface::Plane { frame } => *frame,
        Surface::Freeform {
            kind: "BSPLINE" | "BEZIER",
            ..
        } => match part.recovered(face) {
            Some(Surface::Plane { frame }) => *frame,
            _ => return None,
        },
        _ => return None,
    };
    let z = frame.z;
    // `max` keeps the first of ties, and the key ranks the later axis higher.
    let dominant = (0..3)
        .max_by(|&a, &b| py::order(z[a].abs(), z[b].abs()))
        .expect("three components");
    let sign = if z[dominant] >= 0.0 { 1.0 } else { -1.0 };
    let normal = z.map(|c| if c == 0.0 { 0.0 } else { sign * c });
    let offset = py::sum((0..3).map(|i| frame.origin[i] * normal[i]));
    let norm = py::sum(normal.iter().map(|c| c * c)).sqrt();
    ((norm - 1.0).abs() <= 1e-9 && offset.is_finite()).then_some((normal, offset))
}

/// The axis a planar face's normal aligns with and the plane's coordinate along it.
pub fn axis_aligned_axis(part: &Part, face: usize) -> Option<(usize, f64)> {
    let Surface::Plane { frame } = &part.faces[face].surface else {
        return None;
    };
    let comp = frame.z.map(f64::abs);
    if comp.iter().copied().fold(f64::NEG_INFINITY, f64::max) <= AXIS_ALIGNED_COS {
        return None;
    }
    let ax = dominant_axis(comp);
    Some((ax, frame.origin[ax]))
}

/// Per axis, the coordinate of *face*'s nearest axis-aligned neighbour plane
/// (`nearest_axis_aligned_planes`).
pub fn nearest_axis_aligned_planes(
    part: &Part,
    face: usize,
    centre: [f64; 3],
    exclude_axis: usize,
    refuse_equidistant: bool,
) -> BTreeMap<usize, f64> {
    let mut candidates: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for other in part.neighbours(face) {
        match axis_aligned_axis(part, other) {
            Some((ax, coord)) if ax != exclude_axis => {
                candidates.entry(ax).or_default().push(coord)
            }
            _ => {}
        }
    }
    let mut selected = BTreeMap::new();
    for (axis, coords) in candidates {
        let distance = |c: f64| (c - centre[axis]).abs();
        if !refuse_equidistant {
            let best = coords
                .iter()
                .copied()
                .min_by(|a, b| distance(*a).total_cmp(&distance(*b)).then(a.total_cmp(b)))
                .expect("non-empty");
            selected.insert(axis, best);
            continue;
        }
        let nearest = coords
            .iter()
            .map(|c| distance(*c))
            .fold(f64::INFINITY, f64::min);
        let tied: Vec<f64> = coords
            .iter()
            .copied()
            .filter(|c| {
                (distance(*c) - nearest).abs() <= geom::length_tol(distance(*c).max(nearest), 1e-9)
            })
            .collect();
        let far = tied
            .iter()
            .map(|c| distance(*c))
            .fold(f64::NEG_INFINITY, f64::max);
        let (lo, hi) = tied
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), c| {
                (l.min(*c), h.max(*c))
            });
        if hi - lo > geom::length_tol(far, 1e-9) {
            continue;
        }
        selected.insert(axis, 0.5 * (lo + hi));
    }
    selected
}

/// `gp_Vec::Normalized` divides each component (multiplying by the reciprocal can differ in
/// the last bit).
pub fn normalized(a: V3) -> V3 {
    let l = norm(a);
    a.map(|c| c / l)
}

/// `_coordinates`: each component rounded, without a negative zero.
pub fn coordinates(p: V3, digits: usize) -> V3 {
    p.map(|c| py::without_negative_zero(py::round_to(c, digits)))
}

/// A plane bounded by one loop of four straight edges (`_linear_quad`).
pub fn linear_quad(part: &Part, face: usize) -> bool {
    let f = &part.faces[face];
    let edges = part.face_edges(face);
    let mut vertices: Vec<usize> = edges
        .iter()
        .flat_map(|&e| [part.edges[e].vertices.0, part.edges[e].vertices.1])
        .collect();
    vertices.sort_unstable();
    vertices.dedup();
    matches!(f.surface, Surface::Plane { .. })
        && f.loops.len() == 1
        && part.outer_edges(face).len() == 4
        && part
            .outer_edges(face)
            .iter()
            .all(|&e| matches!(part.edges[e].curve, Curve::Line { .. }))
        && vertices.len() == 4
}

/// The planar plane's outward normal (`face.normal_at()`).
pub fn plane_normal(part: &Part, face: usize) -> Option<V3> {
    part.face_normal(face, 0.0, 0.0).map(normalized)
}
