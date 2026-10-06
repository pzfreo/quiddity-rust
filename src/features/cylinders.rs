//! The cylindrical-face inventory (`quiddity._cylinder_substrate.analyse_cylinders`), native
//! analytic cylinders only.

use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, Surface, V3, dominant_axis_preferring_z};

#[derive(Clone, Debug, PartialEq)]
pub struct CylinderEvidence {
    pub face: usize,
    pub solid: usize,
    pub diameter: f64,
    /// Dominant axis letter index (0 = x, 1 = y, 2 = z).
    pub axis: usize,
    /// The face's angular span in radians.
    pub u_extent: f64,
    pub axis_point: V3,
    /// Unit axis direction with its dominant component positive.
    pub direction: V3,
    pub s_lo: f64,
    pub s_hi: f64,
    pub external: bool,
}

fn canonical(value: f64, floor: f64) -> f64 {
    if value.abs() <= floor {
        0.0
    } else {
        format!("{value:.11e}").parse().expect("float")
    }
}

/// Every native cylinder face, grouped by owning solid (or all faces when there are none).
pub fn analyse_cylinders(part: &Part) -> Vec<CylinderEvidence> {
    let faces: Vec<(usize, usize)> = if part.solids.is_empty() {
        (0..part.faces.len()).map(|f| (0, f)).collect()
    } else {
        part.solids
            .iter()
            .enumerate()
            .flat_map(|(s, solid)| solid.faces.iter().map(move |&f| (s, f)))
            .collect()
    };
    let mut out = Vec::new();
    for (solid, face) in faces {
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            continue;
        };
        let Some((u0, u1, v0, v1)) = part.uv_bounds(face) else {
            continue;
        };
        let axis = dominant_axis_preferring_z(frame.z);
        let sign = if frame.z[axis] > 0.0 { 1.0 } else { -1.0 };
        let direction = geom::scale(frame.z, sign);
        let s_ap: f64 = (0..3).map(|i| frame.origin[i] * direction[i]).sum();
        let axial = (s_ap + sign * v0, s_ap + sign * v1);
        let axial = (
            canonical(axial.0, COORD_FLOOR),
            canonical(axial.1, COORD_FLOOR),
        );
        out.push(CylinderEvidence {
            face,
            solid,
            diameter: geom::quantise(radius * 2.0),
            axis,
            u_extent: u1 - u0,
            axis_point: frame.origin.map(|c| canonical(c, COORD_FLOOR)),
            direction: direction.map(|c| canonical(c, 1e-12)),
            s_lo: axial.0.min(axial.1),
            s_hi: axial.0.max(axial.1),
            external: part.frame_points_outward(face).unwrap_or(false),
        });
    }
    out
}

/// Whether two unit-direction axis lines are parallel and within *tol* (`_coaxial_axis_lines`).
pub fn coaxial_axis_lines(pa: V3, da: V3, pb: V3, db: V3, tol: f64) -> bool {
    if geom::dot(da, db).abs() < 1.0 - 1e-6 {
        return false;
    }
    let offset = geom::sub(pa, pb);
    let along = geom::dot(offset, db);
    let d2: f64 = (0..3).map(|i| (offset[i] - along * db[i]).powi(2)).sum();
    d2 <= tol * tol
}
