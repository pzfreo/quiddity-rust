//! What the turned-stock treatments share — a toroidal fillet and a conical chamfer are both
//! proved by the coaxial external cylinders and the faces they meet (`quiddity.fillets`,
//! `quiddity.chamfers`); countersinks read cone rims the same way.

use std::collections::BTreeMap;

use super::Context;
use super::cylinders::{CylinderEvidence, coaxial_axis_lines};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, AXIS_ALIGNED_COS, Curve, Surface, V3};
use crate::kernel::py;

/// Coaxial analytic axes may differ by modelling noise only (a fraction of the diameter).
const COAXIAL_FRAC: f64 = 1e-4;

/// The run's external cylinders by face.
pub fn external_cylinders<'c>(ctx: &'c Context<'_>) -> BTreeMap<usize, &'c CylinderEvidence> {
    ctx.cylinders()
        .iter()
        .filter(|c| c.external)
        .map(|c| (c.face, c))
        .collect()
}

/// The external cylinders among *neighbours* that revolve about the line (*origin*, *axis*).
pub fn coaxial_cylinders<'c>(
    neighbours: &[usize],
    external: &BTreeMap<usize, &'c CylinderEvidence>,
    origin: V3,
    axis: V3,
) -> Vec<&'c CylinderEvidence> {
    neighbours
        .iter()
        .filter_map(|n| external.get(n).copied())
        .filter(|c| {
            coaxial_axis_lines(
                origin,
                axis,
                c.axis_point,
                c.direction,
                geom::length_tol(c.diameter, COAXIAL_FRAC),
            )
        })
        .collect()
}

/// Whether the coaxial cylinders include two different bands (a shoulder between diameters).
pub fn bridges_two_bands(coaxial: &[&CylinderEvidence]) -> bool {
    coaxial.iter().any(|c| c.diameter != coaxial[0].diameter)
}

/// The planar faces among *neighbours* square to *axis* (a shoulder or end face).
pub fn transverse_planes(part: &Part, neighbours: &[usize], axis: V3) -> Vec<usize> {
    neighbours
        .iter()
        .copied()
        .filter(|&n| match part.faces[n].surface {
            Surface::Plane { frame } => geom::dot(frame.z, axis).abs() > AXIS_ALIGNED_COS,
            _ => false,
        })
        .collect()
}

/// A circular edge's radius and centre.
fn circle(part: &Part, edge: usize) -> Option<(f64, V3)> {
    match part.edges[edge].curve {
        Curve::Circle { frame, radius } => Some((radius, frame.origin)),
        _ => None,
    }
}

/// A circular rim: its radius and centre.
pub type Rim = (f64, V3);

/// `(minor, major, included angle°)` of a cone face: its smallest and largest circular rims and
/// the full cone angle to 2 dp (`cone_rims`).
pub fn cone_rims(part: &Part, face: usize) -> Option<(Rim, Rim, f64)> {
    let Surface::Cone { semi_angle, .. } = part.faces[face].surface else {
        return None;
    };
    let mut circles: Vec<(f64, V3)> = part
        .face_edges(face)
        .into_iter()
        .filter_map(|e| circle(part, e))
        .collect();
    if circles.len() < 2 {
        return None;
    }
    circles.sort_by(|a, b| py::order(a.0, b.0));
    let included = py::round_to(2.0 * semi_angle.to_degrees().abs(), 2);
    Some((circles[0], circles[circles.len() - 1], included))
}
