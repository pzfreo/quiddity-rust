//! What a slot, a pocket and a channel each are, given the faces and the reductions
//! (`quiddity._recess_core`). Each family scans one solid: pair facing walls on one axis,
//! recover the recesses whose walls do not pair (obround ends, corner notches), and reduce the
//! candidates to features. `candidate` is a through-void between two walls; `floored_candidate`
//! the same read with a floor found, which makes a pocket (bounded) or a channel (open at both
//! ends of its solid).

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use super::Context;
use super::evidence::common_valid_solid;
use super::graph;
use super::recess_faces::{
    Cylinder, FLOOR_TOL, Face, MERGE_TOL, center, cylinder_faces, end_cap_faces, has_floor,
    overlap_len, planar_faces,
};
use super::recess_obround::{
    CompetingCaps, extend_obround_proposals, pockets_from_ends, slots_from_ends,
};
use super::recess_radii::with_proved_corner_radius;
use super::recess_records::{AXES, Channel, Pocket, Recess, Slot};
use super::recess_reduce::{Proposal, collapse_collinear_proposals, merge_proposals};
use super::volume_probe::{Spans, prism_is_empty};
use super::wire_seed::{inner_wires, wire_seed};
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{Bounds, COORD_FLOOR};
use crate::kernel::py;

/// Overlaps within this fraction are a tie between length and depth, broken by the part.
const LENGTH_TIE_FRAC: f64 = 0.05;
/// A recess spanning this much of its solid along its length is open, not bounded.
const SLOT_MAX_SPAN_FRAC: f64 = 0.9;

/// The face-graph reads of one scan, each computed once (`FaceGraph`'s caches): neighbours,
/// arcs (symmetric, so cached per unordered pair) and smooth regions.
struct Graph<'a> {
    part: &'a Part,
    neighbours: RefCell<HashMap<usize, Rc<Vec<usize>>>>,
    arcs: RefCell<HashMap<(usize, usize), Option<Arc>>>,
    smooth: RefCell<HashMap<usize, Rc<BTreeSet<usize>>>>,
}

impl<'a> Graph<'a> {
    fn new(part: &'a Part) -> Self {
        Graph {
            part,
            neighbours: RefCell::default(),
            arcs: RefCell::default(),
            smooth: RefCell::default(),
        }
    }

    fn neighbours(&self, face: usize) -> Rc<Vec<usize>> {
        if let Some(n) = self.neighbours.borrow().get(&face) {
            return n.clone();
        }
        let n = Rc::new(self.part.neighbours(face));
        self.neighbours.borrow_mut().insert(face, n.clone());
        n
    }

    fn arc(&self, a: usize, b: usize) -> Option<Arc> {
        let key = (a.min(b), a.max(b));
        if let Some(&arc) = self.arcs.borrow().get(&key) {
            return arc;
        }
        let arc = self.part.arc(a, b);
        self.arcs.borrow_mut().insert(key, arc);
        arc
    }

    fn is_planar(&self, face: usize) -> bool {
        graph::is_planar(self.part, face)
    }

    fn span(&self, face: usize, axis: usize) -> (f64, f64) {
        graph::span(self.part, face, axis)
    }

    /// The maximal region reachable across smooth arcs only (`FaceGraph.smooth_region`).
    fn smooth_region(&self, face: usize) -> Rc<BTreeSet<usize>> {
        if let Some(r) = self.smooth.borrow().get(&face) {
            return r.clone();
        }
        let mut found = BTreeSet::from([face]);
        let mut pending = vec![face];
        while let Some(current) = pending.pop() {
            for &n in self.neighbours(current).iter() {
                if !found.contains(&n) && self.arc(current, n) == Some(Arc::Smooth) {
                    found.insert(n);
                    pending.push(n);
                }
            }
        }
        let region = Rc::new(found);
        let mut cache = self.smooth.borrow_mut();
        for &member in region.iter() {
            cache.insert(member, region.clone());
        }
        region
    }

