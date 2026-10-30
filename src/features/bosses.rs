//! Cylindrical boss recognition (`quiddity.bosses`): each external full-cylinder segment, its
//! free end found from the faces beyond its two axial ends.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::Context;
use super::cylinders::{
    Segment, axis_point_at, coaxial_axis_lines, full_cylinders, segments, z_then_cross,
};
use super::evidence::{self, EvidenceError, Occurrence};
use super::stacks::{End, classify_end};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, Surface, V3};
use crate::kernel::py;

/// A boss: `axis` points from its base to its free end, `location` is the axis point at the free
/// end, `height` the axial length including convex end chamfers and rounds, and `location`
/// includes the free end's treatment too. When geometry leaves the two ends interchangeable,
/// axis/location represent an end gauge, not an asserted material base/free direction.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BossRecord {
    pub axis: V3,
    pub location: V3,
    pub diameter: f64,
    pub height: f64,
}

internal_entry! {
/// `recognise_bosses`.
pub fn recognise_bosses(part: &Part) -> Vec<BossRecord> {
    super::records(discover(&Context::new(part)))
}
}

internal_entry! {
/// The evidence path: each boss with its cylinder faces and the face at its free end.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<BossRecord>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}
}

/// An intrinsic body-relative origin, using the boundary-area distribution already cached by
/// the kernel. Missing surface measurements cannot establish a direction.
fn body_origin(part: &Part, solid: usize) -> Option<V3> {
    let faces = part
        .solids
        .get(solid)
        .map_or_else(|| (0..part.faces.len()).collect(), |s| s.faces.clone());
    let weighted = faces
        .into_iter()
        .map(|f| {
            let moments = part.face_moments(f)?;
            Some((moments.area, moments.centroid))
        })
        .collect::<Option<Vec<_>>>()?;
    let area = py::fsum(weighted.iter().map(|(a, _)| *a));
    if !area.is_finite() || area <= 0. {
        return None;
    }
    let origin = [0, 1, 2].map(|i| py::fsum(weighted.iter().map(|(a, c)| a * c[i])) / area);
    origin.iter().all(|x| x.is_finite()).then_some(origin)
}

fn free_end(lo: End, hi: End, relative_body_origin: Option<f64>) -> (bool, bool) {
    match (lo == End::Open, hi == End::Open) {
        (true, false) => (false, false),
        (false, true) => (true, false),
        _ => {
            // When end states tie, the free end points away from the body's boundary-area
            // centre. With no established direction, retain the OD using its existing
            // undirected cylinder-axis representative. Both ends are presentation-equivalent.
            match relative_body_origin.filter(|offset| offset.abs() > COORD_FLOOR) {
                Some(offset) => (offset < 0., false),
                None => (true, true),
            }
        }
    }
}

/// Extend only through proved coaxial edge treatments adjoining an axial end. A shoulder
/// or a neighbouring cylindrical band cannot lengthen this boss. Keep consulted faces in
/// context: the cylinder still defines the boss, and the treatment can define its own feature.
fn treated_extent(ctx: &Context<'_>, seg: &Segment) -> (f64, f64, Vec<usize>) {
    let part = ctx.part;
    let neighbours: Vec<_> = seg.faces.iter().flat_map(|&f| part.neighbours(f)).collect();
    let coaxial = |face: usize| match part.faces[face].surface {
        Surface::Cone { frame, .. } | Surface::Torus { frame, .. } => coaxial_axis_lines(
            frame.origin,
            frame.z,
            seg.axis_point,
            seg.direction,
            super::policy::length_tol(seg.diameter, 1e-4),
        ),
        _ => false,
    };
    let (mut lo, mut hi) = (seg.s_lo, seg.s_hi);
    let mut context = Vec::new();
    // A spherical end round is tangent to the cylinder only at its equator: its centre
    // lies on this axis at an end, and its radius is the boss radius. A ball/bead of a
    // different radius or a side-contact sphere supplies no axial end evidence.
    for &face in &neighbours {
        if let Surface::Sphere { frame, radius } = part.faces[face].surface {
            let s = geom::dot(frame.origin, seg.direction);
            if (2. * radius - seg.diameter).abs() <= COORD_FLOOR
                && geom::dist(frame.origin, axis_point_at(seg, s)) <= COORD_FLOOR
                && ((s - seg.s_lo).abs() <= COORD_FLOOR || (s - seg.s_hi).abs() <= COORD_FLOOR)
                && part.frame_points_outward(face) == Some(true)
            {
                let (a, b) = part.extent_along(&[face], seg.direction);
                if a >= seg.s_hi - COORD_FLOOR || b <= seg.s_lo + COORD_FLOOR {
                    lo = lo.min(a);
                    hi = hi.max(b);
                    context.push(face);
                }
            }
        }
    }
    if !neighbours.iter().any(|&f| coaxial(f)) {
        return (lo, hi, context);
    }
    for treatment in ctx.edge_treatments() {
        for &face in &treatment.defining {
            if !neighbours.contains(&face) || !coaxial(face) {
                continue;
            }
            let (a, b) = part.extent_along(&treatment.defining, seg.direction);
            if (a >= seg.s_hi - COORD_FLOOR && b > seg.s_hi + COORD_FLOOR)
                || (b <= seg.s_lo + COORD_FLOOR && a < seg.s_lo - COORD_FLOOR)
            {
                lo = lo.min(a);
                hi = hi.max(b);
                context.extend(&treatment.defining);
                context.extend(&treatment.context);
            }
        }
    }
    context.sort_unstable();
    context.dedup();
    (lo, hi, context)
}

