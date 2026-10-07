//! Chamfer recognition (`quiddity.chamfers`).
//!
//! A prismatic chamfer is a small oblique planar face bevelling a convex edge between two
//! axis-aligned walls; a turned chamfer is a conical band meeting a coaxial external cylinder.

use std::collections::BTreeMap;

use serde::Serialize;

use super::Context;
use super::bevel::{classify_bevel, convex_bevel};
use super::countersinks::cone_rims;
use super::cylinders::{CylinderEvidence, coaxial_axis_lines};
use super::evidence::Occurrence;
use super::planes::nearest_axis_aligned_planes;
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, AXIS_ALIGNED_COS, COORD_FLOOR, Surface, V3, dominant_axis};

/// Coaxial analytic axes may differ by modelling noise only (a fraction of the diameter).
const COAXIAL_FRAC: f64 = 1e-4;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Chamfer {
    pub axis: char,
    pub leg1: f64,
    pub leg2: f64,
    pub angle: f64,
    pub at: V3,
    pub turned: bool,
    /// The virtual sharp corner a planar chamfer replaces; `None` for a turned chamfer.
    pub corner: Option<V3>,
}

#[derive(Clone, Copy, Debug)]
pub struct ChamferOptions {
    /// The minimum leg (an edge break below it is not reported); 0 by default.
    pub tol: Option<f64>,
    pub max_leg_frac: f64,
    /// `false` for a part already classified as rotational: only conical chamfers are reported.
    pub include_planar: bool,
}

impl Default for ChamferOptions {
    fn default() -> Self {
        ChamferOptions {
            tol: None,
            max_leg_frac: 0.45,
            include_planar: true,
        }
    }
}

/// `recognise_chamfers`.
pub fn recognise_chamfers(part: &Part, opts: &ChamferOptions) -> Vec<Chamfer> {
    discover(&Context::new(part), opts)
        .into_iter()
        .map(|o| o.record)
        .collect()
}

/// Whether a measured leg is materially below the evidence floor (`_below_minimum_evidence`).
fn below_minimum(value: f64, minimum: f64) -> bool {
    value < minimum && (value - minimum).abs() > COORD_FLOOR
}

fn chamfer(
    axis: usize,
    leg_hi: f64,
    leg_lo: f64,
    at: V3,
    turned: bool,
    corner: Option<V3>,
) -> Chamfer {
    Chamfer {
        axis: ['x', 'y', 'z'][axis],
        leg1: geom::round3(leg_hi),
        leg2: geom::round3(leg_lo),
        angle: geom::round_to(leg_lo.atan2(leg_hi).to_degrees(), 2),
        at: at.map(geom::round3),
        turned,
        corner,
    }
}

pub fn discover(ctx: &Context<'_>, opts: &ChamferOptions) -> Vec<Occurrence<Chamfer>> {
    let part = ctx.part;
    let tol = opts.tol.unwrap_or(0.0);
    let max_leg = opts.max_leg_frac * ctx.bounds().max_extent();
    let mut out = Vec::new();
    if opts.include_planar {
        out.extend((0..part.faces.len()).filter_map(|f| planar(ctx, f, tol, max_leg)));
    }
    let external: BTreeMap<usize, &CylinderEvidence> = ctx
        .cylinders()
        .iter()
        .filter(|c| c.external)
        .map(|c| (c.face, c))
        .collect();
    out.extend((0..part.faces.len()).filter_map(|f| turned(part, f, tol, max_leg, &external)));
    out.sort_by(|a, b| {
        a.record
            .axis
            .cmp(&b.record.axis)
            .then_with(|| geom::python_order(&a.record.at, &b.record.at))
    });
    out
}

/// An oblique planar bevel on a convex edge between two axis-aligned walls.
fn planar(ctx: &Context<'_>, face: usize, tol: f64, max_leg: f64) -> Option<Occurrence<Chamfer>> {
    let part = ctx.part;
    let bevel = classify_bevel(part, face).ok()?;
    let centre = bevel.bounds.centre();
    if below_minimum(bevel.leg_lo, tol) || bevel.leg_hi > max_leg {
        return None;
    }
    let planes = nearest_axis_aligned_planes(part, face, centre, bevel.edge_axis, false);
    if (0..3).any(|j| j != bevel.edge_axis && !planes.contains_key(&j)) {
        return None;
    }
    if !convex_bevel(ctx, centre, bevel.edge_axis, &planes) {
        return None; // a concave corner: a gusset, rib or web
    }
    let corner = [0, 1, 2].map(|i| {
        geom::round3(if i == bevel.edge_axis {
            centre[i]
        } else {
            planes[&i]
        })
    });
    let at = part.face_centre(face)?;
    Some(Occurrence {
        record: chamfer(
            bevel.edge_axis,
            bevel.leg_hi,
            bevel.leg_lo,
            at,
            false,
            Some(corner),
        ),
        defining: vec![face],
        context: vec![],
    })
}

/// A conical band on turned stock, proved by a coaxial external cylinder and either a
/// transverse shoulder plane or a second band.
fn turned(
    part: &Part,
    face: usize,
    tol: f64,
    max_leg: f64,
    external: &BTreeMap<usize, &CylinderEvidence>,
) -> Option<Occurrence<Chamfer>> {
    let Surface::Cone { frame, .. } = part.faces[face].surface else {
        return None;
    };
    let ((minor_r, minor_c), (major_r, major_c), _) = cone_rims(part, face)?;
    let d = frame.z;
    let edge_axis = dominant_axis(d.map(f64::abs));
    if d[edge_axis].abs() < AXIS_ALIGNED_COS {
        return None;
    }
    let radial = major_r - minor_r;
    let axial = geom::dot(geom::sub(major_c, minor_c), d).abs();
    let (leg_hi, leg_lo) = (axial.max(radial), axial.min(radial));
    if below_minimum(leg_lo, tol) || leg_hi > max_leg {
        return None;
    }
    let neighbours = part.neighbours(face);
    let coaxial: Vec<&CylinderEvidence> = neighbours
        .iter()
        .filter_map(|n| external.get(n).copied())
        .filter(|c| {
            coaxial_axis_lines(
                frame.origin,
                d,
                c.axis_point,
                c.direction,
                geom::length_tol(c.diameter, COAXIAL_FRAC),
            )
        })
        .collect();
    if coaxial.is_empty() {
        return None;
    }
    let transverse = neighbours.iter().any(|&n| match part.faces[n].surface {
        Surface::Plane { frame: plane } => geom::dot(plane.z, d).abs() > AXIS_ALIGNED_COS,
        _ => false,
    });
    let mut diameters: Vec<f64> = coaxial.iter().map(|c| c.diameter).collect();
    diameters.sort_by(f64::total_cmp);
    diameters.dedup();
    if !transverse && diameters.len() < 2 {
        return None; // a taper meeting one band, not a bevelled edge
    }
    let at = part.face_centre(face)?;
    Some(Occurrence {
        record: chamfer(edge_axis, leg_hi, leg_lo, at, true, None),
        defining: vec![face],
        context: vec![],
    })
}
