//! External edge fillet recognition (`quiddity.fillets`).
//!
//! A prismatic fillet is a partial cylinder rounding a convex edge between two tangent
//! walls; a turned fillet is a short external torus meeting a coaxial external cylinder.

use std::collections::BTreeMap;
use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::Context;
use super::bevel::{cylinder_support_tolerance, tangent_cylinder_corner};
use super::cylinders::CylinderEvidence;
use super::evidence::{self, EvidenceError, Occurrence};
use super::placed::{Grid, Placed};
use super::policy::AXIS_ALIGNED_COS;
use super::turned;
use crate::kernel::brep::Part;
use crate::kernel::classify::material_at;
use crate::kernel::geom::{self, SMOOTH_ARC_GAP, Surface, V3, dominant_axis};
use crate::kernel::py;

/// A minimum-evidence threshold, deliberately absolute (ADR 0008).
const MIN_RADIUS: f64 = 0.6;
/// Nearly half-turn rounds between nearly parallel walls are groove/slot full-round
/// bottoms, not edge fillets (#120). Keep walls at least 45° apart.
const FILLET_MAX_EXTENT: f64 = 3.0 * PI / 4.0;
/// A turned edge fillet is a quarter circle in section; a bead is a half or full one.
const TURNED_FILLET_MAX_EXTENT: f64 = PI / 2.0 * 1.05;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fillet {
    pub axis: FilletAxis,
    pub radius: f64,
    pub at: Placed<V3>,
    pub turned: bool,
    pub side: &'static str,
}

/// Principal runs keep their existing letter and serialization. Oblique runs carry a
/// full-precision unit direction, canonical with its dominant component positive.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FilletAxis {
    Principal(char),
    Direction(V3),
}

impl FilletAxis {
    fn order(self, other: Self) -> std::cmp::Ordering {
        use FilletAxis::{Direction, Principal};
        match (self, other) {
            (Principal(a), Principal(b)) => a.cmp(&b),
            (Principal(_), Direction(_)) => std::cmp::Ordering::Less,
            (Direction(_), Principal(_)) => std::cmp::Ordering::Greater,
            (Direction(a), Direction(b)) => py::tuple_order(&a, &b),
        }
    }
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

internal_entry! {
/// `recognise_fillets`: the records only.
pub fn recognise_fillets(part: &Part, opts: &FilletOptions) -> Vec<Fillet> {
    discover(&Context::new(part), opts)
        .into_iter()
        .map(|o| o.record)
        .collect()
}
}

internal_entry! {
/// The evidence path: each fillet with its defining face, published only if every one is
/// owned by a valid solid.
pub fn discover_verified(
    ctx: &Context<'_>,
    opts: &FilletOptions,
) -> Result<Vec<Occurrence<Fillet>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx, opts))
}
}

internal_entry! {
/// The leader-tip anchor: the surface point at the middle of the face's trimmed UV range.
pub fn fillet_anchor(part: &Part, face: usize) -> Option<V3> {
    let (u0, u1, v0, v1) = part.uv_bounds(face)?;
    Some(
        part.faces[face]
            .surface
            .value(0.5 * (u0 + u1), 0.5 * (v0 + v1)),
    )
}
}

fn occurrence(
    axis: FilletAxis,
    radius: f64,
    at: V3,
    turned: bool,
    face: usize,
    context: Vec<usize>,
) -> Occurrence<Fillet> {
    Occurrence {
        record: Fillet {
            axis,
            radius: py::round_to3(radius),
            at: Placed::new(at, Grid::Decimals(3)),
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

internal_entry! {
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
        ka.0.order(kb.0).then_with(|| py::tuple_order(&ka.1[..], &kb.1[..]))
    });
    out
}
}

/// A partial cylinder rounding a convex edge between two tangent walls, in any direction.
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
    if radius < min_radius || radius > max_radius {
        return None;
    }
    // Convexity belongs to the cylinder's oriented material side, independently of the
    // query axes. The corner probe alone cannot prove the surface's material side.
    if part.frame_points_outward(face) != Some(true) {
        return None;
    }
    let at = fillet_anchor(part, face)?;
    let corner = tangent_cylinder_corner(part, face, at, frame.z, frame.origin, radius)?;
    // The planes must be tangent to this cylinder, not unrelated nearby walls.
    if corner.normals.iter().any(|&n| {
        (geom::dot(geom::sub(frame.origin, corner.point), n).abs() - radius).abs()
            > cylinder_support_tolerance(part)
    }) || !sweep_matches_walls(u1 - u0, corner.normals)
    {
        return None;
    }
    let near = geom::add(
        corner.point,
        geom::scale(
            geom::sub(at, corner.point),
            super::policy::INTERIOR_PROBE_FRAC,
        ),
    );
    if material_at(ctx.classifier(), near) {
        return None;
    }
    let axis = run_axis(frame.z)?;
    Some(occurrence(axis, radius, at, false, face, vec![]))
}

/// Preserve the legacy principal-direction representation, including its alignment band.
fn run_axis(direction: V3) -> Option<FilletAxis> {
    Some(if let Some(axis) = principal_axis(direction) {
        FilletAxis::Principal(['x', 'y', 'z'][axis])
    } else {
        let direction = geom::unit(direction)?;
        let sign = if direction[dominant_axis(direction.map(f64::abs))] < 0.0 {
            -1.0
        } else {
            1.0
        };
        FilletAxis::Direction(geom::scale(direction, sign))
    })
}

