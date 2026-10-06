//! Face adjacency queries shared by the recognisers (`quiddity._adjacency`).

use std::collections::BTreeMap;

use crate::brep::Part;
use crate::geom::{self, AXIS_ALIGNED_COS, Surface};

/// Faces meeting along each edge, built once per run.
pub struct Adjacency {
    edge_faces: Vec<Vec<usize>>,
}

impl Adjacency {
    pub fn new(part: &Part) -> Self {
        Adjacency {
            edge_faces: part.edge_faces(),
        }
    }

    /// The distinct faces sharing an edge with *face*, in the face's own edge order.
    pub fn neighbours(&self, part: &Part, face: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for lp in &part.faces[face].loops {
            for &(e, _) in &lp.edges {
                for &other in &self.edge_faces[e] {
                    if other != face && !out.contains(&other) {
                        out.push(other);
                    }
                }
            }
        }
        out
    }
}

/// The index of the largest component, the first on a tie (Python's `max(range(3), key=...)`).
pub fn dominant_axis(v: [f64; 3]) -> usize {
    let mut best = 0;
    for i in 1..3 {
        if v[i] > v[best] {
            best = i;
        }
    }
    best
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
    adjacency: &Adjacency,
    face: usize,
    centre: [f64; 3],
    exclude_axis: usize,
    refuse_equidistant: bool,
) -> BTreeMap<usize, f64> {
    let mut candidates: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for other in adjacency.neighbours(part, face) {
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
                .min_by(|a, b| {
                    (distance(*a), *a)
                        .partial_cmp(&(distance(*b), *b))
                        .expect("finite")
                })
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

/// Does this face's surface frame already point out of the solid? `None` for surfaces with no
/// readable frame direction (`frame_points_outward`).
pub fn frame_points_outward(part: &Part, face: usize) -> Option<bool> {
    let f = &part.faces[face];
    let frame = match &f.surface {
        Surface::Plane { frame }
        | Surface::Cylinder { frame, .. }
        | Surface::Sphere { frame, .. } => frame,
        _ => return None,
    };
    Some(!f.reversed == frame.direct())
}