    /// The neighbours two faces share, in the first face's neighbour order.
    fn common(&self, a: usize, b: usize) -> Vec<usize> {
        let theirs = self.neighbours(b);
        self.neighbours(a)
            .iter()
            .copied()
            .filter(|n| theirs.contains(n))
            .collect()
    }
}

/// One solid's scan: its faces, cylinders, envelope and graph.
struct Scan<'c, 'a> {
    ctx: &'c Context<'a>,
    solid: usize,
    faces: Vec<Face>,
    cylinders: Vec<Cylinder>,
    envelope: Bounds,
    part_ext: [f64; 3],
    graph: Graph<'a>,
}

impl<'c, 'a> Scan<'c, 'a> {
    fn new(ctx: &'c Context<'a>, solid: usize) -> Self {
        let part = ctx.part;
        let envelope = super::solid_properties::bounding_box(part, solid);
        Scan {
            ctx,
            solid,
            faces: planar_faces(part, solid),
            cylinders: cylinder_faces(part, solid),
            envelope,
            part_ext: [0, 1, 2].map(|k| envelope.max[k] - envelope.min[k]),
            graph: Graph::new(part),
        }
    }

    /// Wall faces bucketed by their normal's axis, in first-seen order; an oblique wall has no
    /// axis to pair on and is declined here (ADR 0009).
    fn walls_by_axis(&self) -> Vec<(usize, Vec<&Face>)> {
        let mut by_axis: Vec<(usize, Vec<&Face>)> = Vec::new();
        for f in &self.faces {
            let (true, Some(axis)) = (f.wall, f.axis) else {
                continue;
            };
            match by_axis.iter_mut().find(|(a, _)| *a == axis) {
                Some((_, walls)) => walls.push(f),
                None => by_axis.push((axis, vec![f])),
            }
        }
        by_axis
    }

    /// The smooth regions met concavely from a wall (`_concave_boundary_regions`).
    fn concave_boundary_regions(&self, wall: usize) -> Vec<Rc<BTreeSet<usize>>> {
        let mut out: Vec<Rc<BTreeSet<usize>>> = Vec::new();
        for &n in self.graph.neighbours(wall).iter() {
            if self.graph.arc(wall, n) == Some(Arc::Concave) {
                let region = self.graph.smooth_region(n);
                if !out.contains(&region) {
                    out.push(region);
                }
            }
        }
        out
    }

    /// The long-axis span with curved interruptions at its ends removed: a common non-planar
    /// neighbour the material turns oppositely at (a post replacing a slot's end) is added
    /// material, not the void's end (`_uninterrupted_long_span`).
    fn uninterrupted_long_span(
        &self,
        long_axis: usize,
        (mut lo, mut hi): (f64, f64),
        fa: &Face,
        fb: &Face,
    ) -> Option<(f64, f64)> {
        for node in self.graph.common(fa.node, fb.node) {
            if self.graph.is_planar(node) {
                continue;
            }
            let arcs = (self.graph.arc(fa.node, node), self.graph.arc(fb.node, node));
            let opposed = matches!(
                arcs,
                (Some(Arc::Convex), Some(Arc::Concave)) | (Some(Arc::Concave), Some(Arc::Convex))
            );
            if opposed {
                let bounds = self.graph.span(node, long_axis);
                if bounds.0 <= lo + COORD_FLOOR {
                    lo = lo.max(bounds.1);
                }
                if bounds.1 >= hi - COORD_FLOOR {
                    hi = hi.min(bounds.0);
                }
            }
        }
        (hi - lo > COORD_FLOOR).then_some((lo, hi))
    }

    /// Whether the record's uninterrupted prism, inset by `COORD_FLOOR`, is exactly empty
    /// (`_candidate_has_void_evidence`).
    fn has_void_evidence(&self, spans: Spans, long_axis: usize, fa: &Face, fb: &Face) -> bool {
        let Some(long_span) = self.uninterrupted_long_span(long_axis, spans[long_axis], fa, fb)
        else {
            return false;
        };
        let mut probe = spans;
        probe[long_axis] = long_span;
        prism_is_empty(self.ctx, self.solid, &probe, COORD_FLOOR)
    }

