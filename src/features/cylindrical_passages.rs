//! Original wall-cycle proof for a polygonal passage meeting a native cross-bore
//! (`quiddity._cylindrical_passages`): a closed convex cycle of planar walls, concave to one
//! another, convex to an original planar mouth at one end and to a native concave cylinder whose
//! axis lies across the run at the other. The removed cell (the mouth polygon swept to the bore)
//! must be covered by the walls, hold no material and open into air beyond either end.
//!
//! Python builds the cell as the mouth's extrusion to the bore's axis less the bore
//! (`Shape.cut`). With the branch strictly separated from the mouth over the whole section, that
//! is exactly the mouth polygon swept from the mouth to the near branch, built here by
//! [`capped_prism`](super::cylindrical_channels::capped_prism): its end face is the mouth
//! polygon and every face is planar or cylindrical by construction (Python's face census and
//! mouth coverage checks), and each probe beyond an end is that end swept by the probe
//! thickness.

use std::collections::{BTreeMap, BTreeSet};

use super::Context;
use super::cylindrical_channels::{
    Cap, capped_prism, covered_along, empty, end_probe, native_bore, sides, volume,
};
use super::effective_surfaces::EffectiveFaces;
use super::evidence::common_valid_solid;
use super::graph::{is_planar, normal};
use super::section_passages::{components, ordered_cycle, pair_line};
use super::sections::{LocalFrame, PlanarSection, V2};
use super::support_patches::covered_patch;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, Surface, V3};

/// A proved passage end (`CylindricalPassageProof`): the walls sorted by face, the bore, the
/// mouth, the solid, the frame through the section's centroid, the centred section, the run
/// interval from the mouth to the branch over that centroid, which end is the bore (0 low,
/// 1 high), the bore's axis point, direction and radius, and the cell's volume.
#[derive(Clone, Debug, PartialEq)]
pub struct CylindricalPassageProof {
    pub walls: Vec<usize>,
    pub cylinder: usize,
    pub planar_context: usize,
    pub owner: usize,
    pub frame: LocalFrame,
    pub section: PlanarSection,
    pub run_interval: (f64, f64),
    pub cylindrical_end: usize,
    pub axis_point: V3,
    pub axis_direction: V3,
    pub radius: f64,
    pub volume: f64,
}

