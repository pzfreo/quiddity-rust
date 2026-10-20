//! Cylindrical boss recognition (`quiddity.bosses`): each external full-cylinder segment, its
//! free end found from the faces beyond its two axial ends.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use super::Context;
use super::cylinders::{axis_point_at, full_cylinders, segments, z_then_cross};
use super::evidence::{self, EvidenceError, Occurrence};
use super::stacks::{End, classify_end};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, V3};
use crate::kernel::py;

/// A boss: `axis` points from its base to its free end, `location` is the axis point at the free
/// end, `height` the axial length.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BossRecord {
    pub axis: V3,
    pub location: V3,
    pub diameter: f64,
    pub height: f64,
}

/// `recognise_bosses`.
pub fn recognise_bosses(part: &Part) -> Vec<BossRecord> {
    super::records(discover(&Context::new(part)))
}

/// The evidence path: each boss with its cylinder faces and the face at its free end.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<BossRecord>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
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
        .map(|f| Some((part.face_mass(f)?[0], part.face_centre(f)?)))
        .collect::<Option<Vec<_>>>()?;
    let area = py::fsum(weighted.iter().map(|(a, _)| *a));
    if !area.is_finite() || area <= 0. {
        return None;
    }
    let origin = [0, 1, 2].map(|i| py::fsum(weighted.iter().map(|(a, c)| a * c[i])) / area);
    origin.iter().all(|x| x.is_finite()).then_some(origin)
}

fn free_end(lo: End, hi: End, relative_body_origin: Option<f64>) -> Option<bool> {
    match (lo == End::Open, hi == End::Open) {
        (true, false) => Some(false),
        (false, true) => Some(true),
        _ => {
            // When end states tie, the free end points away from the body's boundary-area
            // centre. A zero axial moment supplies no base/free role and must refuse.
            let offset = relative_body_origin?;
            (offset.abs() > COORD_FLOOR).then_some(offset < 0.)
        }
    }
}

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
        let Some(from_hi) = free_end(lo_state, hi_state, relative_origin) else {
            continue;
        };
        let (free_state, free_faces) = if from_hi {
            (hi_state, hi_faces)
        } else {
            (lo_state, lo_faces)
        };
        let d = seg.direction;
        out.push(Occurrence {
            record: BossRecord {
                axis: if from_hi { d } else { geom::scale(d, -1.0) }.map(py::without_negative_zero),
                location: axis_point_at(&seg, if from_hi { seg.s_hi } else { seg.s_lo }),
                diameter: seg.diameter,
                height: py::round_to(seg.s_hi - seg.s_lo, 2),
            },
            defining: seg.faces.clone(),
            context: if free_state == End::Open {
                free_faces
            } else {
                vec![]
            },
        });
    }
    out
}