    /// Whether a curved region smoothly joining both walls closes either proposed depth end:
    /// a deep obround pocket read along the wrong axis (`_has_smooth_depth_closure`).
    fn has_smooth_depth_closure(
        &self,
        fa: &Face,
        fb: &Face,
        depth_axis: usize,
        (lo, hi): (f64, f64),
    ) -> bool {
        let g = &self.graph;
        for &seed in g.neighbours(fa.node).iter() {
            if g.is_planar(seed) || g.arc(fa.node, seed) != Some(Arc::Smooth) {
                continue;
            }
            let available: BTreeSet<usize> = g
                .smooth_region(seed)
                .iter()
                .copied()
                .filter(|&n| !g.is_planar(n))
                .collect();
            let mut region = BTreeSet::from([seed]);
            let mut pending = vec![seed];
            while let Some(current) = pending.pop() {
                for &n in g.neighbours(current).iter() {
                    if available.contains(&n)
                        && !region.contains(&n)
                        && g.arc(current, n) == Some(Arc::Smooth)
                    {
                        region.insert(n);
                        pending.push(n);
                    }
                }
            }
            let joins_b = region.iter().any(|&n| {
                g.neighbours(n).contains(&fb.node) && g.arc(n, fb.node) == Some(Arc::Smooth)
            });
            if !joins_b {
                continue;
            }
            let region_lo = region
                .iter()
                .map(|&n| g.span(n, depth_axis).0)
                .fold(f64::INFINITY, f64::min);
            let region_hi = region
                .iter()
                .map(|&n| g.span(n, depth_axis).1)
                .fold(f64::NEG_INFINITY, f64::max);
            if (region_hi - lo).abs() <= COORD_FLOOR || (region_lo - hi).abs() <= COORD_FLOOR {
                return true;
            }
        }
        false
    }

    /// Whether two opposed walls bound one void (`_bounds_one_void`): every shared boundary
    /// face (planar ones preferred) meets both alike, or, with none shared, one concave region
    /// or one smooth region joins them.
    fn bounds_one_void(&self, fa: &Face, fb: &Face) -> bool {
        let g = &self.graph;
        let common = g.common(fa.node, fb.node);
        if !common.is_empty() {
            let planar: Vec<usize> = common.iter().copied().filter(|&n| g.is_planar(n)).collect();
            let boundary = if planar.is_empty() { common } else { planar };
            return boundary
                .iter()
                .all(|&n| g.arc(fa.node, n) == g.arc(fb.node, n));
        }
        let theirs = self.concave_boundary_regions(fb.node);
        if self
            .concave_boundary_regions(fa.node)
            .iter()
            .any(|r| theirs.contains(r))
        {
            return true;
        }
        g.smooth_region(fa.node).contains(&fb.node)
    }

    /// The facing pair's centres along the width axis, if the walls are anti-parallel, face
    /// each other across a cavity and bound one void.
    fn facing(&self, fa: &Face, fb: &Face, k: usize) -> Option<(f64, f64)> {
        if fa.normal[k] * fb.normal[k] >= 0.0 {
            return None;
        }
        let (c_a, c_b) = (center(&fa.bb, k), center(&fb.bb, k));
        if (c_b - c_a) * fa.normal[k] <= 0.0 {
            return None;
        }
        self.bounds_one_void(fa, fb).then_some((c_a, c_b))
    }

