//! Many recess candidates into the features they describe (`quiddity._recess_reduce`): a scan
//! proposes one void more than once (through its other wall pair, split into arms by a channel
//! crossing it, or again from its end caps). `merge_proposals` folds co-located ones,
//! `collapse_collinear_proposals` rejoins the arms a crossing split, and each proposal carries
//! the faces it was built from through both.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::Context;
use super::policy::round_measurement;
use super::recess_faces::MERGE_TOL;
use super::recess_records::{Recess, Slot};
use super::volume_probe::{Spans, prism_material_fraction};
use crate::kernel::py;

/// One recess occurrence and the faces carried through reduction (`_RecessProposal`): planar
/// walls, cylindrical cap groups and floors apart, since each family decides which roles define
/// it; `constituent` is a pocket's complete inner region where one is proved.
#[derive(Clone, Debug)]
pub struct Proposal<R> {
    pub record: R,
    pub planar: BTreeSet<usize>,
    pub caps: Vec<BTreeSet<usize>>,
    pub floors: BTreeSet<usize>,
    pub constituent: BTreeSet<usize>,
}

impl<R> Proposal<R> {
    pub fn new(record: R) -> Self {
        Proposal {
            record,
            planar: BTreeSet::new(),
            caps: Vec::new(),
            floors: BTreeSet::new(),
            constituent: BTreeSet::new(),
        }
    }

    /// The same faces under another record (`_replace_proposal`).
    pub fn replaced(self, record: R) -> Self {
        Proposal { record, ..self }
    }

    /// The faces that define the occurrence: planar walls and every cap group.
    pub fn defining(&self) -> BTreeSet<usize> {
        self.planar
            .iter()
            .chain(self.caps.iter().flatten())
            .copied()
            .collect()
    }
}

/// *record* with the faces of every member (`_combine_proposals`): cap groups in first-seen
/// order, each once.
fn combine<R: Clone>(record: R, members: &[&Proposal<R>]) -> Proposal<R> {
    let mut out = Proposal::new(record);
    for m in members {
        out.planar.extend(&m.planar);
        out.floors.extend(&m.floors);
        out.constituent.extend(&m.constituent);
        for group in &m.caps {
            if !out.caps.contains(group) {
                out.caps.push(group.clone());
            }
        }
    }
    out
}

/// The order recess records are reduced and published in: width, then region centre.
pub fn width_then_centre<R: Recess>(a: &R, b: &R) -> Ordering {
    // Raw positions must survive reduction, but sub-coordinate-floor noise must not decide
    // which equal-width record comes first (and hence its section-recess index). Quantise
    // only this ordering key; measured spans and the records themselves remain untouched.
    let centre_key = |r: &R| r.region_center().map(|c| py::round_to(c, 6));
    py::order(a.width(), b.width()).then_with(|| py::tuple_order(&centre_key(a), &centre_key(b)))
}

/// Co-located candidates as one, keeping the first of each in (width, centre) order: the
/// narrower width is the true across-flats (`_merge_proposals`).
pub fn merge_proposals<R: Recess>(mut proposals: Vec<Proposal<R>>) -> Vec<Proposal<R>> {
    // Equal wall separations must choose the same interpretation after a rotation.
    // Original face identities survive placement, unlike axis letters and world centres.
    proposals.sort_by(|a, b| {
        if a.record.width() == b.record.width() {
            a.planar
                .cmp(&b.planar)
                .then_with(|| a.floors.cmp(&b.floors))
        } else {
            width_then_centre(&a.record, &b.record)
        }
    });
    let mut kept: Vec<Proposal<R>> = Vec::new();
    for proposal in proposals {
        let centre = proposal.record.region_center();
        match kept
            .iter()
            .position(|k| py::dist(&centre, &k.record.region_center()) <= MERGE_TOL)
        {
            Some(i) => kept[i] = combine(kept[i].record.clone(), &[&kept[i], &proposal]),
            None => kept.push(proposal),
        }
    }
    kept
}

/// The gap between two collinear arms of one channel (same width axis, centreline, width and
/// depth, disjoint along the run), or `None` (`_same_channel_line`).
fn same_channel_line(a: &Slot, b: &Slot) -> Option<(f64, f64)> {
    if a.width_axis != b.width_axis || a.long_axis != b.long_axis {
        return None;
    }
    if (a.w_center - b.w_center).abs() > MERGE_TOL || (a.width - b.width).abs() > MERGE_TOL {
        return None;
    }
    if (a.d_lo - b.d_lo).abs() > MERGE_TOL || (a.d_hi - b.d_hi).abs() > MERGE_TOL {
        return None;
    }
    let gap = if a.hi <= b.lo {
        (a.hi, b.lo)
    } else if b.hi <= a.lo {
        (b.hi, a.lo)
    } else {
        return None;
    };
    (gap.1 - gap.0 > 0.0).then_some(gap)
}

