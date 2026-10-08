//! Original-face proof for a polygonal passage through a convex two-plane roof
//! (`quiddity._plane_envelope_passages`): a closed cycle of planar walls, concave to one another,
//! convex to an original planar mouth at one end and to two roof planes meeting in a convex
//! ridge at the other. The removed cell (the mouth polygon swept along the run and cut by both
//! roof planes) must be covered face for face by the walls, the mouth and the roofs, hold no
//! material, and open into air beyond either end.
//!
//! The cell is convex, so it is built as the intersection of its half-spaces rather than by
//! splitting a prism (Python's `Solid.split`), and the probe beyond each end (Python's translated
//! copy less the cell, `Shape.cut`) is measured as the translated copy less its common part with
//! the cell, which is again convex. An unanswered probe proves nothing, so refuses.

use std::collections::{BTreeMap, BTreeSet};

use super::Context;
use super::entry_treatments::material_fraction;
use super::evidence::common_valid_solid;
use super::graph::{face_vertices, is_planar, normal};
use super::section_passages::{components, ordered_cycle, pair_line};
use super::sections::{LocalFrame, PlanarSection, V2};
use super::support_patches::{covered_patch, polygon_face, polygon_part};
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, Surface, V3};
use crate::kernel::py;
use crate::kernel::rays::RayCaster;
use crate::kernel::volume::{Probe, common_volume};

/// One roof plane as the end surface reads it: the height over the frame origin and the
/// gradient across the section (`PlaneTerm`).
pub type PlaneTerm = (f64, V2);

/// A proved passage (`PlaneEnvelopePassageProof`): its walls, the mouth, the two roofs, the
/// solid, the frame through the section's centroid, the centred section, the run interval, which
/// end is the roof envelope (0 low, 1 high), the two roof terms in order and the cell's volume.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaneEnvelopePassageProof {
    pub walls: Vec<usize>,
    pub planar_context: usize,
    pub roof_contexts: [usize; 2],
    pub owner: usize,
    pub frame: LocalFrame,
    pub section: PlanarSection,
    pub run_interval: (f64, f64),
    pub envelope_end: usize,
    pub terms: [PlaneTerm; 2],
    pub volume: f64,
}

/// A half-space `normal · p <= at`, its normal unit and outward.
type HalfSpace = (V3, f64);

/// Corners nearer than this are one corner, and a corner this near a plane lies on it.
const CORNER_TOL: f64 = 1e-7;

/// A point on a planar face's surface (Python reads the plane through the face's centre, which
/// lies on the same surface).
fn plane_point(part: &Part, face: usize) -> Option<V3> {
    match part.faces[face].surface {
        Surface::Plane { frame } => Some(frame.origin),
        _ => None,
    }
}

