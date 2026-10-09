//! Exact native-cylinder termination proofs for polygonal pockets
//! (`quiddity._cylindrical_pockets`): a planar floor bounded by straight edges only, each shared
//! with one of at least three planar walls along the floor's normal, concave to the floor and
//! convex to a native cylinder (the stock) whose axis lies across that normal, with the floor
//! on the outward side of the axis. The removed cell (the floor swept up to the cylinder) must
//! be covered by the original walls, hold no material, sit on material just under the floor and
//! open into air just past the cylinder.
//!
//! Python builds the cell as the floor's extrusion intersected with the cylinder
//! (`Shape.intersect`). With the floor's every corner strictly inside the cylinder's outer
//! branch, that is exactly the floor swept up to the branch, built here by
//! [`capped_prism`](super::cylindrical_channels::capped_prism): its floor face is the source
//! floor by construction (two of Python's checks), and the probe past the cylinder (the cell
//! with the cylinder moved out, less the cell) is the branch swept by the probe thickness.

use super::Context;
use super::analytic_surfaces::SurfaceKind;
use super::cylindrical_channels::{Cap, capped_prism, covered_along, empty, sides, volume};
use super::effective_surfaces::{EffectiveFaces, SurfaceProvenance};
use super::entry_treatments::material_fraction;
use super::evidence::common_valid_solid;
use super::graph::{is_planar, normal};
use super::support_patches::covered_patch;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, Curve, V3};

/// A proved pocket end (`CylindricalPocketProof`): the floor, its walls sorted by face, the
/// stock cylinder, the solid, the cylinder's axis point (moved along the axis level with the
/// floor's centroid), direction and radius, the floor's normal and the cell's volume.
#[derive(Clone, Debug, PartialEq)]
pub struct CylindricalPocketProof {
    pub floor: usize,
    pub walls: Vec<usize>,
    pub stock: usize,
    pub owner: usize,
    pub axis_point: V3,
    pub axis_direction: V3,
    pub run: V3,
    pub radius: f64,
    pub volume: f64,
}

/// The area centroid of a planar polygon with corners *points* in order (`face.center()`).
fn polygon_centroid(points: &[V3]) -> Option<V3> {
    let origin = points[0];
    let (mut area, mut moment) = (0.0, [0.0; 3]);
    let mut normal = [0.0; 3];
    for i in 1..points.len() - 1 {
        normal = geom::add(
            normal,
            geom::cross(
                geom::sub(points[i], origin),
                geom::sub(points[i + 1], origin),
            ),
        );
    }
    let normal = geom::unit(normal)?;
    for i in 1..points.len() - 1 {
        let (b, c) = (points[i], points[i + 1]);
        let twice = geom::dot(
            geom::cross(geom::sub(b, origin), geom::sub(c, origin)),
            normal,
        );
        area += twice;
        let centre = geom::scale(geom::add(geom::add(origin, b), c), 1.0 / 3.0);
        moment = geom::add(moment, geom::scale(centre, twice));
    }
    (area.abs() > 0.0).then(|| geom::scale(moment, 1.0 / area))
}

/// The floor's corners in loop order, when its one loop is all straight edges.
fn floor_corners(part: &Part, floor: usize) -> Option<Vec<V3>> {
    let lp = part.faces[floor].loops.first()?;
    lp.edges
        .iter()
        .map(|&(e, forward)| {
            let edge = &part.edges[e];
            matches!(edge.curve, Curve::Line { .. }).then_some(if forward {
                edge.start
            } else {
                edge.end
            })
        })
        .collect()
}

/// Every proof ending the pocket on *floor* (`_proofs`), one per stock cylinder that proves.
pub fn proofs(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
    floor: usize,
) -> Vec<CylindricalPocketProof> {
    proofs_or_none(ctx, surfaces, floor).unwrap_or_default()
}

