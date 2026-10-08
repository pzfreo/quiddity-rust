//! The convex-corner probe shared by the bevel and blend families (`quiddity._bevel`).

use std::collections::BTreeMap;

use super::Context;
use super::policy::{AXIS_ALIGNED_COS, INTERIOR_PROBE_FRAC};
use crate::kernel::brep::Part;
use crate::kernel::classify::material_at;
use crate::kernel::geom::{Bounds, Surface, V3};

/// The in-plane component below which a normal runs along that axis, so the face is a
/// single-axis bevel rather than a compound corner.
const RUN_AXIS_COS: f64 = 0.05;

/// Why a face is not a single-axis oblique planar bevel (`BevelReject`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BevelReject {
    Nonplanar,
    /// Axis-aligned, or a shallow draft: a real face, not a bevel.
    Aligned,
    /// Oblique on all three axes: a corner bevel.
    Compound,
}

/// A single-axis oblique planar bevel (`classify_bevel`).
#[derive(Clone, Copy, Debug)]
pub struct Bevel {
    /// The axis the bevelled edge runs along.
    pub edge_axis: usize,
    pub normal: V3,
    pub bounds: Bounds,
    /// The two in-plane leg lengths, longer first.
    pub leg_hi: f64,
    pub leg_lo: f64,
}

pub fn classify_bevel(part: &Part, face: usize) -> Result<Bevel, BevelReject> {
    let Surface::Plane { .. } = part.faces[face].surface else {
        return Err(BevelReject::Nonplanar);
    };
    let normal = part
        .face_normal(face, 0.0, 0.0)
        .ok_or(BevelReject::Nonplanar)?;
    if normal.iter().any(|c| c.abs() > AXIS_ALIGNED_COS) {
        return Err(BevelReject::Aligned);
    }
    let edge_axis = (0..3)
        .find(|&i| normal[i].abs() < RUN_AXIS_COS)
        .ok_or(BevelReject::Compound)?;
    let bounds = part.face_bounds(face);
    let legs: Vec<f64> = (0..3)
        .filter(|&j| j != edge_axis)
        .map(|j| bounds.max[j] - bounds.min[j])
        .collect();
    Ok(Bevel {
        edge_axis,
        normal,
        bounds,
        leg_hi: legs[0].max(legs[1]),
        leg_lo: legs[0].min(legs[1]),
    })
}

/// The point just off the virtual sharp corner a bevel replaces: the corner sits where the two
/// neighbour planes cross, at the bevel's own position along its edge; *toward* 1 nudges it to
/// the bevel's side, -1 to the far side (`_near_corner`).
pub fn near_corner(centre: V3, edge_axis: usize, planes: &BTreeMap<usize, f64>, toward: f64) -> V3 {
    let mut corner = [0.0; 3];
    corner[edge_axis] = centre[edge_axis];
    for (&axis, &coord) in planes {
        corner[axis] = coord;
    }
    let step = toward * INTERIOR_PROBE_FRAC;
    [0, 1, 2].map(|i| corner[i] + step * (centre[i] - corner[i]))
}

/// Is there material just beyond the virtual corner, away from the bevel
/// (`material_beyond_corner`)? A bevel on a recess wall replaces the corner where two of its
/// walls meet, with the stock behind it; a bevel on an edge of the part has free space there.
pub fn material_beyond_corner(
    ctx: &Context<'_>,
    centre: V3,
    edge_axis: usize,
    planes: &BTreeMap<usize, f64>,
) -> bool {
    material_at(
        ctx.classifier(),
        near_corner(centre, edge_axis, planes, -1.0),
    )
}

/// Does the virtual sharp corner the bevel replaces lie outside the solid (`convex_bevel`)?
/// Only a clean `IN` counts as material, as with `BRepClass3d`.
pub fn convex_bevel(
    ctx: &Context<'_>,
    centre: V3,
    edge_axis: usize,
    planes: &BTreeMap<usize, f64>,
) -> bool {
    !material_at(
        ctx.classifier(),
        near_corner(centre, edge_axis, planes, 1.0),
    )
}
