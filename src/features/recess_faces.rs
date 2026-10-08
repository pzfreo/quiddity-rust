//! What a solid's faces say about recesses, before any family has an opinion
//! (`quiddity._recess_faces`): the planar faces the three recess families pair, the
//! axis-aligned cylinders that may be obround ends or rounded corners, and the probes that ask a
//! candidate whether its depth ends are floored and whether side walls join its ends.

use super::graph;
use super::recess_records::Recess;
use crate::kernel::brep::Part;
use crate::kernel::geom::{Bounds, Curve, Surface, V3};

/// What "the same coordinate" means across the recess modules (`_MERGE_TOL`), in mm.
pub const MERGE_TOL: f64 = 0.5;
/// What "the same floor plane" means (`_FLOOR_TOL`), in mm.
pub const FLOOR_TOL: f64 = 0.3;
/// How much of a footprint the faces at one depth end must cover to cap it.
const FLOOR_COVER_FRAC: f64 = 0.5;
const AXIS_ALIGNED_TOL: f64 = 1e-3;

/// The axis a unit vector lies along, if any (`_dominant_axis`).
pub fn dominant_axis(n: V3) -> Option<usize> {
    (0..3).find(|&k| (n[k].abs() - 1.0).abs() <= AXIS_ALIGNED_TOL)
}

/// A planar face reduced to what the recognisers decide by (`_Face`). `axis` is `None` for an
/// oblique face, which is carried so the family that cannot pair it is the one to decline it.
#[derive(Clone, Debug)]
pub struct Face {
    pub normal: V3,
    pub axis: Option<usize>,
    pub bb: Bounds,
    /// Bounded only by lines and arcs, at least one line and at most one arc (`_is_wall`).
    pub wall: bool,
    pub node: usize,
}

/// Bounded only by straight and circular edges, with at least one straight edge and at most
/// one arc (`_is_wall`): a turned groove's annulus has two concentric arcs and never qualifies.
fn is_wall(part: &Part, face: usize) -> bool {
    let edges = part.face_edges(face);
    let (mut lines, mut circles) = (0, 0);
    for e in &edges {
        match part.edges[*e].curve {
            Curve::Line { .. } => lines += 1,
            Curve::Circle { .. } => circles += 1,
            _ => return false,
        }
    }
    !edges.is_empty() && lines >= 1 && circles <= 1
}

/// Every planar face of the solid with a readable normal, in face order (`_planar_faces`).
pub fn planar_faces(part: &Part, solid: usize) -> Vec<Face> {
    part.solids[solid]
        .faces
        .iter()
        .filter(|&&f| graph::is_planar(part, f))
        .filter_map(|&f| {
            let normal = graph::normal(part, f)?;
            Some(Face {
                normal,
                axis: dominant_axis(normal),
                bb: part.face_bounds(f),
                wall: is_wall(part, f),
                node: f,
            })
        })
        .collect()
}

pub fn center(bb: &Bounds, k: usize) -> f64 {
    (bb.min[k] + bb.max[k]) / 2.0
}

/// The length of two boxes' overlap along an axis, negative when they are apart.
pub fn overlap_len(a: &Bounds, b: &Bounds, axis: usize) -> f64 {
    a.max[axis].min(b.max[axis]) - a.min[axis].max(b.min[axis])
}

/// The inward faces at one depth end that together cover at least half of the footprint, or
/// none (`_end_cap_faces`). *foot* is the footprint's (axis, span) pairs, width first; *want*
/// the sign the covering normal must have along the depth axis (+ at the low end, − at the
/// high end), which excludes the part's own outer face at that level. Coverage is summed over
/// the faces, so a floor split by a rib still counts.
pub fn end_cap_faces(
    faces: &[Face],
    foot: &[(usize, (f64, f64)); 2],
    foot_area: f64,
    depth_axis: usize,
    end: f64,
    want: f64,
) -> Vec<usize> {
    let mut covered = 0.0;
    let mut selected = Vec::new();
    for f in faces {
        if f.axis != Some(depth_axis) || (center(&f.bb, depth_axis) - end).abs() > FLOOR_TOL {
            continue;
        }
        if f.normal[depth_axis] * want <= 0.0 {
            continue;
        }
        let mut area = 1.0;
        for &(ax, (lo, hi)) in foot {
            let ov = f.bb.max[ax].min(hi) - f.bb.min[ax].max(lo);
            area *= ov.max(0.0);
        }
        covered += area;
        if area > 0.0 {
            selected.push(f.node);
        }
    }
    if covered >= FLOOR_COVER_FRAC * foot_area {
        selected
    } else {
        Vec::new()
    }
}

