//! Circular blind-step recognition (`quiddity.circular_blind_steps`): an inward quarter
//! cylinder cut from a stock corner along a principal axis, open at one end of the solid's
//! envelope and closed at the other by a perpendicular planar terminal. Two convex side joins
//! prove the corner opening, and the terminal swept to the opening must meet no material.

use std::f64::consts::FRAC_PI_2;

use serde::Serialize;

use super::Context;
use super::cylinders::CylinderEvidence;
use super::evidence::{self, EvidenceError, Occurrence, common_valid_solid};
use super::planes::axis_aligned_axis;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{COORD_FLOOR, SMOOTH_ARC_GAP};
use crate::kernel::py;

/// Radians of cylindrical parameter noise admitted around an exact quarter turn
/// (`QUARTER_TURN_RAD_TOL`).
const QUARTER_TURN_RAD_TOL: f64 = 1e-7;
const PRINCIPAL_AXIS_COS: f64 = 1.0 - SMOOTH_ARC_GAP;

type Point2 = [f64; 2];
type Point3 = [f64; 3];

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CircularBlindStep {
    pub axis: char,
    pub radius: f64,
    pub length: f64,
    /// Blind terminal, then the open end, on the cylinder's axis.
    pub centreline: [Point3; 2],
    /// Arc endpoint, cylinder centre, arc endpoint, in the two transverse coordinates.
    pub section: [Point2; 3],
}

impl CircularBlindStep {
    /// The fields after `axis`, in the order the Python dataclass orders them.
    fn sort_key(&self) -> Vec<f64> {
        let mut key = vec![self.radius, self.length];
        key.extend(self.centreline.iter().flatten());
        key.extend(self.section.iter().flatten());
        key
    }
}

/// `recognise_circular_blind_steps`.
pub fn recognise_circular_blind_steps(part: &Part) -> Vec<CircularBlindStep> {
    super::records(discover(&Context::new(part)))
}

/// The evidence path: each step with its cylindrical wall and terminal.
pub fn discover_verified(
    ctx: &Context<'_>,
) -> Result<Vec<Occurrence<CircularBlindStep>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}

/// `math.isclose(a, b, abs_tol=COORD_FLOOR)`.
fn close(a: f64, b: f64) -> bool {
    py::isclose(a, b, 1e-9, COORD_FLOOR)
}

/// The two distinct arc endpoints the wall and terminal share, in transverse coordinates,
/// ascending (`_shared_arc_endpoints`).
fn shared_arc_endpoints(
    part: &Part,
    cylinder: usize,
    terminal: usize,
    transverse: [usize; 2],
) -> Option<(Point2, Point2)> {
    let mut points: Vec<Point2> = Vec::new();
    for e in part.shared_edges(cylinder, terminal) {
        for p in part.edges[e].vertex_points() {
            let projected = [p[transverse[0]], p[transverse[1]]];
            if !points
                .iter()
                .any(|q| py::dist(&projected, q) <= COORD_FLOOR)
            {
                points.push(projected);
            }
        }
    }
    let [mut a, mut b] = points[..] else {
        return None;
    };
    if py::tuple_order(&b, &a).is_lt() {
        std::mem::swap(&mut a, &mut b);
    }
    Some((a, b))
}

/// The record for a quarter-cylinder wall closed by *terminal*, or `None` (`_candidate`).
fn candidate(
    ctx: &Context<'_>,
    cylinder: usize,
    terminal: usize,
    evidence: &CylinderEvidence,
) -> Option<CircularBlindStep> {
    let part = ctx.part;
    let axis = evidence.axis;
    if evidence.direction[axis].abs() < PRINCIPAL_AXIS_COS {
        return None;
    }
    let (plane_axis, terminal_at) = axis_aligned_axis(part, terminal)?;
    if plane_axis != axis || part.arc(cylinder, terminal) != Some(Arc::Concave) {
        return None;
    }
    if !py::isclose(evidence.u_extent, FRAC_PI_2, 0.0, QUARTER_TURN_RAD_TOL) || evidence.external {
        return None;
    }
    let solid = common_valid_solid(part, &[cylinder, terminal])?;
    let face = part.face_bounds(cylinder);
    let (low, high) = (face.min[axis], face.max[axis]);
    let body = part.solid_bounds(solid);
    let (direction, opening_at) = if close(terminal_at, low) && close(high, body.max[axis]) {
        (1.0, high)
    } else if close(terminal_at, high) && close(low, body.min[axis]) {
        (-1.0, low)
    } else {
        return None;
    };
    let length = high - low;

    let (mut axial, mut sides) = (Vec::new(), Vec::new());
    for neighbour in part.neighbours(cylinder) {
        if neighbour == terminal {
            continue;
        }
        let plane = axis_aligned_axis(part, neighbour)?;
        if part.arc(cylinder, neighbour) != Some(Arc::Convex) {
            return None;
        }
        if plane.0 == axis {
            axial.push(plane);
        } else {
            sides.push(plane);
        }
    }
    let transverse: [usize; 2] = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    let mut side_axes: Vec<usize> = sides.iter().map(|p| p.0).collect();
    side_axes.sort_unstable();
    side_axes.dedup();
    if axial.len() != 1 || sides.len() != 2 || side_axes != transverse {
        return None;
    }
    if !close(axial[0].1, opening_at) {
        return None;
    }
    let mut sweep = [0.0; 3];
    sweep[axis] = direction * length;
    if ctx.swept_face_volume(solid, terminal, [0.0; 3], sweep)? != 0.0 {
        return None;
    }

    let (first, second) = shared_arc_endpoints(part, cylinder, terminal, transverse)?;
    let anchor = evidence.axis_point;
    let centre = [anchor[transverse[0]], anchor[transverse[1]]];
    let (mut terminal_point, mut opening_point) = (anchor, anchor);
    terminal_point[axis] = terminal_at;
    opening_point[axis] = opening_at;
    let point2 = |p: Point2| p.map(py::quantise6);
    let point3 = |p: Point3| p.map(py::quantise6);
    Some(CircularBlindStep {
        axis: ['x', 'y', 'z'][axis],
        radius: py::quantise6(evidence.diameter / 2.0),
        length: py::quantise6(length),
        centreline: [point3(terminal_point), point3(opening_point)],
        section: [point2(first), point2(centre), point2(second)],
    })
}

pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<CircularBlindStep>> {
    let part = ctx.part;
    let mut cylinders: Vec<&CylinderEvidence> = ctx.cylinders().iter().collect();
    cylinders.sort_by_key(|c| c.face);
    let mut out = Vec::new();
    for evidence in cylinders {
        let cylinder = evidence.face;
        for terminal in part.neighbours(cylinder) {
            if let Some(record) = candidate(ctx, cylinder, terminal, evidence) {
                out.push(Occurrence {
                    record,
                    defining: vec![cylinder, terminal],
                    context: vec![],
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.record
            .axis
            .cmp(&b.record.axis)
            .then_with(|| py::tuple_order(&a.record.sort_key(), &b.record.sort_key()))
    });
    out
}