fn proofs_or_none(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
    floor: usize,
) -> Option<Vec<CylindricalPocketProof>> {
    let part = ctx.part;
    let mut results = Vec::new();
    if !is_planar(part, floor) || part.faces[floor].loops.len() != 1 {
        return None;
    }
    let run = normal(part, floor)?;
    let walls: Vec<usize> = part
        .neighbours(floor)
        .into_iter()
        .filter(|&n| part.arc(floor, n) == Some(Arc::Concave))
        .collect();
    if walls.len() < 3
        || walls.iter().any(|&n| {
            !is_planar(part, n) || normal(part, n).is_none_or(|m| geom::dot(m, run).abs() > 1e-8)
        })
    {
        return None;
    }
    // A closed polygonal footprint needs a real wall along EVERY floor edge. Area equality
    // alone misses a sideways breakout with a zero-area fourth wall.
    let shared: Vec<usize> = walls
        .iter()
        .flat_map(|&w| part.shared_edges(floor, w))
        .collect();
    if part
        .face_edges(floor)
        .iter()
        .any(|e| !matches!(part.edges[*e].curve, Curve::Line { .. }) || !shared.contains(e))
    {
        return None;
    }
    let corners = floor_corners(part, floor)?;
    let floor_centre = polygon_centroid(&corners)?;
    let mut contexts: Vec<usize> = walls
        .iter()
        .flat_map(|&w| part.neighbours(w))
        .filter(|n| !walls.contains(n) && *n != floor)
        .collect();
    contexts.sort_unstable();
    contexts.dedup();
    let mut sorted_walls = walls.clone();
    sorted_walls.sort_unstable();
    for stock in contexts {
        let Some(fact) = surfaces.fact(stock).as_ref().ok() else {
            continue;
        };
        if fact.kind() != SurfaceKind::Cylinder || fact.provenance() != SurfaceProvenance::Native {
            continue;
        }
        if !walls
            .iter()
            .all(|&w| part.arc(w, stock) == Some(Arc::Convex))
        {
            continue;
        }
        let mut members = vec![floor];
        members.extend(&walls);
        members.push(stock);
        let Some(owner) = common_valid_solid(part, &members) else {
            continue;
        };
        let p = fact.parameters();
        let (centre, axis, radius) = ([p[0], p[1], p[2]], [p[3], p[4], p[5]], p[6]);
        if geom::dot(axis, run).abs() > 1e-8 {
            continue;
        }
        // Initial bounded branch: floor lies on the outward side of the cylinder axis.
        if geom::dot(geom::sub(floor_centre, centre), run) <= 1e-6 {
            continue;
        }
        // For this planar line-bounded floor, radial offset is affine on each edge; its
        // maximum is at a vertex. This proves strictly positive separation on the complete
        // polygon, including boundaries of zero area.
        let separated = corners.iter().all(|&vertex| {
            let delta = geom::sub(vertex, centre);
            let height = geom::dot(delta, run);
            let transverse = geom::sub(
                geom::sub(delta, geom::scale(axis, geom::dot(delta, axis))),
                geom::scale(run, height),
            );
            let discriminant = radius * radius - geom::dot(transverse, transverse);
            discriminant > 0.0 && discriminant.sqrt() - height > 1e-6
        });
        if !separated {
            continue;
        }
        let axis_point = geom::add(
            centre,
            geom::scale(axis, geom::dot(geom::sub(floor_centre, centre), axis)),
        );
        let floor_at = geom::dot(floor_centre, run);
        let branch = Cap::Branch {
            centre: axis_point,
            axis,
            radius,
            sign: 1.0,
        };
        let Some(cell) = capped_prism(run, &corners, Cap::Plane(floor_at), branch) else {
            continue;
        };
        let Some(cell_volume) = volume(&cell).filter(|&v| v > 1e-12) else {
            continue;
        };
        let side_patches = sides(&cell);
        let supports: Vec<(&Part, usize)> = walls.iter().map(|&w| (part, w)).collect();
        if side_patches.iter().any(|&f| !covered_patch(f, &supports))
            || supports
                .iter()
                .any(|&f| !covered_along(run, f, &side_patches))
        {
            continue;
        }
        if !empty(ctx, owner, Some(&cell)) {
            continue;
        }
        let thickness = 2e-5f64.max(radius * 1e-4);
        // The floor sits on material: the slab under it is full.
        let under = capped_prism(
            run,
            &corners,
            Cap::Plane(floor_at - thickness),
            Cap::Plane(floor_at),
        );
        if under
            .as_ref()
            .and_then(|u| material_fraction(ctx, owner, u))
            .is_none_or(|f| f < 1.0 - 1e-9)
        {
            continue;
        }
        let mouth = capped_prism(
            run,
            &corners,
            branch,
            branch.moved(run, geom::scale(run, thickness)),
        );
        if mouth.as_ref().and_then(volume).is_none_or(|v| v <= 1e-12)
            || !empty(ctx, owner, mouth.as_ref())
        {
            continue;
        }
        results.push(CylindricalPocketProof {
            floor,
            walls: sorted_walls.clone(),
            stock,
            owner,
            axis_point,
            axis_direction: axis,
            run,
            radius,
            volume: cell_volume,
        });
    }
    Some(results)
}

/// Every proof on every floor (`cylindrical_pocket_proofs`), floors in face order.
pub fn cylindrical_pocket_proofs(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
) -> Vec<CylindricalPocketProof> {
    (0..ctx.part.faces.len())
        .flat_map(|floor| proofs(ctx, surfaces, floor))
        .collect()
}
