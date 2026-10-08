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

/// `recognise_slots`.
pub fn recognise_slots(part: &Part) -> Vec<Slot> {
    records(discover(&Context::new(part)))
}

fn occurrence(proposal: Proposal<Slot>) -> Occurrence<Slot> {
    Occurrence {
        defining: proposal.defining().into_iter().collect(),
        context: Vec::new(),
        record: proposal.record,
    }
}

/// Every slot with its walls and caps, in (width, centre) order. Competing caps at one end do
/// not refuse here: the record is extended from the first matching cap cluster at each end and
/// keeps only those clusters, as `recognise_slots` does (`strict_cap_ambiguity=False`).
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<Slot>> {
    body_scoped(ctx, |ctx, solid| slot_proposals(ctx, solid, false))
        .unwrap_or_else(|_| unreachable!("only a strict scan refuses"))
        .into_iter()
        .map(occurrence)
        .collect()
}

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
