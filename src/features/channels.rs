//! Full-span floored channels (`quiddity._recess_features.recognise_channels` and
//! `_discover_channels`): two facing walls with a floor at one depth end whose shared run
//! reaches both ends of its solid's envelope. Defined by the two side walls; the floor is
//! consulted.

use std::cmp::Ordering;

use super::Context;
use super::evidence::{EvidenceError, Occurrence, common_valid_solid};
use super::recess_core::{ChannelProposal, channel_proposals};
use super::records;
use crate::kernel::brep::Part;
use crate::kernel::py;

pub use super::recess_records::Channel;

/// `recognise_channels`.
pub fn recognise_channels(part: &Part) -> Vec<Channel> {
    records(discover(&Context::new(part)))
}

/// Geometry-only order (`_channel_sort_key`).
fn sort_key(a: &Channel, b: &Channel) -> Ordering {
    a.long_axis
        .cmp(&b.long_axis)
        .then(a.width_axis.cmp(&b.width_axis))
        .then_with(|| {
            py::tuple_order(
                &[a.lo, a.hi, a.w_center, a.width, a.d_lo, a.d_hi],
                &[b.lo, b.hi, b.w_center, b.width, b.d_lo, b.d_hi],
            )
        })
        .then(a.open_sign.cmp(&b.open_sign))
}

fn occurrence(proposal: &ChannelProposal) -> Occurrence<Channel> {
    Occurrence {
        record: proposal.record.clone(),
        defining: vec![proposal.low_wall, proposal.high_wall],
        context: proposal.floor.iter().copied().collect(),
    }
}

/// Each solid's channels, one per distinct record (its first distinct wall pair, the last
/// proposal found for it), keyed to a valid solid's unambiguous body; with *strict*, a record
/// found on two wall pairs refuses.
fn retained(ctx: &Context<'_>, strict: bool) -> Result<Vec<ChannelProposal>, EvidenceError> {
    let keys = ctx.body_keys(true);
    let mut out: Vec<ChannelProposal> = Vec::new();
    for (solid, key) in keys.into_iter().enumerate() {
        let mut by_record: Vec<(Channel, Vec<ChannelProposal>)> = Vec::new();
        for proposal in channel_proposals(ctx, solid) {
            match by_record.iter_mut().find(|(r, _)| *r == proposal.record) {
                Some((_, found)) => found.push(proposal),
                None => by_record.push((proposal.record.clone(), vec![proposal])),
            }
        }
        by_record.sort_by(|a, b| sort_key(&a.0, &b.0));
        for (_, proposals) in by_record {
            // Python's dict of wall pairs: first-seen pairs, each holding its last proposal.
            let mut unique: Vec<ChannelProposal> = Vec::new();
            for proposal in proposals {
                let pair = (proposal.low_wall, proposal.high_wall);
                match unique
                    .iter_mut()
                    .find(|u| (u.low_wall, u.high_wall) == pair)
                {
                    Some(u) => *u = proposal,
                    None => unique.push(proposal),
                }
            }
            if strict && unique.len() != 1 {
                return Err(EvidenceError::SharedEvidence);
            }
            let mut kept = unique.swap_remove(0);
            kept.record.body_key = key.clone();
            out.push(kept);
        }
    }
    out.sort_by(|a, b| sort_key(&a.record, &b.record));
    Ok(out)
}

/// Every channel with its two side walls and floor, in record order.
pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<Channel>> {
    retained(ctx, false)
        .unwrap_or_else(|_| unreachable!("only a strict read refuses"))
        .iter()
        .map(occurrence)
        .collect()
}

/// The evidence path (`_discover_channels` with a writer): one record on two wall pairs, side
/// walls that are one face, a floor missing or among the walls, or faces without one valid
/// solid refuse.
pub fn discover_verified(ctx: &Context<'_>) -> Result<Vec<Occurrence<Channel>>, EvidenceError> {
    let mut out = Vec::new();
    for proposal in retained(ctx, true)? {
        let walls = [proposal.low_wall, proposal.high_wall];
        if walls[0] == walls[1]
            || proposal.floor.is_empty()
            || walls.iter().any(|w| proposal.floor.contains(w))
        {
            return Err(EvidenceError::SharedEvidence);
        }
        let found = occurrence(&proposal);
        let members: Vec<usize> = found
            .defining
            .iter()
            .chain(&found.context)
            .copied()
            .collect();
        common_valid_solid(ctx.part, &members).ok_or(EvidenceError::NoValidSolid)?;
        out.push(found);
    }
    Ok(out)
}
