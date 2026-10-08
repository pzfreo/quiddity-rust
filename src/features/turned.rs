//! What the turned-stock treatments share — a toroidal fillet and a conical chamfer are both
//! proved by the coaxial external cylinders and the faces they meet (`quiddity.fillets`,
//! `quiddity.chamfers`); countersinks read cone rims the same way. The turned-profile key
//! (`quiddity.turned.profile_key_from_bands`) under which steps and grooves publish their
//! body/profile join lives here too.

use std::cmp::Ordering;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::Context;
use super::body::BodyKey;
use super::cylinders::{CylinderEvidence, coaxial_axis_lines};
use super::policy::{self, AXIS_ALIGNED_COS};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Bounds, Curve, Surface, V3};
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
                policy::length_tol(c.diameter, COAXIAL_FRAC),
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

/// The body-local membership of one turned profile (`TurnedProfileKey`): the axis letter, the
/// commonest band axis point with its axial coordinate zeroed, the body's box and its key.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnedProfileKey {
    pub axis: char,
    pub axis_origin: V3,
    pub body_bounds: [f64; 6],
    pub body_key: Option<BodyKey>,
}

impl TurnedProfileKey {
    /// Python's `_order_key` comparison.
    pub fn order(&self, other: &Self) -> Ordering {
        self.axis
            .cmp(&other.axis)
            .then_with(|| py::tuple_order(&self.axis_origin, &other.axis_origin))
            .then_with(|| py::tuple_order(&self.body_bounds, &other.body_bounds))
            .then(self.body_key.is_some().cmp(&other.body_key.is_some()))
            .then_with(|| {
                py::tuple_order(
                    self.body_key.as_deref().unwrap_or(&[]),
                    other.body_key.as_deref().unwrap_or(&[]),
                )
            })
    }
}

/// The axis letter of an axis index.
pub fn axis_letter(axis: usize) -> char {
    ['x', 'y', 'z'][axis]
}

/// `profile_key_from_bands`: the profile one body's coaxial bands prove. *bounds* is the body's
/// box (the solid's, or the part's when it has none).
pub fn profile_key_from_bands(
    axis: usize,
    bands: &[&CylinderEvidence],
    bounds: &Bounds,
    body_key: Option<BodyKey>,
) -> TurnedProfileKey {
    // A Counter in first-seen order: the commonest origin wins, ties to the smallest.
    let mut origins: Vec<(V3, usize)> = Vec::new();
    for band in bands {
        let origin: V3 = std::array::from_fn(|i| {
            if i == axis {
                0.0
            } else {
                py::round_to(band.axis_point[i], 8)
            }
        });
        match origins.iter_mut().find(|(o, _)| *o == origin) {
            Some((_, n)) => *n += 1,
            None => origins.push((origin, 1)),
        }
    }
    let (origin, _) = origins
        .iter()
        .copied()
        .min_by(|a, b| b.1.cmp(&a.1).then_with(|| py::tuple_order(&a.0, &b.0)))
        .expect("a profile has bands");
    let r = |v: f64| py::round_to(v, 8);
    TurnedProfileKey {
        axis: axis_letter(axis),
        axis_origin: origin,
        body_bounds: [
            r(bounds.min[0]),
            r(bounds.max[0]),
            r(bounds.min[1]),
            r(bounds.max[1]),
            r(bounds.min[2]),
            r(bounds.max[2]),
        ],
        body_key,
    }
}
