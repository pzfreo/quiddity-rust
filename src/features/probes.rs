//! The fixed interior probes the whole-body families (`thin_walls`, `interior_voids`) sample a
//! face at: five separated points of its parameter range, kept where they land on the face.

use crate::kernel::brep::Part;
use crate::kernel::geom::V3;

/// Fractions of the face's parameter ranges (`_UV_PROBES`).
const UV_PROBES: [(f64, f64); 5] = [
    (0.25, 0.25),
    (0.5, 0.5),
    (0.75, 0.75),
    (0.25, 0.75),
    (0.75, 0.25),
];

/// A point on a face with its parameters.
pub struct Sample {
    pub point: V3,
    pub uv: (f64, f64),
}

/// The probes that land inside the face (`_face_samples`; the first half of `_samples`).
pub fn probe_samples(part: &Part, face: usize) -> Vec<Sample> {
    let Some(domain) = part.domain(face) else {
        return Vec::new();
    };
    UV_PROBES
        .iter()
        .filter_map(|&(u, v)| part.position_at(face, u, v))
        .filter(|(_, (u, v))| domain.contains(*u, *v))
        .map(|(point, uv)| Sample { point, uv })
        .collect()
}