/// The outward wall normals turn through the swept arc: PI minus the interior wall
/// angle. Use the existing parallel-direction angular tolerance, not a fixed 90° cap.
/// The 135° cap separates edge rounds from full-round groove/slot bottoms.
fn sweep_matches_walls(sweep: f64, normals: [V3; 2]) -> bool {
    if !(sweep > 0.0 && sweep <= FILLET_MAX_EXTENT) {
        return false;
    }
    let expected =
        geom::norm(geom::cross(normals[0], normals[1])).atan2(geom::dot(normals[0], normals[1]));
    1.0 - (sweep - expected).cos() <= SMOOTH_ARC_GAP
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
    Some(occurrence(
        FilletAxis::Principal(['x', 'y', 'z'][edge_axis]),
        minor,
        at,
        true,
        face,
        context,
    ))
}

internal_entry! {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_runs_keep_the_legacy_axis_letter_throughout_the_alignment_band() {
        for degrees in [0_f64, 2.94, 5., 8.] {
            let (s, c) = degrees.to_radians().sin_cos();
            for (axis, direction) in [('x', [c, s, 0.]), ('y', [0., c, s]), ('z', [s, 0., c])] {
                assert_eq!(run_axis(direction), Some(FilletAxis::Principal(axis)));
                assert_eq!(
                    run_axis(direction.map(|d| -d)),
                    Some(FilletAxis::Principal(axis))
                );
                assert_eq!(
                    serde_json::to_value(run_axis(direction).unwrap()).unwrap(),
                    axis.to_string()
                );
            }
        }
        let (s, c) = 9_f64.to_radians().sin_cos();
        assert!(matches!(
            run_axis([c, s, 0.]),
            Some(FilletAxis::Direction(_))
        ));
    }

    #[test]
    fn sweep_follows_acute_right_and_obtuse_walls() {
        let tolerance = (1.0 - SMOOTH_ARC_GAP).acos();
        for wall_degrees in [60_f64, 86., 90., 94., 120., 160.] {
            let sweep = PI - wall_degrees.to_radians();
            let walls = [[1., 0., 0.], [sweep.cos(), sweep.sin(), 0.]];
            assert!(sweep_matches_walls(sweep, walls));
            assert!(sweep_matches_walls(sweep + 0.5 * tolerance, walls));
            assert!(!sweep_matches_walls(sweep + 2.0 * tolerance, walls));
            assert!(!sweep_matches_walls(sweep - 2.0 * tolerance, walls));
            if wall_degrees != 90. {
                assert!(!sweep_matches_walls(PI - sweep, walls));
            }
            for invalid in [0., -sweep, PI, 2. * PI, f64::NAN] {
                assert!(!sweep_matches_walls(invalid, walls));
            }
        }
    }

    #[test]
    fn edge_rounds_stop_at_135_degrees() {
        for degrees in [45_f64, 40., 5.879068972873341] {
            let sweep = PI - degrees.to_radians();
            let walls = [[1., 0., 0.], [sweep.cos(), sweep.sin(), 0.]];
            assert_eq!(sweep_matches_walls(sweep, walls), degrees >= 45.);
        }
    }

    #[test]
    fn drafted_round_still_requires_tangent_walls_and_convex_material() {
        let part = crate::read_step_file(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/cast_bearing_unit.step.gz"),
        )
        .unwrap();
        let face = 24;
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            panic!()
        };
        let accepts = |p: &Part| prismatic(&Context::new(p), face, MIN_RADIUS, 100.).is_some();
        assert!(accepts(&part));
        // Perturb measured evidence in fresh Parts so caches cannot mask a failed proof.
        let mut faces = part.faces.clone();
        faces[face].surface = Surface::Cylinder {
            frame,
            radius: radius + 0.1,
        };
        assert!(!accepts(&Part::new(
            faces,
            part.edges.clone(),
            part.solids.clone()
        )));
        let mut faces = part.faces.clone();
        faces[face].reversed = !faces[face].reversed;
        assert!(!accepts(&Part::new(
            faces,
            part.edges.clone(),
            part.solids.clone()
        )));
        let mut faces = part.faces.clone();
        for n in part.neighbours(face) {
            if matches!(faces[n].surface, Surface::Plane { .. }) {
                faces[n].surface = Surface::Other {
                    kind: "missing support",
                };
            }
        }
        assert!(!accepts(&Part::new(
            faces,
            part.edges.clone(),
            part.solids.clone()
        )));
        assert!(prismatic(&Context::new(&part), face, radius + 0.1, 100.).is_none());
        assert!(prismatic(&Context::new(&part), face, MIN_RADIUS, radius - 0.1).is_none());
        let mut over_limit = 0;
        for f in 0..part.faces.len() {
            if matches!(part.faces[f].surface, Surface::Cylinder { .. })
                && let Some((u0, u1, _, _)) = part.uv_bounds(f)
                && u1 - u0 >= PI
            {
                over_limit += 1;
                assert!(prismatic(&Context::new(&part), f, MIN_RADIUS, 100.).is_none());
            }
        }
        assert!(
            over_limit > 0,
            "fixture must exercise the bore/extent guard"
        );
    }
}
