//! Circular blind-step recognition (`quiddity.circular_blind_steps`): an inward quarter
//! cylinder cut from a stock corner along a principal axis, open at one end of the solid's
//! envelope and closed at the other by a perpendicular planar terminal. Two convex side joins
//! prove the corner opening, and the terminal swept to the opening must meet no material.

use std::f64::consts::FRAC_PI_2;

use serde::Serialize;

use super::Context;
use super::cylinders::CylinderEvidence;
use super::evidence::{self, EvidenceError, Occurrence, common_valid_solid};
use super::placed::{Grid, Placed};
use super::planes::{axis_aligned_axis, unit_plane_normal};
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, COORD_FLOOR, SMOOTH_ARC_GAP, Surface};
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
    pub centreline: [Placed<Point3>; 2],
    /// Arc endpoint, cylinder centre, arc endpoint, in the two transverse coordinates.
    pub section: [Placed<Point2>; 3],
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

internal_entry! {
/// `recognise_circular_blind_steps`.
pub fn recognise_circular_blind_steps(part: &Part) -> Vec<CircularBlindStep> {
    super::records(discover(&Context::new(part)))
}
}

internal_entry! {
/// The evidence path: each step with its cylindrical wall and terminal.
pub fn discover_verified(
    ctx: &Context<'_>,
) -> Result<Vec<Occurrence<CircularBlindStep>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}
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
    } else if close(terminal_at, low) && !close(terminal_at, high) {
        (1.0, high)
    } else if close(terminal_at, high) && !close(terminal_at, low) {
        (-1.0, low)
    } else {
        return None;
    };
    let length = high - low;
    let on_envelope = if direction > 0.0 {
        close(opening_at, body.max[axis])
    } else {
        close(opening_at, body.min[axis])
    };

    let (mut axial, mut sides) = (Vec::new(), Vec::new());
    let mut side_axes = Vec::new();
    for neighbour in part.neighbours(cylinder) {
        if neighbour == terminal {
            continue;
        }
        if part.arc(cylinder, neighbour) != Some(Arc::Convex) {
            return None;
        }
        if let Some(plane) = axis_aligned_axis(part, neighbour)
            && plane.0 == axis
        {
            axial.push((neighbour, plane));
        } else {
            // The section contains actual transverse points, so its two side planes may
            // roll about the principal cylinder without changing the record contract.
            let Surface::Plane { frame } = part.faces[neighbour].surface else {
                return None;
            };
            let normal = unit_plane_normal(part, neighbour)?;
            let aligned = axis_aligned_axis(part, neighbour);
            // Preserve the existing principal-plane admission; only newly admitted rolled
            // supports need the additional intrinsic perpendicularity and axis-incidence proof.
            if (aligned.is_none() || !on_envelope)
                && (geom::dot(normal, evidence.direction).abs() > SMOOTH_ARC_GAP
                    || geom::dot(geom::sub(evidence.axis_point, frame.origin), normal).abs()
                        > COORD_FLOOR)
            {
                return None;
            }
            sides.push(normal);
            side_axes.push(aligned.map(|p| p.0));
        }
    }
    let transverse: [usize; 2] = match axis {
        0 => [1, 2],
        1 => [0, 2],
        _ => [0, 1],
    };
    if axial.len() != 1 || sides.len() != 2 {
        return None;
    }
    let perpendicular = match side_axes.as_slice() {
        [Some(a), Some(b)] => a != b,
        _ => geom::dot(sides[0], sides[1]).abs() <= SMOOTH_ARC_GAP,
    };
    if !perpendicular {
        return None;
    }
    if !close(axial[0].1.1, opening_at) {
        return None;
    }
    // A convex shoulder can open the cut inside the overall stock envelope. Its plane
    // must face out along the run; the side-plane incidence checks above establish the
    // same corner as the cylinder, and the complete terminal sweep below proves air.
    if !on_envelope {
        for face in [axial[0].0, terminal] {
            let normal = unit_plane_normal(part, face)?;
            if normal[axis] * direction < PRINCIPAL_AXIS_COS
                || (0..3).any(|i| i != axis && normal[i].abs() > SMOOTH_ARC_GAP)
            {
                return None;
            }
        }
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
    let point2 = |p: Point2| Placed::new(p, Grid::Figures(6));
    let point3 = |p: Point3| Placed::new(p, Grid::Figures(6));
    Some(CircularBlindStep {
        axis: ['x', 'y', 'z'][axis],
        radius: py::quantise6(evidence.diameter / 2.0),
        length: py::quantise6(length),
        centreline: [point3(terminal_point), point3(opening_point)],
        section: [point2(first), point2(centre), point2(second)],
    })
}

internal_entry! {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::support_patches::ruled_prism;

    #[test]
    fn a_remote_stock_extension_keeps_a_local_corner_opening() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/golden_circular_blind_step.step");
        let input = crate::read_step_file(&path).unwrap();
        let frame = crate::frames::infer_part_frame(&input).unwrap();
        let stock = input.transformed(&frame.normalisation()).unwrap();
        let expected = discover(&Context::new(&stock));
        assert_eq!(expected.len(), 1);
        let b = stock.bounds();
        // Attach a small rectangular boss to the opening plane, away from the circular
        // corner. The opening is unchanged, but is no longer the solid's axial extremum.
        let low = [
            [b.min[0] + 2.0, b.min[1] + 2.0, b.min[2]],
            [b.min[0] + 6.0, b.min[1] + 2.0, b.min[2]],
            [b.min[0] + 6.0, b.min[1] + 6.0, b.min[2]],
            [b.min[0] + 2.0, b.min[1] + 6.0, b.min[2]],
        ];
        let high = low.map(|p| [p[0], p[1], p[2] - 5.0]);
        let boss = ruled_prism(&low, &high).unwrap();
        let mut faces = stock.faces.clone();
        let mut edges = stock.edges.clone();
        let edge_offset = edges.len();
        let vertex_offset = edges
            .iter()
            .flat_map(|e| [e.vertices.0, e.vertices.1])
            .max()
            .unwrap()
            + 1;
        for mut edge in boss.edges.clone() {
            edge.vertices = (
                edge.vertices.0 + vertex_offset,
                edge.vertices.1 + vertex_offset,
            );
            edges.push(edge);
        }
        let owner = (0..stock.faces.len())
            .find(|&f| {
                stock
                    .face_normal(f, 0.0, 0.0)
                    .is_some_and(|n| n[2] < -PRINCIPAL_AXIS_COS)
                    && close(stock.face_bounds(f).min[2], b.min[2])
            })
            .unwrap();
        for (index, mut face) in boss.faces.into_iter().enumerate() {
            for lp in &mut face.loops {
                for (edge, _) in &mut lp.edges {
                    *edge += edge_offset;
                }
            }
            if index == 0 {
                faces[owner].loops.push(face.loops.remove(0));
            } else {
                faces.push(face);
            }
        }
        let solid = crate::kernel::brep::Solid {
            faces: (0..faces.len()).collect(),
        };
        let extended = Part::new(faces, edges, vec![solid]);
        assert!(
            extended.solid_is_geometrically_valid(0),
            "bad faces {:?}",
            extended.bad_faces(0)
        );
        let found = discover(&Context::new(&extended));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].defining, expected[0].defining);
        assert_eq!(found[0].record, expected[0].record);
    }
}
