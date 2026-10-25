//! Enclosed through-slots (`quiddity._recess_features.recognise_slots` and `_discover_slots`):
//! two facing walls with no floor at either depth end, or a stubby obround's two end caps,
//! recognised per solid.

use super::Context;
use super::evidence::{EvidenceError, Occurrence, common_valid_solid};
use super::recess_core::slot_proposals;
use super::recess_reduce::{Proposal, body_scoped};
use super::records;
use crate::kernel::brep::Part;

pub use super::recess_records::Slot;

internal_entry! {
/// `recognise_slots`.
pub fn recognise_slots(part: &Part) -> Vec<Slot> {
    records(discover(&Context::new(part)))
}
}

fn occurrence(proposal: Proposal<Slot>) -> Occurrence<Slot> {
    Occurrence {
        defining: proposal.defining().into_iter().collect(),
        context: Vec::new(),
        record: proposal.record.published(),
    }
}

/// Complete an already recognised rectangular slot's face claim. This does not
/// propose, measure, reduce or accept a slot: both end walls must fit its raw
/// bounds and meet its defining walls through the proved-radius corner cylinders.
pub(crate) fn claim_rectangular_end_walls(part: &Part, slot: &Slot, defining: &mut Vec<usize>) {
    use super::recess_records::{axis_index, third_axis};
    use crate::kernel::geom::{Curve, Surface};
    let Some(radius) = slot.corner_radius.filter(|r| *r > 0.) else {
        return;
    };
    if slot.end_radius.is_some() {
        return;
    }
    let (w, l) = (axis_index(slot.width_axis), axis_index(slot.long_axis));
    let d = third_axis(w, l);
    let Some(solid) = defining.first().and_then(|&f| part.faces[f].solid) else {
        return;
    };
    if !part.solid_is_geometrically_valid(solid) {
        return;
    }
    let tol = 1e-7 * part.solid_bounds(solid).max_extent().max(1.);
    let walls: Vec<_> = defining
        .iter()
        .copied()
        .filter(|&f| {
            matches!(part.faces[f].surface, Surface::Plane { .. })
                && part
                    .face_normal(f, 0., 0.)
                    .is_some_and(|n| n[w].abs() >= 1. - 1e-9)
        })
        .collect();
    let wlo = walls
        .iter()
        .map(|&f| part.face_bounds(f).min[w])
        .fold(f64::INFINITY, f64::min);
    let whi = walls
        .iter()
        .map(|&f| part.face_bounds(f).max[w])
        .fold(f64::NEG_INFINITY, f64::max);
    if (super::policy::round_measurement(whi - wlo, 2) - slot.width).abs() > tol
        || ((wlo + whi) / 2. - slot.w_center.raw()).abs() > tol
    {
        return;
    }
    let mut ends = Vec::new();
    for (at, sign) in [(*slot.lo.raw(), 1.), (*slot.hi.raw(), -1.)] {
        let candidates: Vec<_> = part.solids[solid]
            .faces
            .iter()
            .copied()
            .filter(|&f| {
                if !matches!(part.faces[f].surface, Surface::Plane { .. }) {
                    return false;
                }
                let Some(n) = part.face_normal(f, 0., 0.) else {
                    return false;
                };
                let b = part.face_bounds(f);
                if (n[l] - sign).abs() > 1e-9
                    || (b.min[l] - at).abs() > tol
                    || (b.max[l] - at).abs() > tol
                    || (b.min[d] - slot.d_lo.raw()).abs() > tol
                    || (b.max[d] - slot.d_hi.raw()).abs() > tol
                {
                    return false;
                }
                // A matching plane elsewhere in the solid is not an end wall.
                let mut radii = Vec::new();
                for (side, low) in [(wlo, true), (whi, false)] {
                    let corner = part.neighbours(f).iter().copied().find_map(|g| {
                        let Surface::Cylinder { frame, radius: r } = part.faces[g].surface else {
                            return None;
                        };
                        let inset = if low { side + r } else { side - r };
                        (r > 0.
                            && (super::policy::round_measurement(r, 2) - radius).abs() <= tol
                            && frame.z[d].abs() >= 1. - 1e-9
                            && (frame.origin[w] - inset).abs() <= tol
                            && (frame.origin[l] - (at + sign * r)).abs() <= tol
                            && ((if low { b.min[w] } else { b.max[w] }) - inset).abs() <= tol
                            && part.frame_points_outward(g) == Some(false)
                            && part
                                .shared_edges(f, g)
                                .iter()
                                .any(|&e| matches!(part.edges[e].curve, Curve::Line { .. }))
                            && part.neighbours(g).iter().any(|h| {
                                walls.contains(h) && {
                                    let hb = part.face_bounds(*h);
                                    (hb.min[w] - side).abs() <= tol
                                        && (hb.max[w] - side).abs() <= tol
                                }
                            }))
                        .then_some(r)
                    });
                    let Some(r) = corner else {
                        return false;
                    };
                    radii.push(r);
                }
                (radii[0] - radii[1]).abs() <= tol
            })
            .collect();
        if candidates.len() != 1 {
            return;
        }
        ends.push(candidates[0]);
    }
    defining.extend(ends);
    defining.sort_unstable();
    defining.dedup();
}

