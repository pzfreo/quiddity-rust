//! Bounded blind recesses (`quiddity._recess_features.recognise_pockets` and
//! `_discover_pockets`): the through-slot's wall pairs kept where a floor caps one depth end,
//! corner notches, and stubby obround pockets from their caps, recognised per solid.
//!
//! What defines a pocket depends on how it was found: from opposed walls the two walls (the
//! floor only had to exist), from a corner notch the floor and both walls, from obround ends the
//! cap clusters. The floor and any proved complete inner region are consulted context.

use std::collections::BTreeSet;

use super::Context;
use super::evidence::{EvidenceError, Occurrence, common_valid_solid};
use super::recess_core::pocket_proposals;
use super::recess_reduce::{Proposal, body_scoped};
use super::records;
use crate::kernel::brep::Part;

pub use super::recess_records::Pocket;

/// `recognise_pockets`.
pub fn recognise_pockets(part: &Part) -> Vec<Pocket> {
    records(discover(&Context::new(part)))
}

/// The occurrence and every face it consulted (`members`: defining, floors, inner region).
fn occurrence(proposal: Proposal<Pocket>) -> (Occurrence<Pocket>, Vec<usize>) {
    let defining = proposal.defining();
    let members: BTreeSet<usize> = defining
        .iter()
        .chain(&proposal.floors)
        .chain(&proposal.constituent)
        .copied()
        .collect();
    let found = Occurrence {
        context: members.difference(&defining).copied().collect(),
        defining: defining.into_iter().collect(),
        record: proposal.record,
    };
    (found, members.into_iter().collect())
}

/// Every pocket with its faces, in (width, centre) order. Competing caps at one end do not
/// refuse here: the record is extended from the first matching cap cluster at each end and
/// keeps only those clusters, as `recognise_pockets` does (`strict_cap_ambiguity=False`).
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<Pocket>> {
    body_scoped(ctx, |ctx, solid| pocket_proposals(ctx, solid, false))
        .unwrap_or_else(|_| unreachable!("only a strict scan refuses"))
        .into_iter()
        .map(|p| occurrence(p).0)
        .collect()
}

/// The evidence path (`_discover_pockets` with a writer): competing cap clusters refuse, as do
/// a wall- or cap-found pocket without floor faces, faces without one valid solid, and one
/// record given two face sets on one solid; a record found twice from the same faces is
/// published once.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<Pocket>>, EvidenceError> {
    let proposals = body_scoped(ctx, |ctx, solid| pocket_proposals(ctx, solid, true))
        .map_err(|_| EvidenceError::CompetingCaps)?;
    let mut pending: Vec<(Occurrence<Pocket>, Vec<usize>, usize)> = Vec::new();
    for proposal in proposals {
        if !proposal.record.edge_anchored && proposal.floors.is_empty() {
            return Err(EvidenceError::MissingFloor);
        }
        let (found, members) = occurrence(proposal);
        if found.defining.is_empty() {
            return Err(EvidenceError::NoValidSolid);
        }
        let solid = common_valid_solid(ctx.part, &members).ok_or(EvidenceError::NoValidSolid)?;
        match pending
            .iter()
            .find(|(other, _, other_solid)| other.record == found.record && *other_solid == solid)
        {
            Some((other, other_members, _)) => {
                if other.defining != found.defining || *other_members != members {
                    return Err(EvidenceError::SharedEvidence);
                }
            }
            None => pending.push((found, members, solid)),
        }
    }
    Ok(pending.into_iter().map(|(o, _, _)| o).collect())
}