/// Fingerprint placement uses the measured midpoint, before two-decimal height publication.
/// In particular, interchangeable end gauges must yield exactly the same physical midpoint.
pub(crate) fn midpoint(ctx: &Context<'_>, boss: &BossRecord, faces: &[usize]) -> V3 {
    let (s_lo, s_hi) = ctx.part.extent_along(faces, boss.axis);
    let seg = Segment {
        solid: 0,
        axis: 0,
        external: true,
        diameter: boss.diameter,
        direction: boss.axis,
        axis_point: boss.location,
        s_lo,
        s_hi,
        faces: faces.to_vec(),
    };
    let (lo, hi, _) = treated_extent(ctx, &seg);
    axis_point_at(&seg, (lo + hi) / 2.)
}

internal_entry! {
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<BossRecord>> {
    let part = ctx.part;
    let (z, cross) = z_then_cross(ctx.cylinders());
    let external: Vec<_> = full_cylinders(&z)
        .into_iter()
        .chain(full_cylinders(&cross))
        .filter(|c| c.external)
        .collect();
    let mut out = Vec::new();
    let mut origins = BTreeMap::new();
    for seg in segments(&external) {
        let (lo_state, lo_faces) = classify_end(part, &seg, seg.s_lo, false);
        let (hi_state, hi_faces) = classify_end(part, &seg, seg.s_hi, true);
        let relative_origin = if (lo_state == End::Open) == (hi_state == End::Open) {
            origins
                .entry(seg.solid)
                .or_insert_with(|| body_origin(part, seg.solid))
                .map(|origin| {
                    let middle = axis_point_at(&seg, (seg.s_lo + seg.s_hi) / 2.);
                    geom::dot(geom::sub(origin, middle), seg.direction)
                })
        } else {
            None
        };
        let (from_hi, presentation) = free_end(lo_state, hi_state, relative_origin);
        let gauge_faces = if presentation {
            let mut faces: Vec<_> = lo_faces.iter().chain(&hi_faces).copied().collect();
            faces.sort_unstable();
            faces.dedup();
            faces
        } else {
            vec![]
        };
        let (free_state, free_faces) = if from_hi {
            (hi_state, hi_faces)
        } else {
            (lo_state, lo_faces)
        };
        let d = seg.direction;
        let (lo, hi, mut treatment_faces) = treated_extent(ctx, &seg);
        treatment_faces.extend(if presentation && free_state == End::Open {
            gauge_faces
        } else if free_state == End::Open {
            free_faces
        } else {
            vec![]
        });
        treatment_faces.sort_unstable();
        treatment_faces.dedup();
        out.push(Occurrence {
            record: BossRecord {
                axis: if from_hi { d } else { geom::scale(d, -1.0) }.map(py::without_negative_zero),
                location: axis_point_at(&seg, if from_hi { hi } else { lo }),
                diameter: seg.diameter,
                height: py::round_to(hi - lo, 2),
            },
            defining: seg.faces.clone(),
            context: treatment_faces,
        });
    }
    out
}
}