    /// A through-slot candidate from two facing walls, before the floor test (`_candidate`).
    fn candidate(&self, fa: &Face, fb: &Face, axis: usize) -> Option<Slot> {
        let (c_a, c_b) = self.facing(fa, fb, axis)?;
        let others: Vec<usize> = (0..3).filter(|&a| a != axis).collect();
        let ov: Vec<f64> = others
            .iter()
            .map(|&a| overlap_len(&fa.bb, &fb.bb, a))
            .collect();
        if ov.iter().copied().fold(f64::INFINITY, f64::min) <= 0.0 {
            return None;
        }
        let width = (c_b - c_a).abs();
        // The longer shared extent is the length, the other the depth (sorted descending,
        // stable); a near tie goes to the part's longer axis.
        let ((ax0, ov0), (ax1, ov1)) = if py::order(ov[1], ov[0]).is_gt() {
            ((others[1], ov[1]), (others[0], ov[0]))
        } else {
            ((others[0], ov[0]), (others[1], ov[1]))
        };
        let (long_axis, length, depth_axis) =
            if (ov0 - ov1) <= LENGTH_TIE_FRAC * ov0 && self.part_ext[ax1] > self.part_ext[ax0] {
                (ax1, ov1, ax0)
            } else {
                (ax0, ov0, ax1)
            };
        if width > length || length >= SLOT_MAX_SPAN_FRAC * self.part_ext[long_axis] {
            return None;
        }
        let lo = fa.bb.min[long_axis].max(fb.bb.min[long_axis]);
        let hi = fa.bb.max[long_axis].min(fb.bb.max[long_axis]);
        let d_lo = fa.bb.min[depth_axis].max(fb.bb.min[depth_axis]);
        let d_hi = fa.bb.max[depth_axis].min(fb.bb.max[depth_axis]);
        if self.has_smooth_depth_closure(fa, fb, depth_axis, (d_lo, d_hi)) {
            return None;
        }
        let mut spans: Spans = [(0.0, 0.0); 3];
        spans[axis] = (c_a.min(c_b), c_a.max(c_b));
        spans[long_axis] = (lo, hi);
        spans[depth_axis] = (d_lo, d_hi);
        if !self.has_void_evidence(spans, long_axis, fa, fb) {
            return None;
        }
        Some(Slot {
            width_axis: AXES[axis],
            long_axis: AXES[long_axis],
            width: py::round_to(width, 2),
            length: py::round_to(hi - lo, 2),
            w_center: py::round_to((c_a + c_b) / 2.0, 2),
            lo: py::round_to(lo, 2),
            hi: py::round_to(hi, 2),
            d_lo: py::round_to(d_lo, 2),
            d_hi: py::round_to(d_hi, 2),
            body_key: Some(Vec::new()),
            end_radius: None,
            corner_radius: None,
        })
    }

