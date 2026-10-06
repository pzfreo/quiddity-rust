//! External edge fillet recognition (`quiddity.fillets`).
//!
//! A prismatic fillet is a partial cylinder rounding a convex 90° edge between two axis-aligned
//! walls; a turned fillet is a short external torus meeting a coaxial external cylinder.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use serde::Serialize;

use super::Context;
use super::bevel::convex_bevel;
use super::cylinders::{CylinderEvidence, coaxial_axis_lines};
use super::evidence::{self, EvidenceError, Occurrence};
use super::planes::nearest_axis_aligned_planes;
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, AXIS_ALIGNED_COS, Surface, V3, dominant_axis};

/// A minimum-evidence threshold, deliberately absolute (ADR 0008).
const MIN_RADIUS: f64 = 0.6;
/// A convex edge fillet is a quarter turn; a bore keeps more than half a turn.
const FILLET_MAX_EXTENT: f64 = PI * 1.05;
/// A turned edge fillet is a quarter circle in section; a bead is a half or full one.
const TURNED_FILLET_MAX_EXTENT: f64 = PI / 2.0 * 1.05;
/// Coaxial analytic axes may differ by modelling noise only (a fraction of the diameter).
const COAXIAL_FRAC: f64 = 1e-4;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fillet {
    pub axis: char,
    pub radius: f64,
    pub at: V3,
    pub turned: bool,
    pub side: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct FilletOptions {
    pub min_radius: Option<f64>,
    pub max_radius_frac: f64,
    /// `false` for a part already classified as rotational: only toroidal fillets are reported.
    pub include_cylindrical: bool,
}

impl Default for FilletOptions {
    fn default() -> Self {
        FilletOptions {
            min_radius: None,
            max_radius_frac: 0.45,
            include_cylindrical: true,
        }
    }
}

/// `recognise_fillets`: the records only.
pub fn recognise_fillets(part: &Part, opts: &FilletOptions) -> Vec<Fillet> {
    discover(&Context::new(part), opts)
        .into_iter()
        .map(|o| o.record)
        .collect()
}

/// The evidence path: each fillet with its defining face, published only if every one is
/// owned by a valid solid.
pub fn discover_verified(
    ctx: &Context<'_>,
    opts: &FilletOptions,
) -> Result<Vec<Occurrence<Fillet>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx, opts))
}

/// The leader-tip anchor: the surface point at the middle of the face's trimmed UV range.
pub fn fillet_anchor(part: &Part, face: usize) -> Option<V3> {
    let (u0, u1, v0, v1) = part.uv_bounds(face)?;
    Some(
        part.faces[face]
            .surface
            .value(0.5 * (u0 + u1), 0.5 * (v0 + v1)),
    )
}

fn occurrence(
    axis: usize,
    radius: f64,
    at: V3,
    turned: bool,
    face: usize,
    context: Vec<usize>,
) -> Occurrence<Fillet> {
    Occurrence {
        record: Fillet {
            axis: ['x', 'y', 'z'][axis],
            radius: geom::round3(radius),
            at: at.map(geom::round3),
            turned,
            side: "convex",
        },
        defining: vec![face],
        context,
    }
}

/// The run axis of a principal-direction surface, or `None` for a compound (oblique) one.
fn principal_axis(direction: V3) -> Option<usize> {
    let comp = direction.map(f64::abs);
    (comp[0].max(comp[1]).max(comp[2]) > AXIS_ALIGNED_COS).then(|| dominant_axis(comp))
}

