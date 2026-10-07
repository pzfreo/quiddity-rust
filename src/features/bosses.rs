//! Cylindrical boss recognition (`quiddity.bosses`): each external full-cylinder segment, its
//! free end found from the faces beyond its two axial ends.

use serde::{Deserialize, Serialize};

use super::Context;
use super::cylinders::{axis_point_at, full_cylinders, segments, z_then_cross};
use super::evidence::{self, EvidenceError, Occurrence};
use super::stacks::{End, classify_end};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, V3};
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

pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<BossRecord>> {
    let part = ctx.part;
    let (z, cross) = z_then_cross(ctx.cylinders());
    let external: Vec<_> = full_cylinders(&z)
        .into_iter()
        .chain(full_cylinders(&cross))
        .filter(|c| c.external)
        .collect();
    let mut out = Vec::new();
    for seg in segments(&external) {
        let (lo_state, lo_faces) = classify_end(part, &seg, seg.s_lo, false);
        let (hi_state, hi_faces) = classify_end(part, &seg, seg.s_hi, true);
        // The free end is the high one unless only the low end is open.
        let from_hi = !(lo_state == End::Open && hi_state != End::Open);
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
