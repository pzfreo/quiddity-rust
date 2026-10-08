//! Finite planar entry bevels explaining the missing patches of an otherwise constant wall ring
//! (`quiddity._entry_treatments`): a passage whose mouth edge is chamfered keeps its base section
//! when every wall reaches the stock face directly or through one convex planar bevel, the
//! removed cell under each bevel is a convex polyhedron empty of material, and the walls and
//! cells together cover the whole base wall up to the stock.
//!
//! The generated cell faces are internal proof supports, never source evidence.

use std::collections::BTreeSet;

use super::Context;
use super::evidence::common_valid_solid;
use super::graph::{face_vertices, is_planar, normal};
use super::support_patches::{covered_patch, polygon_face, polygon_part};
use crate::kernel::brep::{Arc, Part};
use crate::kernel::cover::FaceRef;
use crate::kernel::geom::{self, V3};
use crate::kernel::rays::RayCaster;
use crate::kernel::volume::{Probe, common_volume, probe_volume};

/// The bevels that explain a ring's entry, and the stock faces they meet (`EntryTreatmentProof`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryTreatmentProof {
    pub treatments: BTreeSet<usize>,
    pub stock: BTreeSet<usize>,
}

/// A half-space `normal · p >= at`.
type Plane = (V3, f64);

/// A planar face's plane, its normal scaled by *factor*, through the face's vertices' mean
/// (Python reads it through the face's centre: the same plane).
fn plane(part: &Part, face: usize, factor: f64) -> Option<Plane> {
    let n = geom::scale(normal(part, face)?, factor);
    let corners = face_vertices(part, face);
    if corners.is_empty() {
        return None;
    }
    let mut centre = [0.0; 3];
    for c in &corners {
        centre = geom::add(centre, geom::scale(*c, 1.0 / corners.len() as f64));
    }
    Some((n, geom::dot(n, centre)))
}

fn arc_is(part: &Part, a: usize, b: usize, kinds: &[Arc]) -> bool {
    part.arc(a, b).is_some_and(|k| kinds.contains(&k))
}

/// The fraction of *probe*'s volume that *owner*'s material fills (`material_fraction`).
pub fn material_fraction(ctx: &Context<'_>, owner: usize, probe: &Part) -> Option<f64> {
    let probe = Probe::Solid(RayCaster::for_solid(probe, 0));
    let whole = probe_volume(&probe).filter(|&v| v > 0.0)?;
    Some(common_volume(ctx.solid_classifier(owner), &probe)? / whole)
}

/// The removed cell under one bevel (`_cell_supports`): outside the base wall, inside the stock
/// end, above the bevel and inside the ring's other wall planes, as a convex polyhedron whose
/// bevel face is exactly the source bevel and which holds no material.
fn cell_supports(
    ctx: &Context<'_>,
    seed: &BTreeSet<usize>,
    wall: usize,
    bevel: usize,
    run: V3,
    far: f64,
    sign: f64,
) -> Option<Part> {
    let part = ctx.part;
    let mut planes: Vec<Plane> = vec![
        plane(part, bevel, 1.0)?,
        plane(part, wall, -1.0)?,
        (geom::scale(run, -sign), -sign * far),
    ];
    for &node in seed.iter().filter(|&&n| n != wall) {
        planes.push(plane(part, node, 1.0)?);
    }
    let mut points: Vec<V3> = Vec::new();
    for i in 0..planes.len() {
        for j in i + 1..planes.len() {
            for k in j + 1..planes.len() {
                let ((first, a), (second, b), (third, c)) = (planes[i], planes[j], planes[k]);
                let determinant = geom::dot(first, geom::cross(second, third));
                if determinant.abs() < 1e-10 {
                    continue;
                }
                let sum = geom::add(
                    geom::add(
                        geom::scale(geom::cross(second, third), a),
                        geom::scale(geom::cross(third, first), b),
                    ),
                    geom::scale(geom::cross(first, second), c),
                );
                let point = geom::scale(sum, 1.0 / determinant);
                if planes.iter().all(|&(n, d)| geom::dot(n, point) >= d - 1e-7)
                    && !points.iter().any(|&o| geom::dist(point, o) < 1e-7)
                {
                    points.push(point);
                }
            }
        }
    }
    if points.len() < 4 {
        return None;
    }
    let mut faces: Vec<Vec<usize>> = Vec::new();
    let mut bevel_face = None;
    for (index, &(n, at)) in planes.iter().enumerate() {
        let mut corners: Vec<usize> = (0..points.len())
            .filter(|&p| (geom::dot(n, points[p]) - at).abs() < 1e-7)
            .collect();
        if corners.len() < 3 {
            continue;
        }
        let mut centre = [0.0; 3];
        for &c in &corners {
            centre = geom::add(centre, points[c]);
        }
        let centre = geom::scale(centre, 1.0 / corners.len() as f64);
        let u = geom::sub(points[corners[0]], centre);
        let length = geom::norm(u);
        if length <= 1e-12 {
            return None;
        }
        let u = geom::scale(u, 1.0 / length);
        // The plane's normal points into the cell: walk about the outward one.
        let v = geom::cross(geom::scale(n, -1.0), u);
        let angle = |p: usize| {
            let d = geom::sub(points[p], centre);
            geom::dot(d, v).atan2(geom::dot(d, u))
        };
        corners.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
        if index == 0 {
            bevel_face = Some(faces.len());
        }
        faces.push(corners);
    }
    let bevel_face = bevel_face?;
    let cell = polygon_part(&points, &faces, true)?;
    let source: FaceRef<'_> = (part, bevel);
    if !covered_patch(source, &[(&cell, bevel_face)])
        || !covered_patch((&cell, bevel_face), &[source])
    {
        return None;
    }
    if !cell.solid_is_valid(0) || cell.solid_mass(0).is_none_or(|(volume, _)| volume <= 1e-12) {
        return None;
    }
    let mut members: Vec<usize> = seed.iter().copied().collect();
    members.push(bevel);
    let owner = common_valid_solid(part, &members)?;
    // An unanswered probe proves nothing.
    material_fraction(ctx, owner, &cell)
        .is_some_and(|f| f <= 1e-9)
        .then_some(cell)
}

