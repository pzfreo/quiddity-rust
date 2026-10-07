//! Angled-step recognition (`quiddity.angled_steps`): an oblique planar face breaking a convex
//! corner of the part, larger than a chamfer, and closed at an end by a triangular flat.

use serde::Serialize;

use super::Context;
use super::bevel::{classify_bevel, convex_bevel, material_beyond_corner};
use super::evidence::{self, EvidenceError, Occurrence};
use super::planes::{axis_aligned_axis, nearest_axis_aligned_planes};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Curve, V3};
use crate::kernel::py;

/// Two edge directions this close to parallel are one side (`SMOOTH_ARC_GAP`).
const SMOOTH_ARC_GAP: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AngledStep {
    pub axis: char,
    pub leg1: f64,
    pub leg2: f64,
    pub angle: f64,
    pub length: f64,
    pub at: V3,
    pub corner: Option<V3>,
}

/// `recognise_angled_steps`.
pub fn recognise_angled_steps(part: &Part) -> Vec<AngledStep> {
    super::records(discover(&Context::new(part)))
}

/// The evidence path: each step with its slant face and the triangular flats closing it.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<AngledStep>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}

/// Corners of an outer loop made only of lines: direction changes between consecutive sides,
/// collinear runs counted once (`_effective_linear_sides`).
fn effective_linear_sides(part: &Part, face: usize) -> Option<usize> {
    let lp = &part.faces[face].loops[part.outer_loop(face)?];
    let mut directions = Vec::new();
    for &(e, forward) in &lp.edges {
        let edge = &part.edges[e];
        let Curve::Line { .. } = edge.curve else {
            return None;
        };
        let d = py::unit(geom::sub(edge.end, edge.start))?;
        directions.push(if forward { d } else { geom::scale(d, -1.0) });
    }
    if directions.is_empty() {
        return None;
    }
    let n = directions.len();
    Some(
        (0..n)
            .filter(|&i| 1.0 - geom::dot(directions[i], directions[(i + 1) % n]) > SMOOTH_ARC_GAP)
            .count(),
    )
}

/// The distinct edges of the face's outer loop (`outer_wire().edges()`).
fn outer_edges(part: &Part, face: usize) -> usize {
    let Some(outer) = part.outer_loop(face) else {
        return 0;
    };
    let mut edges: Vec<usize> = part.faces[face].loops[outer]
        .edges
        .iter()
        .map(|e| e.0)
        .collect();
    edges.sort_unstable();
    edges.dedup();
    edges.len()
}

/// The axis-aligned triangular flats among the face's neighbours (`_terminal_read`).
fn terminals(part: &Part, face: usize) -> Vec<usize> {
    part.neighbours(face)
        .into_iter()
        .filter(|&other| {
            axis_aligned_axis(part, other).is_some()
                && (part.face_edges(other).len() == 3 || {
                    let outer = outer_edges(part, other);
                    outer == 3 || (outer > 3 && effective_linear_sides(part, other) == Some(3))
                })
        })
        .collect()
}

pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<AngledStep>> {
    let part = ctx.part;
    let mut out = Vec::new();
    for face in 0..part.faces.len() {
        let Ok(bevel) = classify_bevel(part, face) else {
            continue;
        };
        let (centre, axis) = (bevel.bounds.centre(), bevel.edge_axis);
        let planes = nearest_axis_aligned_planes(part, face, centre, axis, false);
        if (0..3).any(|j| j != axis && !planes.contains_key(&j)) {
            continue;
        }
        if !convex_bevel(ctx, centre, axis, &planes) {
            continue; // concave: a pocket or passage wall, not a step
        }
        if material_beyond_corner(ctx, centre, axis, &planes) {
            continue; // the corner two recess walls meet at, not a corner of the part
        }
        let closing = terminals(part, face);
        if closing.is_empty() {
            continue; // runs edge to edge: a chamfer
        }
        let Some(at) = part.face_centre(face) else {
            continue;
        };
        let corner =
            [0, 1, 2].map(|i| py::round_to3(if i == axis { centre[i] } else { planes[&i] }));
        out.push(Occurrence {
            record: AngledStep {
                axis: ['x', 'y', 'z'][axis],
                leg1: py::round_to3(bevel.leg_hi),
                leg2: py::round_to3(bevel.leg_lo),
                angle: py::round_to(bevel.leg_lo.atan2(bevel.leg_hi).to_degrees(), 2),
                length: py::round_to3(bevel.bounds.max[axis] - bevel.bounds.min[axis]),
                at: at.map(py::round_to3),
                corner: Some(corner),
            },
            defining: vec![face],
            context: closing,
        });
    }
    out.sort_by(|a, b| {
        a.record
            .axis
            .cmp(&b.record.axis)
            .then_with(|| py::tuple_order(&a.record.at, &b.record.at))
    });
    out
}