internal_entry! {
/// Every slot with its faces, in (width, centre) order. Competing cap clusters skip the
/// ambiguous occurrence on this record path; the evidence path returns `CompetingCaps`.
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<Slot>> {
    body_scoped(ctx, |ctx, solid| slot_proposals(ctx, solid, false))
        .unwrap_or_else(|_| unreachable!("only a strict scan refuses"))
        .into_iter()
        .map(occurrence)
        .collect()
}
}

internal_entry! {
/// The evidence path (`_discover_slots` with a writer): competing cap clusters refuse, as do
/// faces without one valid solid and one record given two face sets on one solid; a record
/// found twice from the same faces is published once.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<Slot>>, EvidenceError> {
    let proposals = body_scoped(ctx, |ctx, solid| slot_proposals(ctx, solid, true))
        .map_err(|_| EvidenceError::CompetingCaps)?;
    let mut pending: Vec<(Occurrence<Slot>, usize)> = Vec::new();
    for proposal in proposals {
        let found = occurrence(proposal);
        if found.defining.is_empty() {
            return Err(EvidenceError::NoValidSolid);
        }
        let solid =
            common_valid_solid(ctx.part, &found.defining).ok_or(EvidenceError::NoValidSolid)?;
        let mut duplicate = false;
        for (other, other_solid) in &pending {
            if found.record == other.record && solid == *other_solid {
                if found.defining != other.defining {
                    return Err(EvidenceError::SharedEvidence);
                }
                duplicate = true;
                break;
            }
            if solid != *other_solid && found.defining.iter().any(|f| other.defining.contains(f)) {
                return Err(EvidenceError::SharedEvidence);
            }
        }
        if !duplicate {
            pending.push((found, solid));
        }
    }
    Ok(pending.into_iter().map(|(o, _)| o).collect())
}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangular_end_claim_requires_radius_footprint_and_both_wall_connections() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/ap242/nist/nist_ctc_01_asme1_ap242-e1.stp.gz");
        let part = crate::read_step_file(&path).unwrap();
        let found = discover(&Context::new(&part));
        let slot = found
            .iter()
            .find(|s| s.record.corner_radius.is_some())
            .unwrap();
        let mut faces = slot.defining.clone();
        claim_rectangular_end_walls(&part, &slot.record, &mut faces);
        assert_eq!(faces, [82, 84, 86, 88]);
        for kind in 0..4 {
            let mut record = slot.record.clone();
            match kind {
                0 => record.corner_radius = None,
                1 => record.end_radius = Some(10.),
                2 => record.corner_radius = Some(9.),
                _ => record.width += 1.,
            }
            let mut faces = slot.defining.clone();
            claim_rectangular_end_walls(&part, &record, &mut faces);
            assert_eq!(faces, slot.defining);
        }
        let mut foreign = found
            .iter()
            .find(|s| s.record.end_radius.is_some())
            .unwrap()
            .defining
            .clone();
        let before = foreign.clone();
        claim_rectangular_end_walls(&part, &slot.record, &mut foreign);
        assert_eq!(foreign, before);
        let mut inward = Part::new(part.faces.clone(), part.edges.clone(), part.solids.clone());
        inward.faces[82].reversed = !inward.faces[82].reversed;
        let mut faces = slot.defining.clone();
        claim_rectangular_end_walls(&inward, &slot.record, &mut faces);
        assert_eq!(faces, slot.defining);
    }
}