    /// A floored opposed-wall recess and the floor faces that cap it (`_floored_candidate`):
    /// a bounded `Pocket` without *channel_bounds*, a `Channel` open at both ends of the solid's
    /// envelope with them. The depth axis is the one capped at exactly one end; two such axes
    /// are a corner ambiguity and refuse.
    fn floored_candidate(
        &self,
        fa: &Face,
        fb: &Face,
        axis: usize,
        channel_bounds: Option<&Bounds>,
    ) -> Option<(Floored, Vec<usize>)> {
        let (c_a, c_b) = self.facing(fa, fb, axis)?;
        let width = (c_b - c_a).abs();
        let others: Vec<usize> = (0..3).filter(|&a| a != axis).collect();
        let mut ranges = [(0.0, 0.0); 3];
        for &a in &others {
            let lo = fa.bb.min[a].max(fb.bb.min[a]);
            let hi = fa.bb.max[a].min(fb.bb.max[a]);
            if hi - lo <= 0.0 {
                return None;
            }
            ranges[a] = (lo, hi);
        }
        let mid = (c_a + c_b) / 2.0;
        let w_range = (mid - width / 2.0, mid + width / 2.0);
        let mut candidates: Vec<(Floored, Vec<usize>)> = Vec::new();
        for &depth_axis in &others {
            let long_axis = others.iter().copied().find(|&a| a != depth_axis)?;
            let (d_lo, d_hi) = ranges[depth_axis];
            let (l_lo, l_hi) = ranges[long_axis];
            let foot = [(axis, w_range), (long_axis, (l_lo, l_hi))];
            let foot_area = width * (l_hi - l_lo);
            let cap_lo = end_cap_faces(&self.faces, &foot, foot_area, depth_axis, d_lo, 1.0);
            let cap_hi = end_cap_faces(&self.faces, &foot, foot_area, depth_axis, d_hi, -1.0);
            if usize::from(!cap_lo.is_empty()) + usize::from(!cap_hi.is_empty()) != 1 {
                continue;
            }
            let length = l_hi - l_lo;
            let open_sign = if cap_lo.is_empty() { -1 } else { 1 };
            let selected = if cap_lo.is_empty() { cap_hi } else { cap_lo };
            let mut spans: Spans = [(0.0, 0.0); 3];
            spans[axis] = (c_a.min(c_b), c_a.max(c_b));
            spans[long_axis] = (l_lo, l_hi);
            spans[depth_axis] = (d_lo, d_hi);
            let record = match channel_bounds {
                None => {
                    if width > length
                        || length >= SLOT_MAX_SPAN_FRAC * self.part_ext[long_axis]
                        || !self.has_void_evidence(spans, long_axis, fa, fb)
                    {
                        continue;
                    }
                    Floored::Pocket(Pocket {
                        width_axis: AXES[axis],
                        long_axis: AXES[long_axis],
                        width: py::round_to(width, 2),
                        length: py::round_to(length, 2),
                        depth: py::round_to(d_hi - d_lo, 2),
                        w_center: py::round_to(mid, 2),
                        lo: py::round_to(l_lo, 2),
                        hi: py::round_to(l_hi, 2),
                        d_lo: py::round_to(d_lo, 2),
                        d_hi: py::round_to(d_hi, 2),
                        open_sign,
                        edge_anchored: false,
                        body_key: Some(Vec::new()),
                        end_radius: None,
                        corner_radius: None,
                    })
                }
                Some(bounds) => {
                    let (part_lo, part_hi) = (bounds.min[long_axis], bounds.max[long_axis]);
                    if (l_lo - part_lo).abs() > FLOOR_TOL
                        || (l_hi - part_hi).abs() > FLOOR_TOL
                        || !self.has_void_evidence(spans, long_axis, fa, fb)
                    {
                        continue;
                    }
                    Floored::Channel(Channel {
                        width_axis: AXES[axis],
                        long_axis: AXES[long_axis],
                        width: py::round_to(width, 2),
                        w_center: py::round_to(mid, 2),
                        lo: py::round_to(l_lo, 2),
                        hi: py::round_to(l_hi, 2),
                        d_lo: py::round_to(d_lo, 2),
                        d_hi: py::round_to(d_hi, 2),
                        open_sign,
                        body_key: Some(Vec::new()),
                    })
                }
            };
            candidates.push((record, selected));
        }
        if candidates.len() != 1 {
            return None;
        }
        candidates.pop()
    }
}

/// What `floored_candidate` builds.
enum Floored {
    Pocket(Pocket),
    Channel(Channel),
}

/// Every through-slot of one solid with the faces it was built from (`_slot_proposals_one`):
/// unfloored wall pairs and obround ends, merged, arms rejoined, obround ends extended and
/// corner radii proved. With *strict*, competing cap clusters refuse.
pub fn slot_proposals(
    ctx: &Context<'_>,
    solid: usize,
    strict: bool,
) -> Result<Vec<Proposal<Slot>>, CompetingCaps> {
    let scan = Scan::new(ctx, solid);
    let mut candidates = Vec::new();
    for (axis, walls) in scan.walls_by_axis() {
        for i in 0..walls.len() {
            for j in i + 1..walls.len() {
                if let Some(s) = scan.candidate(walls[i], walls[j], axis)
                    && !has_floor(&scan.faces, &s)
                {
                    candidates.push(Proposal {
                        planar: BTreeSet::from([walls[i].node, walls[j].node]),
                        ..Proposal::new(s)
                    });
                }
            }
        }
    }
    candidates.extend(slots_from_ends(&scan.faces, &scan.cylinders));
    let merged = collapse_collinear_proposals(ctx, solid, merge_proposals(candidates));
    let extended = extend_obround_proposals(merged, &scan.cylinders, strict)?;
    Ok(extended
        .into_iter()
        .map(|p| {
            let record = with_proved_corner_radius(p.record.clone(), &scan.cylinders);
            p.replaced(record)
        })
        .collect())
}

