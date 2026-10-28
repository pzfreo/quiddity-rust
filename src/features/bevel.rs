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

internal_entry! {
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
}

internal_entry! {
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
}

internal_entry! {
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
}

internal_entry! {
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
}

/// A bevel's virtual corner must stand off its own cutting plane, into free space. A nearest
/// support tie may include the bevel's terminal edge, which is not a removed sharp corner.
/// Evaluate all tied support pairs and accept only one geometrically distinct proved corner.
pub(crate) fn proved_bevel_planes(
    ctx: &super::Context<'_>,
    face: usize,
    centre: V3,
    edge_axis: usize,
) -> Option<BTreeMap<usize, f64>> {
    let crate::kernel::geom::Surface::Plane { frame } = ctx.part.faces[face].surface else {
        return None;
    };
    let candidates =
        super::planes::nearest_axis_aligned_plane_candidates(ctx.part, face, centre, edge_axis);
    // Additional evidence resolves competing supports; preserve the existing single-support
    // API when no tie exists. Auditing untied bevel hypotheses is a separate geometric task.
    let ambiguous = candidates.values().any(|coords| {
        let lo = coords.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = coords.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        hi - lo > crate::kernel::geom::COORD_FLOOR
    });
    let others: Vec<_> = (0..3).filter(|&i| i != edge_axis).collect();
    let mut proved: Option<(V3, BTreeMap<usize, f64>)> = None;
    for &a in candidates.get(&others[0])? {
        for &b in candidates.get(&others[1])? {
            let planes = BTreeMap::from([(others[0], a), (others[1], b)]);
            let mut corner = centre;
            corner[others[0]] = a;
            corner[others[1]] = b;
            if (ambiguous
                && crate::kernel::geom::dot(
                    crate::kernel::geom::sub(corner, frame.origin),
                    frame.z,
                )
                .abs()
                    <= crate::kernel::geom::COORD_FLOOR)
                || !convex_bevel(ctx, centre, edge_axis, &planes)
            {
                continue;
            }
            if let Some((previous, _)) = &proved {
                if crate::kernel::py::dist(previous, &corner) > crate::kernel::geom::COORD_FLOOR {
                    return None;
                }
            } else {
                proved = Some((corner, planes));
            }
        }
    }
    proved.map(|(_, planes)| planes)
}

/// The two supporting planes intersected at a station along the run. Their normals and the
/// corner are measured in caller coordinates; the planes need not be principal planes.
pub(crate) struct TransverseCorner {
    pub point: V3,
    pub normals: [V3; 2],
    pub areas: [f64; 2],
}

/// Read two distinct transverse supports in one neighbour traversal. Coplanar splits are one
/// support; a third direction or a distinct parallel plane is a competing corner claim.
pub(crate) fn transverse_corner(
    part: &Part,
    face: usize,
    centre: V3,
    run: V3,
) -> Option<TransverseCorner> {
    let corner = transverse_wall_corner(part, face, centre, run)?;
    (crate::kernel::geom::dot(corner.normals[0], corner.normals[1]).abs()
        <= crate::kernel::geom::SMOOTH_ARC_GAP)
        .then_some(corner)
}

/// Two nonparallel transverse walls, at any angle. Callers prove the treatment's own
/// angle contract; chamfers retain their perpendicular-wall requirement above.
pub(crate) fn transverse_wall_corner(
    part: &Part,
    face: usize,
    centre: V3,
    run: V3,
) -> Option<TransverseCorner> {
    use super::planes::unit_plane_normal;
    use crate::kernel::geom::{self, COORD_FLOOR, SMOOTH_ARC_GAP};
    let mut supports: Vec<(V3, V3, f64)> = Vec::with_capacity(2);
    for f in part.neighbours(face) {
        let Surface::Plane { frame } = part.faces[f].surface else {
            continue;
        };
        let normal = unit_plane_normal(part, f)?;
        if geom::dot(normal, run).abs() > SMOOTH_ARC_GAP {
            continue;
        }
        let area = part.face_mass(f)?[0];
        if let Some((_, origin, total)) = supports
            .iter_mut()
            .find(|(n, _, _)| geom::dot(*n, normal).abs() >= 1. - SMOOTH_ARC_GAP)
        {
            if geom::dot(geom::sub(frame.origin, *origin), normal).abs() > COORD_FLOOR {
                return None;
            }
            *total += area;
            continue;
        }
        if supports.len() == 2 {
            return None;
        }
        supports.push((normal, frame.origin, area));
    }
    let [(a, pa, aa), (b, pb, ab)] = supports.as_slice() else {
        return None;
    };
    let d = geom::dot(*a, *b);
    let da = geom::dot(geom::sub(*pa, centre), *a);
    let db = geom::dot(geom::sub(*pb, centre), *b);
    Some(TransverseCorner {
        point: geom::add(
            centre,
            geom::add(
                geom::scale(*a, (da - d * db) / (1. - d * d)),
                geom::scale(*b, (db - d * da) / (1. - d * d)),
            ),
        ),
        normals: [*a, *b],
        areas: [*aa, *ab],
    })
}
