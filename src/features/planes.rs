//! Axis-aligned neighbour planes, the "what does this blend bridge" query shared by the bevel
//! and blend families (`quiddity._adjacency`).

use std::collections::BTreeMap;

use crate::kernel::brep::Part;
use crate::kernel::geom::{self, AXIS_ALIGNED_COS, Surface, dominant_axis};

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