/// Every bounded pocket of one solid with its faces (`_pocket_proposals_one`): floored wall
/// pairs (walls defining, floor consulted), corner notches (floor and both walls defining) and
/// obround ends (caps defining, floor consulted), reduced as slots are, then each given its
/// complete inner region where one is proved.
pub fn pocket_proposals(
    ctx: &Context<'_>,
    solid: usize,
    strict: bool,
) -> Result<Vec<Proposal<Pocket>>, CompetingCaps> {
    let scan = Scan::new(ctx, solid);
    let mut candidates = Vec::new();
    for (axis, walls) in scan.walls_by_axis() {
        for i in 0..walls.len() {
            for j in i + 1..walls.len() {
                if let Some((Floored::Pocket(p), floors)) =
                    scan.floored_candidate(walls[i], walls[j], axis, None)
                {
                    // A cap is found only where some face covers the footprint, so there is
                    // always at least one floor face here.
                    debug_assert!(!floors.is_empty());
                    candidates.push(Proposal {
                        planar: BTreeSet::from([walls[i].node, walls[j].node]),
                        floors: floors.into_iter().collect(),
                        ..Proposal::new(p)
                    });
                }
            }
        }
    }
    candidates.extend(corner_notch_proposals(&scan.faces, &scan.envelope));
    candidates.extend(pockets_from_ends(&scan.faces, &scan.cylinders));
    let extended = extend_obround_proposals(merge_proposals(candidates), &scan.cylinders, strict)?;
    let proposals: Vec<Proposal<Pocket>> = extended
        .into_iter()
        .map(|p| {
            let record = with_proved_corner_radius(p.record.clone(), &scan.cylinders);
            p.replaced(record)
        })
        .collect();
    Ok(attach_complete_pocket_regions(&scan, proposals))
}

/// One channel with its two side walls (low then high along the width axis) and its floor
/// (`_ChannelProposal`).
#[derive(Clone, Debug)]
pub struct ChannelProposal {
    pub record: Channel,
    pub low_wall: usize,
    pub high_wall: usize,
    pub floor: BTreeSet<usize>,
}

/// Every channel candidate of one solid (`_channel_proposals_one`), one per wall pair.
pub fn channel_proposals(ctx: &Context<'_>, solid: usize) -> Vec<ChannelProposal> {
    let scan = Scan::new(ctx, solid);
    let mut out = Vec::new();
    for (axis, walls) in scan.walls_by_axis() {
        for i in 0..walls.len() {
            for j in i + 1..walls.len() {
                let (first, second) = (walls[i], walls[j]);
                if let Some((Floored::Channel(record), floor)) =
                    scan.floored_candidate(first, second, axis, Some(&scan.envelope))
                {
                    let (low_wall, high_wall) =
                        if center(&first.bb, axis) <= center(&second.bb, axis) {
                            (first.node, second.node)
                        } else {
                            (second.node, first.node)
                        };
                    out.push(ChannelProposal {
                        record,
                        low_wall,
                        high_wall,
                        floor: floor.into_iter().collect(),
                    });
                }
            }
        }
    }
    out
}

/// The region an opening's inner wire bounds: from the faces on the wire, across concave and
/// smooth arcs only, never back through the opening (`_bounded_inner_region`).
fn bounded_inner_region(g: &Graph<'_>, opening: usize, seed: &BTreeSet<usize>) -> BTreeSet<usize> {
    let mut region = seed.clone();
    let mut pending: Vec<usize> = seed.iter().copied().collect();
    while let Some(current) = pending.pop() {
        for &n in g.neighbours(current).iter() {
            if n == opening || region.contains(&n) {
                continue;
            }
            if matches!(g.arc(current, n), Some(Arc::Concave | Arc::Smooth)) {
                region.insert(n);
                pending.push(n);
            }
        }
    }
    region
}