pub fn discover(ctx: &Context<'_>, opts: &FilletOptions) -> Vec<Occurrence<Fillet>> {
    let part = ctx.part;
    let min_radius = opts.min_radius.unwrap_or(MIN_RADIUS);
    let max_radius = opts.max_radius_frac * ctx.bounds().max_extent();
    let mut out = Vec::new();
    if opts.include_cylindrical {
        for face in 0..part.faces.len() {
            if let Some(found) = prismatic(ctx, face, min_radius, max_radius) {
                out.push(found);
            }
        }
    }
    let external: BTreeMap<usize, &CylinderEvidence> = ctx
        .cylinders()
        .iter()
        .filter(|c| c.external)
        .map(|c| (c.face, c))
        .collect();
    for face in 0..part.faces.len() {
        if let Some(found) = turned(part, face, min_radius, max_radius, &external) {
            out.push(found);
        }
    }
    out.sort_by(|a, b| {
        let key = |o: &Occurrence<Fillet>| (o.record.axis, o.record.at);
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0).then_with(|| {
            // Python's tuple order: -0.0 equals 0.0 (so `total_cmp` would reorder ties).
            (0..3)
                .map(|i| {
                    ka.1[i]
                        .partial_cmp(&kb.1[i])
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    out
}

/// A partial cylinder rounding a convex edge between two axis-aligned walls.
fn prismatic(
    ctx: &Context<'_>,
    face: usize,
    min_radius: f64,
    max_radius: f64,
) -> Option<Occurrence<Fillet>> {
    let part = ctx.part;
    let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
        return None;
    };
    let (u0, u1, _, _) = part.uv_bounds(face)?;
    if u1 - u0 >= FILLET_MAX_EXTENT || radius < min_radius || radius > max_radius {
        return None;
    }
    let edge_axis = principal_axis(frame.z)?;
    let centre = part.face_bounds(face).centre();
    let planes = nearest_axis_aligned_planes(part, face, centre, edge_axis, true);
    if (0..3).any(|j| j != edge_axis && !planes.contains_key(&j)) {
        return None;
    }
    if !convex_bevel(ctx, centre, edge_axis, &planes) {
        return None;
    }
    let at = fillet_anchor(part, face)?;
    Some(occurrence(edge_axis, radius, at, false, face, vec![]))
}

/// A short external torus: a lathe-swept edge round, proved by a coaxial external cylinder.
fn turned(
    part: &Part,
    face: usize,
    min_radius: f64,
    max_radius: f64,
    external: &BTreeMap<usize, &CylinderEvidence>,
) -> Option<Occurrence<Fillet>> {
    let Surface::Torus { frame, minor, .. } = part.faces[face].surface else {
        return None;
    };
    if minor < min_radius || minor > max_radius {
        return None;
    }
    let (_, _, v0, v1) = part.uv_bounds(face)?;
    if (v1 - v0).abs() > TURNED_FILLET_MAX_EXTENT {
        return None;
    }
    let edge_axis = principal_axis(frame.z)?;
    let context = turned_context(part, face, &part.neighbours(face), external)?;
    let at = fillet_anchor(part, face)?;
    Some(occurrence(edge_axis, minor, at, true, face, context))
}

/// The material-side proof for a toroidal blend: the coaxial external cylinders it meets, plus
/// the transverse planes or spherical caps that make it a rounded edge rather than a bead.
/// `None` when the torus is not a turned edge fillet.
pub fn turned_context(
    part: &Part,
    torus: usize,
    neighbours: &[usize],
    external: &BTreeMap<usize, &CylinderEvidence>,
) -> Option<Vec<usize>> {
    let Surface::Torus { frame, .. } = part.faces[torus].surface else {
        return None;
    };
    let coaxial: Vec<&CylinderEvidence> = neighbours
        .iter()
        .filter_map(|n| external.get(n).copied())
        .filter(|c| {
            coaxial_axis_lines(
                frame.origin,
                frame.z,
                c.axis_point,
                c.direction,
                geom::length_tol(c.diameter, COAXIAL_FRAC),
            )
        })
        .collect();
    if coaxial.is_empty() {
        return None;
    }
    let mut transverse = Vec::new();
    let mut continuations = Vec::new();
    for &n in neighbours {
        match &part.faces[n].surface {
            Surface::Sphere { .. } => continuations.push(n),
            Surface::Plane { frame: plane }
                if geom::dot(plane.z, frame.z).abs() > AXIS_ALIGNED_COS =>
            {
                transverse.push(n)
            }
            _ => {}
        }
    }
    let mut diameters: Vec<f64> = coaxial.iter().map(|c| c.diameter).collect();
    diameters.sort_by(f64::total_cmp);
    diameters.dedup();
    let bridges_two_bands = diameters.len() >= 2;
    if transverse.is_empty() && !bridges_two_bands && continuations.is_empty() {
        return None;
    }
    let mut context: Vec<usize> = Vec::new();
    for f in coaxial
        .iter()
        .map(|c| c.face)
        .chain(transverse)
        .chain(continuations)
    {
        if !context.contains(&f) {
            context.push(f);
        }
    }
    Some(context)
}