/// Explain every missing base-wall patch of the ring *seed* by finite observed planar
/// treatments (`prove_entry_treatments`): *wire* is the mouth (an inner loop of the opening
/// face, as `(edge, forward)` uses), the walls run along *run* from the mouth at *at* to the
/// stock at *far*. `None` when any wall is unexplained, nothing is treated, or the walls and
/// cells leave part of the base wall uncovered.
pub fn prove_entry_treatments(
    ctx: &Context<'_>,
    seed: &BTreeSet<usize>,
    wire: &[(usize, bool)],
    run: V3,
    at: f64,
    far: f64,
) -> Option<EntryTreatmentProof> {
    let part = ctx.part;
    let sign = if far > at { 1.0 } else { -1.0 };
    let mut contexts = BTreeSet::new();
    for &wall in seed {
        contexts.extend(
            part.neighbours(wall)
                .into_iter()
                .filter(|n| !seed.contains(n)),
        );
    }
    let stock: BTreeSet<usize> = contexts
        .into_iter()
        .filter(|&n| {
            is_planar(part, n)
                && normal(part, n).is_some_and(|m| geom::dot(m, run) * sign > 1.0 - 1e-8)
                && face_vertices(part, n)
                    .iter()
                    .all(|&v| (geom::dot(v, run) - far).abs() < 1e-6)
        })
        .collect();
    if stock.is_empty() {
        return None;
    }
    let mut cells: Vec<Part> = Vec::new();
    let mut treatments = BTreeSet::new();
    for &wall in seed {
        if stock
            .iter()
            .any(|&n| arc_is(part, wall, n, &[Arc::Convex, Arc::Smooth]))
        {
            continue;
        }
        let wall_direction = normal(part, wall)?;
        let mut explained = false;
        let mut bevels: Vec<usize> = part
            .neighbours(wall)
            .into_iter()
            .filter(|n| !seed.contains(n) && !stock.contains(n))
            .collect();
        bevels.sort_unstable();
        for bevel in bevels {
            if !(is_planar(part, bevel) && part.arc(wall, bevel) == Some(Arc::Convex)) {
                continue;
            }
            let Some(bnormal) = normal(part, bevel) else {
                continue;
            };
            let (along, across) = (geom::dot(bnormal, run), geom::dot(bnormal, wall_direction));
            if along * sign <= 1e-6 || across <= 1e-6 {
                continue;
            }
            let rest = geom::sub(
                geom::sub(bnormal, geom::scale(run, along)),
                geom::scale(wall_direction, across),
            );
            if geom::norm(rest) > 1e-6 {
                continue;
            }
            if !stock
                .iter()
                .any(|&n| arc_is(part, bevel, n, &[Arc::Convex]))
            {
                continue;
            }
            if part
                .neighbours(bevel)
                .iter()
                .any(|n| !seed.contains(n) && !stock.contains(n))
            {
                continue;
            }
            let mut members: Vec<usize> = seed.iter().chain(&stock).copied().collect();
            members.push(bevel);
            if common_valid_solid(part, &members).is_none() {
                continue;
            }
            if let Some(cell) = cell_supports(ctx, seed, wall, bevel, run, far, sign) {
                cells.push(cell);
                treatments.insert(bevel);
                explained = true;
            }
        }
        if !explained {
            return None;
        }
    }
    if treatments.is_empty() {
        return None;
    }
    let mut supports: Vec<FaceRef<'_>> = seed.iter().map(|&w| (part, w)).collect();
    for cell in &cells {
        supports.extend((0..cell.faces.len()).map(|f| (cell, f)));
    }
    let delta = geom::scale(run, far - at);
    for &(e, _) in wire {
        let (a, b) = (part.edges[e].start, part.edges[e].end);
        let patch = polygon_face(&[a, b, geom::add(b, delta), geom::add(a, delta)])?;
        if !covered_patch((&patch, 0), &supports) {
            return None;
        }
    }
    Some(EntryTreatmentProof { treatments, stock })
}