const VOID_INSET: f64 = 0.1;
const VOID_VOL_FRAC: f64 = 0.01;

/// Whether the gap between collinear arms is near-empty, as a crossing channel leaves it
/// (`_gap_is_void`). A probe that cannot answer proves nothing, so the arms stay apart.
fn gap_is_void(ctx: &Context<'_>, solid: usize, gap: (f64, f64), arm: &Slot) -> bool {
    let mut spans: Spans = [(0.0, 0.0); 3];
    spans[arm.long_axis()] = gap;
    spans[arm.width_axis()] = (
        arm.w_center - arm.width / 2.0,
        arm.w_center + arm.width / 2.0,
    );
    spans[arm.depth_axis()] = (arm.d_lo, arm.d_hi);
    prism_material_fraction(ctx, solid, &spans, VOID_INSET).is_some_and(|f| f <= VOID_VOL_FRAC)
}

/// Slot arms split by a crossing channel, rejoined into whole channels where the gap between
/// them is void; arms parted by material stay separate (`_collapse_collinear_proposals`).
pub fn collapse_collinear_proposals(
    ctx: &Context<'_>,
    solid: usize,
    proposals: Vec<Proposal<Slot>>,
) -> Vec<Proposal<Slot>> {
    let n = proposals.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for left in 0..n {
        for right in left + 1..n {
            let (a, b) = (&proposals[left].record, &proposals[right].record);
            if let Some(gap) = same_channel_line(a, b)
                && gap_is_void(ctx, solid, gap, a)
            {
                let (l, r) = (find(&mut parent, left), find(&mut parent, right));
                parent[l] = r;
            }
        }
    }
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        match groups.iter_mut().find(|(r, _)| *r == root) {
            Some((_, members)) => members.push(i),
            None => groups.push((root, vec![i])),
        }
    }
    let mut out: Vec<Proposal<Slot>> = groups
        .into_iter()
        .map(|(_, members)| {
            if let [only] = members[..] {
                return proposals[only].clone();
            }
            let base = &proposals[members[0]].record;
            let lo = members
                .iter()
                .map(|&m| proposals[m].record.lo)
                .fold(f64::INFINITY, f64::min);
            let hi = members
                .iter()
                .map(|&m| proposals[m].record.hi)
                .fold(f64::NEG_INFINITY, f64::max);
            let spanned = Slot {
                width_axis: base.width_axis,
                long_axis: base.long_axis,
                width: base.width,
                length: round_measurement(hi - lo, 2),
                w_center: base.w_center,
                lo,
                hi,
                d_lo: base.d_lo,
                d_hi: base.d_hi,
                body_key: Some(Vec::new()),
                end_radius: None,
                corner_radius: None,
            };
            let members: Vec<&Proposal<Slot>> = members.iter().map(|&m| &proposals[m]).collect();
            combine(spanned, &members)
        })
        .collect();
    out.sort_by(|a, b| width_then_centre(&a.record, &b.record));
    out
}

/// Every solid's proposals, each record given its solid's body key (ambiguous keys `None`),
/// in (width, centre) order (`_body_scoped_proposals` and the sort its callers apply). A
/// compound is scanned per solid, so faces of separate bodies never combine into one recess.
pub fn body_scoped<R: Recess, E>(
    ctx: &Context<'_>,
    scan: impl Fn(&Context<'_>, usize) -> Result<Vec<Proposal<R>>, E>,
) -> Result<Vec<Proposal<R>>, E> {
    let keys = ctx.body_keys(false);
    let mut out = Vec::new();
    for (solid, key) in keys.into_iter().enumerate() {
        for mut proposal in scan(ctx, solid)? {
            proposal.record.set_body_key(key.clone());
            out.push(proposal);
        }
    }
    out.sort_by(|a, b| width_then_centre(&a.record, &b.record));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_width_recess_order_ignores_roundoff_in_a_tied_coordinate() {
        let slot = |x, y| Slot {
            width_axis: 'x',
            long_axis: 'y',
            width: 2.0,
            length: 4.0,
            w_center: x,
            lo: y - 2.0,
            hi: y + 2.0,
            d_lo: 0.0,
            d_hi: 1.0,
            body_key: None,
            end_radius: None,
            corner_radius: None,
        };
        for noise in [-1e-12, 0.0, 1e-12] {
            let a = slot(noise, 10.0);
            let b = slot(-noise, 20.0);
            assert_eq!(width_then_centre(&a, &b), Ordering::Less);
            assert_eq!(width_then_centre(&b, &a), Ordering::Greater);
        }
    }
}