/// The convex polyhedron bounded by *planes*, as a one-solid part, with each face's outward
/// normal; `None` when it is empty, flat or not a valid solid.
fn convex_cell(planes: &[HalfSpace]) -> Option<(Part, Vec<V3>)> {
    let mut points: Vec<V3> = Vec::new();
    for i in 0..planes.len() {
        for j in i + 1..planes.len() {
            for k in j + 1..planes.len() {
                let ((a, da), (b, db), (c, dc)) = (planes[i], planes[j], planes[k]);
                let determinant = geom::dot(a, geom::cross(b, c));
                if determinant.abs() < 1e-10 {
                    continue;
                }
                let sum = geom::add(
                    geom::add(
                        geom::scale(geom::cross(b, c), da),
                        geom::scale(geom::cross(c, a), db),
                    ),
                    geom::scale(geom::cross(a, b), dc),
                );
                let point = geom::scale(sum, 1.0 / determinant);
                if planes
                    .iter()
                    .all(|&(n, at)| geom::dot(n, point) <= at + CORNER_TOL)
                    && !points.iter().any(|&o| geom::dist(point, o) < CORNER_TOL)
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
    let mut normals: Vec<V3> = Vec::new();
    for &(n, at) in planes {
        let mut corners: Vec<usize> = (0..points.len())
            .filter(|&p| (geom::dot(n, points[p]) - at).abs() < CORNER_TOL)
            .collect();
        if corners.len() < 3 {
            continue;
        }
        let mut centre = [0.0; 3];
        for &c in &corners {
            centre = geom::add(centre, geom::scale(points[c], 1.0 / corners.len() as f64));
        }
        let u = geom::unit(geom::sub(points[corners[0]], centre))?;
        let v = geom::cross(n, u);
        let angle = |p: usize| {
            let d = geom::sub(points[p], centre);
            geom::dot(d, v).atan2(geom::dot(d, u))
        };
        corners.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
        faces.push(corners);
        normals.push(n);
    }
    let part = polygon_part(&points, &faces, true)?;
    part.solid_is_valid(0).then_some((part, normals))
}

/// The cell's volume (`cell.volume`).
fn volume(cell: &Part) -> Option<f64> {
    cell.solid_mass(0).map(|m| m.0)
}

/// The material *owner* holds inside *probe* (`probe_volume`).
fn material(ctx: &Context<'_>, owner: usize, probe: &Part) -> Option<f64> {
    common_volume(
        ctx.solid_classifier(owner),
        &Probe::Solid(RayCaster::for_solid(probe, 0)),
    )
}

/// The passage proof for one mouth and one concave component of its convex planar neighbours
/// along the run of *base* (`_prove`); `None` for anything outside the contract.
pub fn prove(
    ctx: &Context<'_>,
    mouth_node: usize,
    walls: &[usize],
    base: &LocalFrame,
) -> Option<PlaneEnvelopePassageProof> {
    let part = ctx.part;
    if walls.len() < 3 {
        return None;
    }
    let run = base.run;
    let index = usize::from(geom::dot(normal(part, mouth_node)?, run) < 0.0);
    let sign = if index == 1 { 1.0 } else { -1.0 };
    let own: BTreeSet<usize> = walls.iter().copied().collect();
    let contexts: BTreeSet<usize> = walls
        .iter()
        .flat_map(|&w| part.neighbours(w))
        .filter(|n| !own.contains(n) && *n != mouth_node)
        .collect();
    let roofs: Vec<usize> = contexts.into_iter().collect();
    let &[first_roof, second_roof] = roofs.as_slice() else {
        return None;
    };
    let mut roof_normals = Vec::new();
    for &roof in &roofs {
        let n = normal(part, roof).filter(|_| is_planar(part, roof))?;
        if geom::dot(n, run) * sign <= 1e-8 {
            return None;
        }
        roof_normals.push(n);
    }
    if part.arc(first_roof, second_roof) != Some(Arc::Convex)
        || part.shared_edges(first_roof, second_roof).is_empty()
    {
        return None;
    }
    if walls
        .iter()
        .any(|&w| !roofs.iter().any(|&r| part.arc(w, r) == Some(Arc::Convex)))
    {
        return None;
    }
    let mut members = walls.to_vec();
    members.push(mouth_node);
    members.extend(&roofs);
    let owner = common_valid_solid(part, &members)?;
    let adjacency: BTreeMap<usize, BTreeSet<usize>> = walls
        .iter()
        .map(|&a| {
            let joined = walls
                .iter()
                .copied()
                .filter(|&b| b != a && part.arc(a, b) == Some(Arc::Concave))
                .collect();
            (a, joined)
        })
        .collect();
    if adjacency.values().any(|n| n.len() != 2) {
        return None;
    }
    let mut lines: BTreeMap<(usize, usize), [f64; 4]> = BTreeMap::new();
    for (i, &a) in walls.iter().enumerate() {
        for &b in &walls[i + 1..] {
            if adjacency[&a].contains(&b) {
                lines.insert((a.min(b), a.max(b)), pair_line(part, a, b, base)?);
            }
        }
    }
    let order = ordered_cycle(walls, &adjacency, &lines).ok()?;
    if order.iter().collect::<BTreeSet<_>>().len() != walls.len() {
        return None;
    }
    let line = |a: usize, b: usize| lines[&(a.min(b), a.max(b))];
    let corners: Vec<V2> = (0..order.len())
        .map(|i| {
            let l = line(order[i], order[(i + 1) % order.len()]);
            [l[0], l[1]]
        })
        .collect();
    let raw = PlanarSection::polygon(&corners).ok()?;
    let points: Vec<V2> = raw.boundary().iter().map(|v| v.point).collect();
    let n = points.len();
    for (i, p) in points.iter().enumerate() {
        let (previous, following) = (points[(i + n - 1) % n], points[(i + 1) % n]);
        if (p[0] - previous[0]) * (following[1] - p[1])
            - (p[1] - previous[1]) * (following[0] - p[0])
            < 0.0
        {
            return None;
        }
    }
    let world = |point: V2, height: f64| -> V3 {
        geom::add(
            geom::add(geom::scale(base.u, point[0]), geom::scale(base.v, point[1])),
            geom::scale(run, height),
        )
    };
    let far = geom::dot(plane_point(part, mouth_node)?, run);
    let heights: Vec<f64> = walls
        .iter()
        .flat_map(|&w| face_vertices(part, w))
        .map(|p| geom::dot(p, run))
        .collect();
    let span = heights
        .iter()
        .map(|h| (h - far).abs())
        .fold(f64::NEG_INFINITY, f64::max);
    if span <= 1e-6 {
        return None;
    }
    let extreme = if index == 1 {
        heights.iter().copied().fold(f64::NEG_INFINITY, f64::max)
    } else {
        heights.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let bound = extreme + sign * 1e-3f64.max(span * 0.01);
    // The mouth polygon swept from the mouth to the bound, cut below each roof plane.
    let mut planes: Vec<HalfSpace> = Vec::new();
    for i in 0..n {
        let (p, q) = (points[i], points[(i + 1) % n]);
        let outward = geom::unit(geom::sub(
            geom::scale(base.u, q[1] - p[1]),
            geom::scale(base.v, q[0] - p[0]),
        ))?;
        planes.push((outward, geom::dot(outward, world(p, 0.0))));
    }
    let (low, high) = (far.min(bound), far.max(bound));
    planes.push((geom::scale(run, -1.0), -low));
    planes.push((run, high));
    let mut roof_points = Vec::new();
    for (&roof, &n) in roofs.iter().zip(&roof_normals) {
        let at = plane_point(part, roof)?;
        roof_points.push(at);
        planes.push((n, geom::dot(n, at)));
    }
    let (cell, normals) = convex_cell(&planes)?;
    let cell_volume = volume(&cell)?;
    if cell_volume <= 1e-12 {
        return None;
    }
    let faces: Vec<usize> = (0..cell.faces.len()).collect();
    let lateral: Vec<usize> = faces
        .iter()
        .copied()
        .filter(|&f| geom::dot(normals[f], run).abs() < 1e-8)
        .collect();
    let source: Vec<(&Part, usize)> = walls.iter().map(|&w| (part, w)).collect();
    let lateral_refs: Vec<(&Part, usize)> = lateral.iter().map(|&f| (&cell, f)).collect();
    if lateral_refs.iter().any(|&f| !covered_patch(f, &source))
        || source.iter().any(|&f| !covered_patch(f, &lateral_refs))
    {
        return None;
    }
    let opposite: Vec<usize> = faces
        .iter()
        .copied()
        .filter(|&f| geom::dot(normals[f], run) * sign < -1.0 + 1e-8)
        .collect();
    let mouth = polygon_face(&points.iter().map(|&p| world(p, far)).collect::<Vec<_>>())?;
    let opposite_refs: Vec<(&Part, usize)> = opposite.iter().map(|&f| (&cell, f)).collect();
    if !covered_patch((&mouth, 0), &opposite_refs)
        || opposite_refs
            .iter()
            .any(|&f| !covered_patch(f, &[(&mouth, 0)]))
    {
        return None;
    }
    let terminal: Vec<usize> = faces
        .iter()
        .copied()
        .filter(|f| !lateral.contains(f) && !opposite.contains(f))
        .collect();
    let mut matches: Vec<usize> = Vec::new();
    for (&n, &at) in roof_normals.iter().zip(&roof_points) {
        let selected: Vec<usize> = terminal
            .iter()
            .copied()
            .filter(|&f| {
                geom::dot(normals[f], n) > 1.0 - 1e-8
                    && face_vertices(&cell, f)
                        .iter()
                        .all(|&p| geom::dot(geom::sub(p, at), n).abs() < 1e-6)
            })
            .collect();
        // An area the kernel cannot integrate proves nothing.
        let area: f64 = selected
            .iter()
            .map(|&f| cell.face_mass(f).map(|m| m[0]))
            .sum::<Option<f64>>()?;
        if selected.is_empty() || area <= 1e-10 {
            return None;
        }
        matches.extend(selected);
    }
    if matches.len() != terminal.len()
        || terminal
            .iter()
            .any(|f| matches.iter().filter(|m| *m == f).count() != 1)
    {
        return None;
    }
    if material_fraction(ctx, owner, &cell).is_none_or(|f| f > 1e-9) {
        return None;
    }
    // Beyond either end: the cell moved along the run, less what it shares with the cell.
    let step = 2e-5f64.max(span * 1e-4);
    for direction in [-1.0, 1.0] {
        let offset = geom::scale(run, direction * step);
        let moved: Vec<HalfSpace> = planes
            .iter()
            .map(|&(n, at)| (n, at + geom::dot(n, offset)))
            .collect();
        let (shifted, _) = convex_cell(&moved)?;
        let both: Vec<HalfSpace> = planes.iter().chain(&moved).copied().collect();
        let common = convex_cell(&both);
        let (common_volume, common_material) = match &common {
            Some((c, _)) => (volume(c)?, material(ctx, owner, c)?),
            None => (0.0, 0.0),
        };
        let probe_volume = volume(&shifted)? - common_volume;
        let probe_material = material(ctx, owner, &shifted)? - common_material;
        if probe_volume <= 1e-12 || probe_material / probe_volume > 1e-9 {
            return None;
        }
    }
    let centroid = raw.centroid();
    let origin = world(centroid, 0.0);
    let frame = LocalFrame::canonical(run, origin).ok()?;
    let section = raw.translated([-centroid[0], -centroid[1]]).ok()?;
    let mut terms: Vec<PlaneTerm> = roof_normals
        .iter()
        .zip(&roof_points)
        .map(|(&n, &at)| {
            let divisor = geom::dot(n, run);
            (
                geom::dot(geom::sub(at, origin), n) / divisor,
                [
                    -geom::dot(n, frame.u) / divisor,
                    -geom::dot(n, frame.v) / divisor,
                ],
            )
        })
        .collect();
    let height = if index == 1 {
        terms.iter().map(|t| t.0).fold(f64::INFINITY, f64::min)
    } else {
        terms.iter().map(|t| t.0).fold(f64::NEG_INFINITY, f64::max)
    };
    terms.sort_by(|a, b| py::tuple_order(&[a.0, a.1[0], a.1[1]], &[b.0, b.1[0], b.1[1]]));
    Some(PlaneEnvelopePassageProof {
        walls: own.into_iter().collect(),
        planar_context: mouth_node,
        roof_contexts: [first_roof, second_roof],
        owner,
        frame,
        section,
        run_interval: if index == 1 {
            (far, height)
        } else {
            (height, far)
        },
        envelope_end: index,
        terms: [terms[0], terms[1]],
        volume: cell_volume,
    })
}

/// Every proved passage (`plane_envelope_passage_proofs`): from each planar mouth, its convex
/// planar neighbours along its normal, grouped by concave adjacency, each group proved. Uses
/// original mouth/wall adjacency, without expected side counts or labels.
pub fn plane_envelope_passage_proofs(ctx: &Context<'_>) -> Vec<PlaneEnvelopePassageProof> {
    let part = ctx.part;
    let mut found = Vec::new();
    for mouth in 0..part.faces.len() {
        if !is_planar(part, mouth) {
            continue;
        }
        let Some(base) = normal(part, mouth).and_then(|n| LocalFrame::canonical(n, [0.0; 3]).ok())
        else {
            continue;
        };
        let candidates: BTreeSet<usize> = part
            .neighbours(mouth)
            .into_iter()
            .filter(|&w| {
                is_planar(part, w)
                    && normal(part, w).is_some_and(|n| geom::dot(n, base.run).abs() < 1e-8)
                    && part.arc(mouth, w) == Some(Arc::Convex)
            })
            .collect();
        let adjacency: BTreeMap<usize, BTreeSet<usize>> = candidates
            .iter()
            .map(|&a| {
                let joined = candidates
                    .iter()
                    .copied()
                    .filter(|&b| b != a && part.arc(a, b) == Some(Arc::Concave))
                    .collect();
                (a, joined)
            })
            .collect();
        for walls in components(&candidates, &adjacency) {
            if let Some(proof) = prove(ctx, mouth, &walls, &base) {
                found.push(proof);
            }
        }
    }
    found
}