/// Each pocket given the complete inner region of an opening wire that holds all its faces,
/// where that region is the only one that does, intersects no other region and serves no other
/// pocket (`_attach_complete_pocket_regions`). Ambiguity changes nothing. The regions are read
/// within the scanned solid: a region that leaves it has no common valid solid and is dropped,
/// as Python's run-wide graph drops it.
fn attach_complete_pocket_regions(
    scan: &Scan<'_, '_>,
    proposals: Vec<Proposal<Pocket>>,
) -> Vec<Proposal<Pocket>> {
    if proposals.is_empty() {
        return proposals;
    }
    let part = scan.ctx.part;
    let g = &scan.graph;
    let mut by_region: Vec<(BTreeSet<usize>, BTreeSet<usize>)> = Vec::new();
    for &opening in &part.solids[scan.solid].faces {
        for wire in inner_wires(part, opening) {
            let seed = wire_seed(part, opening, &wire);
            if seed.is_empty() {
                continue;
            }
            let region = bounded_inner_region(g, opening, &seed);
            match by_region.iter_mut().find(|(r, _)| *r == region) {
                Some((_, openings)) => {
                    openings.insert(opening);
                }
                None => by_region.push((region, BTreeSet::from([opening]))),
            }
        }
    }
    let mut regions: Vec<BTreeSet<usize>> = Vec::new();
    for (region, openings) in by_region {
        let members: Vec<usize> = region.union(&openings).copied().collect();
        let Some(solid) = common_valid_solid(part, &members) else {
            continue;
        };
        let covers_solid = part.solids[solid]
            .faces
            .iter()
            .all(|f| openings.contains(f) || region.contains(f));
        if covers_solid {
            continue;
        }
        regions.push(region);
    }
    // Sorted by their sorted face indices (BTreeSet order is that tuple order).
    regions.sort();
    let intersecting: Vec<bool> = (0..regions.len())
        .map(|i| (0..regions.len()).any(|j| j != i && !regions[i].is_disjoint(&regions[j])))
        .collect();
    let matches: Vec<Vec<usize>> = proposals
        .iter()
        .map(|p| {
            let anchors: BTreeSet<usize> = p.defining().union(&p.floors).copied().collect();
            (0..regions.len())
                .filter(|&i| !intersecting[i] && anchors.is_subset(&regions[i]))
                .collect()
        })
        .collect();
    let uses = |index: usize| matches.iter().flatten().filter(|&&i| i == index).count();
    proposals
        .into_iter()
        .zip(&matches)
        .map(|(mut p, m)| {
            if let [only] = m[..]
                && uses(only) == 1
            {
                p.constituent = regions[only].clone();
            }
            p
        })
        .collect()
}