/// The passage proof for one mouth, one bore and one concave component of the walls they share
/// (`_cell_proof`), the walls in the order given; `None` for anything outside the contract.
pub fn prove(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
    planar: usize,
    cylinder: usize,
    walls: &[usize],
    base: &LocalFrame,
) -> Option<CylindricalPassageProof> {
    let part = ctx.part;
    let (centre, axis, radius) = native_bore(ctx, surfaces, cylinder)?;
    let mut members = walls.to_vec();
    members.extend([planar, cylinder]);
    let owner = common_valid_solid(part, &members)?;
    if walls.len() < 3 {
        return None;
    }
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
    // Caller supplies a connected component, so degree two now means one cycle.
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
    let turns: Vec<f64> = (0..n)
        .map(|i| {
            let (p, before, after) = (points[i], points[(i + n - 1) % n], points[(i + 1) % n]);
            (p[0] - before[0]) * (after[1] - p[1]) - (p[1] - before[1]) * (after[0] - p[0])
        })
        .collect();
    if turns.iter().any(|&t| t < 0.0) {
        return None;
    }
    let run = base.run;
    let mouth_normal = normal(part, planar)?;
    // Python reads the plane through the face's centre, which lies on the same surface.
    let Surface::Plane { frame: plane } = part.faces[planar].surface else {
        return None;
    };
    let far = geom::dot(plane.origin, run);
    let end = usize::from(geom::dot(mouth_normal, run) < 0.0);
    let sign = if end == 1 { 1.0 } else { -1.0 };
    if (geom::dot(centre, run) - far) * sign <= 1e-6 {
        return None;
    }
    let world = |point: V2, height: f64| -> V3 {
        geom::add(
            geom::add(geom::scale(base.u, point[0]), geom::scale(base.v, point[1])),
            geom::scale(run, height),
        )
    };
    let transverse = geom::unit(geom::cross(axis, run))?;
    let offsets: Vec<f64> = points
        .iter()
        .map(|&p| geom::dot(geom::sub(world(p, far), centre), transverse))
        .collect();
    if radius - offsets.iter().fold(0.0f64, |m, q| m.max(q.abs())) <= 1e-6 {
        return None;
    }
    // Bound the entire branch, including an interior crest, with the source axis tilt
    // retained. q and the centre-line correction are affine over the convex domain. Their
    // separate extrema give a conservative separation proof.
    let low_q = offsets.iter().copied().fold(f64::INFINITY, f64::min);
    let high_q = offsets.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let qmin = if low_q <= 0.0 && 0.0 <= high_q {
        0.0
    } else {
        offsets.iter().fold(f64::INFINITY, |m, q| m.min(q.abs()))
    };
    let k = geom::dot(axis, run);
    let a = 1.0 - k * k;
    let centres: Vec<f64> = points
        .iter()
        .map(|&p| {
            let delta = geom::sub(world(p, 0.0), centre);
            -(geom::dot(delta, run) - geom::dot(delta, axis) * k) / a
        })
        .collect();
    let radius_height = ((radius * radius - qmin * qmin) / a).sqrt();
    let separation = if end == 1 {
        centres.iter().copied().fold(f64::INFINITY, f64::min) - radius_height - far
    } else {
        far - centres.iter().copied().fold(f64::NEG_INFINITY, f64::max) - radius_height
    };
    if separation.is_nan() || separation <= 1e-6 {
        return None;
    }
    let footprint: Vec<V3> = points.iter().map(|&p| world(p, far)).collect();
    let branch = Cap::Branch {
        centre,
        axis,
        radius,
        sign: -sign,
    };
    let (low, high) = if end == 1 {
        (Cap::Plane(far), branch)
    } else {
        (branch, Cap::Plane(far))
    };
    let cell = capped_prism(run, &footprint, low, high)?;
    let cell_volume = volume(&cell).filter(|&v| v > 1e-12)?;
    let side_patches = sides(&cell);
    let sources: Vec<(&Part, usize)> = walls.iter().map(|&w| (part, w)).collect();
    if side_patches.iter().any(|&f| !covered_patch(f, &sources))
        || sources
            .iter()
            .any(|&f| !covered_along(run, f, &side_patches))
    {
        return None;
    }
    if !empty(ctx, owner, Some(&cell)) {
        return None;
    }
    let thickness = 2e-5f64.max(radius * 1e-4);
    for by in [-thickness, thickness] {
        let probe = end_probe(run, &footprint, low, high, by);
        if probe.as_ref().and_then(volume).is_none_or(|v| v <= 1e-12)
            || !empty(ctx, owner, probe.as_ref())
        {
            return None;
        }
    }
    let centroid = raw.centroid();
    let origin = world(centroid, 0.0);
    let frame = LocalFrame::canonical(run, origin).ok()?;
    let section = raw.translated([-centroid[0], -centroid[1]]).ok()?;
    let delta = geom::sub(origin, centre);
    let b = 2.0 * (geom::dot(delta, run) - geom::dot(delta, axis) * k);
    let c = geom::dot(delta, delta) - geom::dot(delta, axis).powi(2) - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant <= 0.0 {
        return None;
    }
    let height = (-b - sign * discriminant.sqrt()) / (2.0 * a);
    let mut sorted_walls = walls.to_vec();
    sorted_walls.sort_unstable();
    Some(CylindricalPassageProof {
        walls: sorted_walls,
        cylinder,
        planar_context: planar,
        owner,
        frame,
        section,
        run_interval: if end == 1 {
            (far, height)
        } else {
            (height, far)
        },
        cylindrical_end: end,
        axis_point: centre,
        axis_direction: axis,
        radius,
        volume: cell_volume,
    })
}

/// The native concave cylinders, in face order (`_native_bores`).
pub fn native_bores(ctx: &Context<'_>, surfaces: &EffectiveFaces<'_, '_>) -> Vec<usize> {
    (0..ctx.part.faces.len())
        .filter(|&f| native_bore(ctx, surfaces, f).is_some())
        .collect()
}

/// Every proved passage end (`cylindrical_passage_proofs`): from each planar mouth and each
/// bore across its normal, the planar walls convex to both along the run, grouped by concave
/// adjacency, each group proved. Discovers original closed wall cycles; expected side counts
/// are not inputs.
pub fn cylindrical_passage_proofs(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
) -> Vec<CylindricalPassageProof> {
    let part = ctx.part;
    let bores: Vec<(usize, V3)> = native_bores(ctx, surfaces)
        .into_iter()
        .filter_map(|f| native_bore(ctx, surfaces, f).map(|(_, axis, _)| (f, axis)))
        .collect();
    let mut found = Vec::new();
    for planar in 0..part.faces.len() {
        if !is_planar(part, planar) {
            continue;
        }
        let Some(base) = normal(part, planar).and_then(|n| LocalFrame::canonical(n, [0.0; 3]).ok())
        else {
            continue;
        };
        let around: BTreeSet<usize> = part.neighbours(planar).into_iter().collect();
        for &(cylinder, axis) in &bores {
            if geom::dot(axis, base.run).abs() > 1e-8 {
                continue;
            }
            let candidates: BTreeSet<usize> = part
                .neighbours(cylinder)
                .into_iter()
                .filter(|n| around.contains(n))
                .filter(|&n| {
                    is_planar(part, n)
                        && normal(part, n).is_some_and(|m| geom::dot(m, base.run).abs() < 1e-8)
                        && part.arc(planar, n) == Some(Arc::Convex)
                        && part.arc(cylinder, n) == Some(Arc::Convex)
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
                if let Some(proof) = prove(ctx, surfaces, planar, cylinder, &walls, &base) {
                    found.push(proof);
                }
            }
        }
    }
    found
}
