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

internal_entry! {
/// The order recess records are reduced and published in: width, then region centre.
pub fn width_then_centre<R: Recess>(a: &R, b: &R) -> Ordering {
    // Raw positions must survive reduction, but sub-coordinate-floor noise must not decide
    // which equal-width record comes first (and hence its section-recess index). Quantise
    // only this ordering key; measured spans and the records themselves remain untouched.
    let centre_key = |r: &R| r.region_center().map(|c| py::round_to(c, 6));
    py::order(a.width(), b.width()).then_with(|| py::tuple_order(&centre_key(a), &centre_key(b)))
}
}

internal_entry! {
/// Co-located readings share one occurrence only when the minimum-width interpretation is
/// unique. A directional square record needs a unique body-relative run, too (ADR 0011).
/// Evidence is combined only after this decision; a face number cannot choose the record.
pub fn merge_proposals<R: Recess>(proposals: Vec<Proposal<R>>) -> Vec<Proposal<R>> {
    merge_proposals_in(proposals, None)
}
}

pub(crate) fn merge_proposals_in<R: Recess>(
    proposals: Vec<Proposal<R>>,
    extents: Option<[f64; 3]>,
) -> Vec<Proposal<R>> {
    let mut remaining: BTreeSet<usize> = (0..proposals.len()).collect();
    let mut out = Vec::new();
    while let Some(first) = remaining.pop_first() {
        let mut members = vec![first];
        let mut pending = vec![first];
        while let Some(i) = pending.pop() {
            let adjacent: Vec<_> = remaining
                .iter()
                .copied()
                .filter(|&j| {
                    py::dist(
                        &proposals[i].record.region_center(),
                        &proposals[j].record.region_center(),
                    ) <= MERGE_TOL
                })
                .collect();
            for j in adjacent {
                remaining.remove(&j);
                members.push(j);
                pending.push(j);
            }
        }
        // A tolerance chain does not prove one co-located occurrence.
        if members.iter().any(|&i| {
            members.iter().any(|&j| {
                py::dist(
                    &proposals[i].record.region_center(),
                    &proposals[j].record.region_center(),
                ) > MERGE_TOL
            })
        }) {
            continue;
        }
        let width = members
            .iter()
            .map(|&i| proposals[i].record.width())
            .fold(f64::INFINITY, f64::min);
        let tied: Vec<_> = members
            .iter()
            .copied()
            .filter(|&i| {
                (proposals[i].record.width() - width).abs() <= crate::kernel::geom::COORD_FLOOR
            })
            .collect();
        // When width alone ties, the uniquely shallowest removed-box interpretation is
        // established by geometry (the same criterion as corner-notch recognition).
        let depth = tied
            .iter()
            .map(|&i| proposals[i].record.d_hi() - proposals[i].record.d_lo())
            .fold(f64::INFINITY, f64::min);
        let tied: Vec<_> = tied
            .into_iter()
            .filter(|&i| {
                (proposals[i].record.d_hi() - proposals[i].record.d_lo() - depth).abs()
                    <= crate::kernel::geom::COORD_FLOOR
            })
            .collect();
        let best = &proposals[tied[0]];
        if tied
            .iter()
            .any(|&i| !equivalent_roles(&best.record, &proposals[i].record))
        {
            continue;
        }
        // long_axis is consumed as a semantic axis by fingerprints and pattern grouping.
        // A square cannot silently give it a frame/face-order gauge: the part must establish
        // a unique transverse long direction, or this directional record refuses.
        let best = if tied
            .iter()
            .any(|&i| !best.record.same_roles(&proposals[i].record))
        {
            let Some(extents) = extents else {
                continue;
            };
            let reach = tied
                .iter()
                .map(|&i| extents[proposals[i].record.long_axis()])
                .fold(f64::NEG_INFINITY, f64::max);
            let supported: Vec<_> = tied
                .iter()
                .copied()
                .filter(|&i| {
                    (extents[proposals[i].record.long_axis()] - reach).abs()
                        <= crate::kernel::geom::COORD_FLOOR
                })
                .collect();
            let selected = &proposals[supported[0]];
            if supported
                .iter()
                .any(|&i| !selected.record.same_roles(&proposals[i].record))
            {
                continue;
            }
            selected
        } else {
            best
        };
        let refs: Vec<_> = members.iter().map(|&i| &proposals[i]).collect();
        out.push(combine(best.record.clone(), &refs));
    }
    out.sort_by(|a, b| width_then_centre(&a.record, &b.record));
    out
}

/// Two square readings may describe one box, but their directional role still needs a
/// geometric body-relative proof before either can be published.
fn equivalent_roles<R: Recess>(a: &R, b: &R) -> bool {
    if a.same_roles(b) {
        return true;
    }
    let close = |a: f64, b: f64| (a - b).abs() <= crate::kernel::geom::COORD_FLOOR;
    a.depth_axis() == b.depth_axis()
        && a.same_opening(b)
        && close(a.width(), a.hi() - a.lo())
        && close(b.width(), b.hi() - b.lo())
        && close(a.d_lo(), b.d_lo())
        && close(a.d_hi(), b.d_hi())
        && py::dist(&a.region_center(), &b.region_center()) <= crate::kernel::geom::COORD_FLOOR
}