/// Rectangular blind corner interruptions: a floor touching two adjacent envelope edges with
/// one wall on each of the two footprint axes; of the interpretations of one removed box the
/// uniquely shallowest leg is the depth, and a tie refuses (`_corner_notch_proposals`).
fn corner_notch_proposals(faces: &[Face], pbb: &Bounds) -> Vec<Proposal<Pocket>> {
    let tol = MERGE_TOL;
    let limits = |bb: &Bounds, axis: usize| (bb.min[axis], bb.max[axis]);
    let envelope = [0, 1, 2].map(|a| limits(pbb, a));
    let mut candidates: Vec<(f64, Proposal<Pocket>)> = Vec::new();
    for depth_axis in 0..3 {
        let footprint: Vec<usize> = (0..3).filter(|&a| a != depth_axis).collect();
        let (first_axis, second_axis) = (footprint[0], footprint[1]);
        for floor in faces
            .iter()
            .filter(|f| f.axis == Some(depth_axis) && f.wall)
        {
            let (first_lo, first_hi) = limits(&floor.bb, first_axis);
            let (second_lo, second_hi) = limits(&floor.bb, second_axis);
            let (depth_lo, depth_hi) = limits(&floor.bb, depth_axis);
            let first_span = first_hi - first_lo;
            let second_span = second_hi - second_lo;
            if first_span <= tol || second_span <= tol || (depth_hi - depth_lo).abs() > tol {
                continue;
            }
            let span_of = |a: usize| envelope[a].1 - envelope[a].0;
            if first_span >= SLOT_MAX_SPAN_FRAC * span_of(first_axis)
                || second_span >= SLOT_MAX_SPAN_FRAC * span_of(second_axis)
            {
                continue;
            }
            let first_at_low = (first_lo - envelope[first_axis].0).abs() <= tol;
            let first_at_high = (first_hi - envelope[first_axis].1).abs() <= tol;
            let second_at_low = (second_lo - envelope[second_axis].0).abs() <= tol;
            let second_at_high = (second_hi - envelope[second_axis].1).abs() <= tol;
            let depth_end_gap = (depth_lo - envelope[depth_axis].0)
                .abs()
                .min((depth_lo - envelope[depth_axis].1).abs());
            if !((first_at_low || first_at_high) && (second_at_low || second_at_high))
                || depth_end_gap <= tol
            {
                continue;
            }
            let first_inner = if first_at_low { first_hi } else { first_lo };
            let second_inner = if second_at_low { second_hi } else { second_lo };
            let first_wall = faces.iter().find(|f| {
                f.axis == Some(first_axis)
                    && (center(&f.bb, first_axis) - first_inner).abs() <= tol
                    && overlap_len(&f.bb, &floor.bb, second_axis) >= second_span - tol
            });
            let second_wall = faces.iter().find(|f| {
                f.axis == Some(second_axis)
                    && (center(&f.bb, second_axis) - second_inner).abs() <= tol
                    && overlap_len(&f.bb, &floor.bb, first_axis) >= first_span - tol
            });
            let (Some(first_wall), Some(second_wall)) = (first_wall, second_wall) else {
                continue;
            };
            let first_depth = limits(&first_wall.bb, depth_axis);
            let second_depth = limits(&second_wall.bb, depth_axis);
            if (first_depth.0 - second_depth.0).abs() > tol
                || (first_depth.1 - second_depth.1).abs() > tol
            {
                continue; // a split side cannot define the complete removed box
            }
            let d_lo = first_depth.0.max(second_depth.0);
            let d_hi = first_depth.1.min(second_depth.1);
            if d_hi - d_lo <= tol || !(d_lo - tol <= depth_lo && depth_lo <= d_hi + tol) {
                continue;
            }
            let (width_axis, long_axis, width, length, w_center, lo, hi) =
                if first_span <= second_span {
                    (
                        first_axis,
                        second_axis,
                        first_span,
                        second_span,
                        (first_lo + first_hi) / 2.0,
                        second_lo,
                        second_hi,
                    )
                } else {
                    (
                        second_axis,
                        first_axis,
                        second_span,
                        first_span,
                        (second_lo + second_hi) / 2.0,
                        first_lo,
                        first_hi,
                    )
                };
            let record = Pocket {
                width_axis: AXES[width_axis],
                long_axis: AXES[long_axis],
                width: py::round_to(width, 2),
                length: py::round_to(length, 2),
                depth: py::round_to(d_hi - d_lo, 2),
                w_center: py::round_to(w_center, 2),
                lo: py::round_to(lo, 2),
                hi: py::round_to(hi, 2),
                d_lo: py::round_to(d_lo, 2),
                d_hi: py::round_to(d_hi, 2),
                open_sign: if floor.normal[depth_axis] > 0.0 {
                    1
                } else {
                    -1
                },
                edge_anchored: true,
                body_key: Some(Vec::new()),
                end_radius: None,
                corner_radius: None,
            };
            candidates.push((
                d_hi - d_lo,
                Proposal {
                    planar: BTreeSet::from([floor.node, first_wall.node, second_wall.node]),
                    ..Proposal::new(record)
                },
            ));
        }
    }
    // Interpretations of one removed box share its centre: group them, then take the depth
    // only where one leg is uniquely the shallowest.
    let mut groups: Vec<Vec<(f64, Proposal<Pocket>)>> = Vec::new();
    for candidate in candidates {
        let centre = candidate.1.record.region_center();
        match groups
            .iter_mut()
            .find(|g| py::dist(&centre, &g[0].1.record.region_center()) <= tol)
        {
            Some(group) => group.push(candidate),
            None => groups.push(vec![candidate]),
        }
    }
    let mut out = Vec::new();
    for mut group in groups {
        group.sort_by(|a, b| py::order(a.0, b.0));
        if group.len() > 1 && group[1].0 - group[0].0 <= COORD_FLOOR {
            continue;
        }
        out.push(group.swap_remove(0).1);
    }
    out
}