/// The faces capping each depth end of a record's footprint, low end first
/// (`_floor_end_faces`); an end left open has none.
pub fn floor_end_faces<R: Recess>(faces: &[Face], s: &R) -> (Vec<usize>, Vec<usize>) {
    let foot = [
        (
            s.width_axis(),
            (
                s.w_center() - s.width() / 2.0,
                s.w_center() + s.width() / 2.0,
            ),
        ),
        (s.long_axis(), (s.lo(), s.hi())),
    ];
    let foot_area = 1.0 * (foot[0].1.1 - foot[0].1.0) * (foot[1].1.1 - foot[1].1.0);
    (
        end_cap_faces(faces, &foot, foot_area, s.depth_axis(), s.d_lo(), 1.0),
        end_cap_faces(faces, &foot, foot_area, s.depth_axis(), s.d_hi(), -1.0),
    )
}

/// Whether a planar floor caps either depth end: the record is not a through recess
/// (`_has_floor`).
pub fn has_floor<R: Recess>(faces: &[Face], s: &R) -> bool {
    let (low, high) = floor_end_faces(faces, s);
    !low.is_empty() || !high.is_empty()
}

/// A native cylindrical face along a principal axis (`_cylinder_faces`): a possible obround end
/// or rounded corner. `location` is a point on its axis; `concave` that it bounds a void (its
/// frame does not point out of the material) rather than added material.
#[derive(Clone, Debug)]
pub struct Cylinder {
    pub radius: f64,
    pub axis: usize,
    pub location: V3,
    pub bb: Bounds,
    pub concave: bool,
    pub node: usize,
}

/// Every native cylinder of the solid along a principal axis, in face order.
pub fn cylinder_faces(part: &Part, solid: usize) -> Vec<Cylinder> {
    part.solids[solid]
        .faces
        .iter()
        .filter_map(|&f| {
            let Surface::Cylinder { frame, radius } = &part.faces[f].surface else {
                return None;
            };
            Some(Cylinder {
                radius: *radius,
                axis: dominant_axis(frame.z)?,
                location: frame.origin,
                bb: part.face_bounds(f),
                // Python's `not frame_points_outward(face)`: an unreadable side counts as concave.
                concave: part.frame_points_outward(f) != Some(true),
                node: f,
            })
        })
        .collect()
}

/// Whether both flat side walls of an obround record are present (`_has_side_walls`):
/// inward-facing walls on the width axis at each side of the centreline, each overlapping the
/// straight run. The normal test keeps the stock's outward sides, at the same place when the
/// stock is exactly as wide as the slot, from counting.
pub fn has_side_walls<R: Recess>(faces: &[Face], s: &R) -> bool {
    let (wk, lk) = (s.width_axis(), s.long_axis());
    let (mut lo_wall, mut hi_wall) = (false, false);
    for f in faces {
        if !f.wall || f.axis != Some(wk) {
            continue;
        }
        let (lo, hi) = (f.bb.min[lk], f.bb.max[lk]);
        if hi.min(s.hi()) - lo.max(s.lo()) <= 0.0 {
            continue;
        }
        let c = center(&f.bb, wk);
        if (c - (s.w_center() - s.width() / 2.0)).abs() <= MERGE_TOL && f.normal[wk] > 0.0 {
            lo_wall = true;
        }
        if (c - (s.w_center() + s.width() / 2.0)).abs() <= MERGE_TOL && f.normal[wk] < 0.0 {
            hi_wall = true;
        }
    }
    lo_wall && hi_wall
}

/// The box around two boxes (`_union_bb`).
pub fn union_bb(a: &Bounds, b: &Bounds) -> Bounds {
    let mut out = *a;
    out.merge(b);
    out
}