/// The gap between two collinear arms of one channel (same width axis, centreline, width and
/// depth, disjoint along the run), or `None` (`_same_channel_line`).
fn same_channel_line(a: &Slot, b: &Slot) -> Option<(f64, f64)> {
    if a.width_axis != b.width_axis || a.long_axis != b.long_axis {
        return None;
    }
    if (*a.w_center - *b.w_center).abs() > MERGE_TOL || (a.width - b.width).abs() > MERGE_TOL {
        return None;
    }
    if (*a.d_lo - *b.d_lo).abs() > MERGE_TOL || (*a.d_hi - *b.d_hi).abs() > MERGE_TOL {
        return None;
    }
    let gap = if *a.hi <= *b.lo {
        (*a.hi, *b.lo)
    } else if *b.hi <= *a.lo {
        (*b.hi, *a.lo)
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
        *arm.w_center - arm.width / 2.0,
        *arm.w_center + arm.width / 2.0,
    );
    spans[arm.depth_axis()] = (*arm.d_lo, *arm.d_hi);
    prism_material_fraction(ctx, solid, &spans, VOID_INSET).is_some_and(|f| f <= VOID_VOL_FRAC)
}

internal_entry! {
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
                .map(|&m| *proposals[m].record.lo)
                .fold(f64::INFINITY, f64::min);
            let hi = members
                .iter()
                .map(|&m| *proposals[m].record.hi)
                .fold(f64::NEG_INFINITY, f64::max);
            let spanned = Slot {
                width_axis: base.width_axis,
                long_axis: base.long_axis,
                width: base.width,
                length: round_measurement(hi - lo, 2),
                w_center: base.w_center,
                lo: lo.into(),
                hi: hi.into(),
                d_lo: base.d_lo,
                d_hi: base.d_hi,
                body_key: Some(Vec::new().into()),
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
}

internal_entry! {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_width_recess_order_ignores_roundoff_in_a_tied_coordinate() {
        let slot = |x: f64, y: f64| Slot {
            width_axis: 'x',
            long_axis: 'y',
            width: 2.0,
            length: 4.0,
            w_center: x.into(),
            lo: (y - 2.0).into(),
            hi: (y + 2.0).into(),
            d_lo: 0.0.into(),
            d_hi: 1.0.into(),
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

#[cfg(test)]
mod role_motion_tests {
    use super::*;
    #[test]
    fn equal_width_crossing_boxes_refuse_after_motion_and_proposal_permutation() {
        for [x, y, z] in [
            [0, 1, 2],
            [1, 0, 2],
            [2, 1, 0],
            [0, 2, 1],
            [1, 2, 0],
            [2, 0, 1],
        ] {
            for shift in [0., 123.456] {
                let slot = |w, l| Slot {
                    width_axis: ['x', 'y', 'z'][w],
                    long_axis: ['x', 'y', 'z'][l],
                    width: 2.,
                    length: 6.,
                    w_center: shift.into(),
                    lo: (shift - 3.).into(),
                    hi: (shift + 3.).into(),
                    d_lo: (shift - 1.).into(),
                    d_hi: (shift + 1.).into(),
                    body_key: None,
                    end_radius: None,
                    corner_radius: None,
                };
                let mut proposals = vec![Proposal::new(slot(x, y)), Proposal::new(slot(y, x))];
                // Two equal 2x6 arms of a plus-shaped void have no unique single-box record.
                proposals[0].planar = BTreeSet::from([9, 10]);
                proposals[1].planar = BTreeSet::from([1, 2]);
                assert!(merge_proposals(proposals.clone()).is_empty(), "depth {z}");
                proposals.reverse();
                assert!(merge_proposals(proposals).is_empty());
            }
        }
    }
}

#[cfg(test)]
mod depth_tie_tests {
    use super::*;
    #[test]
    fn equal_width_roles_choose_the_unique_shallow_depth_after_motion_and_permutation() {
        for [x, y, z] in [
            [0, 1, 2],
            [1, 0, 2],
            [2, 1, 0],
            [0, 2, 1],
            [1, 2, 0],
            [2, 0, 1],
        ] {
            for shift in [0., 123.456] {
                let slot = |w: usize, l: usize, run: f64, depth: f64| Slot {
                    width_axis: ['x', 'y', 'z'][w],
                    long_axis: ['x', 'y', 'z'][l],
                    width: 2.,
                    length: run,
                    w_center: shift.into(),
                    lo: (shift - run / 2.).into(),
                    hi: (shift + run / 2.).into(),
                    d_lo: (shift - depth / 2.).into(),
                    d_hi: (shift + depth / 2.).into(),
                    body_key: None,
                    end_radius: None,
                    corner_radius: None,
                };
                let mut proposals = vec![
                    Proposal::new(slot(x, y, 2., 6.)),
                    Proposal::new(slot(y, z, 6., 1.)),
                ];
                for _ in 0..2 {
                    let result = merge_proposals(proposals.clone());
                    assert_eq!(result.len(), 1);
                    assert_eq!(result[0].record.depth_axis(), x);
                    assert_eq!(*result[0].record.d_hi - *result[0].record.d_lo, 1.);
                    proposals.reverse();
                }
            }
        }
    }
}
