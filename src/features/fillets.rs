//! External edge fillet recognition (`quiddity.fillets`).
//!
//! A prismatic fillet is a partial cylinder rounding a convex 90° edge between two axis-aligned
//! walls; a turned fillet is a short external torus meeting a coaxial external cylinder.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::Context;
use super::bevel::convex_bevel;
use super::cylinders::CylinderEvidence;
use super::evidence::{self, EvidenceError, Occurrence};
use super::planes::nearest_axis_aligned_planes;
use super::policy::AXIS_ALIGNED_COS;
use super::turned;
use crate::kernel::brep::Part;
use crate::kernel::geom::{Surface, V3, dominant_axis};
use crate::kernel::py;

/// A minimum-evidence threshold, deliberately absolute (ADR 0008).
const MIN_RADIUS: f64 = 0.6;
/// A convex edge fillet is a quarter turn; a bore keeps more than half a turn.
const FILLET_MAX_EXTENT: f64 = PI * 1.05;
/// A turned edge fillet is a quarter circle in section; a bead is a half or full one.
const TURNED_FILLET_MAX_EXTENT: f64 = PI / 2.0 * 1.05;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fillet {
    pub axis: char,
    pub radius: f64,
    pub at: V3,
    pub turned: bool,
    pub side: &'static str,
}

/// Options named as `recognise_fillets`' keyword arguments.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
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
            radius: py::round_to3(radius),
            at: at.map(py::round_to3),
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
    let external = turned::external_cylinders(ctx);
    for face in 0..part.faces.len() {
        if let Some(found) = turned(part, face, min_radius, max_radius, &external) {
            out.push(found);
        }
    }
    out.sort_by(|a, b| {
        let key = |o: &Occurrence<Fillet>| (o.record.axis, o.record.at);
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0).then_with(|| py::tuple_order(&ka.1, &kb.1))
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
    let coaxial = turned::coaxial_cylinders(neighbours, external, frame.origin, frame.z);
    if coaxial.is_empty() {
        return None;
    }
    let transverse = turned::transverse_planes(part, neighbours, frame.z);
    // A compound turned fillet can continue into its spherical corner cap.
    let continuations: Vec<usize> = neighbours
        .iter()
        .copied()
        .filter(|&n| matches!(part.faces[n].surface, Surface::Sphere { .. }))
        .collect();
    if transverse.is_empty() && !turned::bridges_two_bands(&coaxial) && continuations.is_empty() {
        return None; // a toroidal bead meeting one band, not a rounded edge
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
