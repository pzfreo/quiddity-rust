//! Drilled-hole recognition (`quiddity.holes`).
//!
//! Coaxial internal cylinders are grouped into stacks — drill, optional counterbore and
//! optional spotface become one hole, and a bore interrupted by a crossing hole is recombined.
//! Each stack's ends are classified from the faces beyond them, which decides the opening, the
//! bottom and the depth.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::Context;
use super::countersinks::{
    CounterSink, HoleMouth, countersink_matches_hole, recognise_countersinks,
};
use super::cylinders::{
    STACK_GAP_FRAC, Segment, axis_point_at, full_cylinders, line_key, merge_runs, segments,
};
use super::evidence::Occurrence;
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Surface, V3};
use crate::kernel::py;

/// Two quantised diameters are one diameter within this proportion.
const SAME_DIAMETER_FRAC: f64 = 1e-4;
/// Smallest diameter the proportional test divides by.
const DIAMETER_FLOOR: f64 = 1e-9;
/// A step shallower than this fraction of its diameter is a spotface, deeper a counterbore.
const SPOTFACE_MAX_RATIO: f64 = 0.2;

/// A cylindrical enlargement at a hole mouth.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CounterBore {
    pub diameter: f64,
    pub depth: f64,
}

/// A drilled hole: `axis` points from the opening into the hole, `location` is the axis point at
/// the opening; `diameter`/`depth` describe the bore (the narrowest segment), with `depth` from
/// the top of the bore to the hole's deep end. `bottom` is `"through"`, `"flat"`,
/// `"drill_point"` or `"unknown"`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HoleRecord {
    pub axis: V3,
    pub location: V3,
    pub diameter: f64,
    pub depth: f64,
    pub bottom: String,
    pub cbore: Option<CounterBore>,
    pub spotface: Option<CounterBore>,
    pub csink: Option<CounterSink>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HoleOptions {
    /// Compose recognised countersinks onto the holes they flare (Python's
    /// `csinks=recognise_countersinks(part)`); off by default, as there.
    pub with_countersinks: bool,
}

/// `recognise_holes`.
pub fn recognise_holes(part: &Part, opts: &HoleOptions) -> Vec<HoleRecord> {
    let ctx = Context::new(part);
    let csinks = if opts.with_countersinks {
        recognise_countersinks(part)
    } else {
        Vec::new()
    };
    discover(&ctx, &csinks)
        .into_iter()
        .map(|o| o.record)
        .collect()
}

fn same_diameter(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME_DIAMETER_FRAC * a.abs().max(b.abs()).max(DIAMETER_FLOOR)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum End {
    Open,
    Flat,
    DrillPoint,
    Unknown,
}

/// The faces beyond one axial end of a segment: partners across the segment's edges that lie at
/// that end (within a margin for lips dipping off the end plane), nearest first
/// (`_end_partners`).
fn end_partners(part: &Part, seg: &Segment, s_end: f64) -> Vec<usize> {
    let d = seg.direction;
    let margin = geom::length_tol(seg.diameter, STACK_GAP_FRAC)
        .max((0.45 * (seg.s_hi - seg.s_lo)).min(0.5 * seg.diameter));
    let edge_faces = part.edge_faces();
    // (distance, first-seen order, face)
    let mut ranked: Vec<(f64, usize, usize)> = Vec::new();
    for &face in &seg.faces {
        for edge in part.face_edges(face) {
            let e = &part.edges[edge];
            let distance = std::iter::once(e.midpoint())
                .chain(e.vertex_points())
                .map(|p| (geom::dot(p, d) - s_end).abs())
                .fold(f64::NEG_INFINITY, f64::max);
            if distance > margin {
                continue;
            }
            for &partner in &edge_faces[edge] {
                if seg.faces.contains(&partner) {
                    continue;
                }
                match ranked.iter_mut().find(|r| r.2 == partner) {
                    None => {
                        let order = ranked.len();
                        ranked.push((distance, order, partner));
                    }
                    Some(r) if distance < r.0 => r.0 = distance,
                    Some(_) => {}
                }
            }
        }
    }
    ranked.sort_by(|a, b| py::order(a.0, b.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|r| r.2).collect()
}

/// Classify one axial end of a segment from the face beyond it (`_classify_end`). Planes,
/// cones and tori decide; a curved wall is a weak signal that counts only when nothing decides.
fn classify_end(part: &Part, seg: &Segment, s_end: f64, hi_end: bool) -> End {
    let d = seg.direction;
    let e_sign = if hi_end { 1.0 } else { -1.0 };
    let mut weak = None;
    for partner in end_partners(part, seg, s_end) {
        let surface = &part.faces[partner].surface;
        match surface {
            Surface::Cone { .. } => {
                let apex = surface.cone_apex().expect("a cone");
                let outward = (geom::dot(apex, d) - s_end) * e_sign > 0.0;
                if seg.external {
                    return if outward { End::Open } else { End::Flat };
                }
                if !outward {
                    return End::Open; // an entry chamfer or countersink widens the bore
                }
                // Apex outward closes the bore — unless a plane across the cone faces back
                // along the axis, which makes it a chamfered flat floor.
                for e2 in part.face_edges(partner) {
                    for &n in &part.edge_faces()[e2] {
                        if n == partner || seg.faces.contains(&n) {
                            continue;
                        }
                        if let Some(normal) = plane_normal(part, n) {
                            if geom::dot(normal, d).abs() > 0.9 {
                                return End::Flat;
                            }
                        }
                    }
                }
                return End::DrillPoint;
            }
            Surface::Torus { major, .. } => {
                let curls_in = *major < seg.diameter / 2.0;
                return match (seg.external, curls_in) {
                    (false, true) | (true, false) => End::Flat,
                    _ => End::Open,
                };
            }
            Surface::Plane { .. } => {
                let normal = plane_normal(part, partner).expect("a plane");
                let alignment = py::dot(&normal, &d) * e_sign;
                if alignment < -0.5 {
                    return End::Flat;
                }
                if alignment > 0.5 {
                    return End::Open;
                }
            }
            Surface::Sphere { .. } => {
                let convex = part.frame_points_outward(partner).unwrap_or(false);
                weak = Some(if seg.external == convex {
                    End::Flat
                } else {
                    End::Open
                });
            }
            Surface::Cylinder { .. } => {
                weak = Some(if seg.external { End::Flat } else { End::Open });
            }
            // A freeform face may be an exact plane in spline form; Python certifies that
            // through its effective-surface recovery, which this port does not yet have.
            _ => {}
        }
    }
    weak.unwrap_or(End::Unknown)
}

/// A planar face's outward normal.
fn plane_normal(part: &Part, face: usize) -> Option<V3> {
    match part.faces[face].surface {
        Surface::Plane { .. } => part.face_normal(face, 0.0, 0.0),
        _ => None,
    }
}

/// A cone or torus spans the gap between *a*'s high end and *b*'s low end — the shoulder chamfer
/// or fillet that makes them steps of one hole, reached directly or one adjacency hop away
/// (`_shared_transition`).
fn shared_transition(part: &Part, a: &Segment, b: &Segment) -> bool {
    let a_partners = end_partners(part, a, a.s_hi);
    let b_partners = end_partners(part, b, b.s_lo);
    for (own, other) in [(&a_partners, &b_partners), (&b_partners, &a_partners)] {
        for &t in own {
            if !matches!(
                part.faces[t].surface,
                Surface::Cone { .. } | Surface::Torus { .. }
            ) {
                continue;
            }
            if other.contains(&t) || part.neighbours(t).iter().any(|n| other.contains(n)) {
                return true;
            }
        }
    }
    false
}

/// Recombine coaxial stacks that are one hole (`_merge_stacks`): equal bores either side of an
/// internal cylinder whose finite cylinder contains the gap (a crossing bore), or different
/// diameters bridged by a shoulder chamfer or fillet.
fn merge_stacks(part: &Part, stacks: Vec<Vec<Segment>>) -> Vec<Vec<Segment>> {
    let original: Vec<Segment> = stacks.iter().flatten().cloned().collect();
    let owner_of = |solid: usize, face: usize| {
        original
            .iter()
            .position(|s| s.solid == solid && s.faces.contains(&face))
    };
    let sources = |seg: &Segment| -> BTreeSet<usize> {
        let mut found = BTreeSet::new();
        for &face in &seg.faces {
            let own = owner_of(seg.solid, face);
            for other in part.neighbours(face) {
                if let Some(index) = owner_of(seg.solid, other) {
                    if Some(index) != own {
                        found.insert(index);
                    }
                }
            }
        }
        found
    };
    let shares_interruption = |a: &Segment, b: &Segment| -> bool {
        let endpoints = [axis_point_at(a, a.s_hi), axis_point_at(b, b.s_lo)];
        let (sa, sb) = (sources(a), sources(b));
        sa.intersection(&sb).any(|&index| {
            let cut = &original[index];
            let tolerance = geom::length_tol(cut.diameter, STACK_GAP_FRAC);
            let radius = cut.diameter / 2.0;
            let radial_tolerance = geom::length_tol(radius, 5e-6);
            endpoints.iter().all(|&point| {
                let axial = py::dot(&point, &cut.direction);
                let centre = axis_point_at(cut, axial);
                cut.s_lo - tolerance <= axial
                    && axial <= cut.s_hi + tolerance
                    && py::dist(&point, &centre) < radius + radial_tolerance
            })
        })
    };
    let mut by_line: Vec<(Vec<f64>, Vec<Vec<Segment>>)> = Vec::new();
    for stack in stacks {
        let key = line_key(&stack[0]);
        match by_line.iter_mut().find(|(k, _)| *k == key) {
            Some((_, group)) => group.push(stack),
            None => by_line.push((key, vec![stack])),
        }
    }
    let mut merged = Vec::new();
    for (_, mut line) in by_line {
        let low = |st: &Vec<Segment>| st.iter().map(|s| s.s_lo).fold(f64::INFINITY, f64::min);
        line.sort_by(|a, b| py::order(low(a), low(b)));
        let mut stacks = line.into_iter();
        let mut cur = stacks.next().expect("a non-empty group");
        for nxt in stacks {
            let ai = py::first_max(&cur, |s| s.s_hi);
            let bi = py::first_min(&nxt, |s| s.s_lo);
            let (a, b) = (&cur[ai], &nxt[bi]);
            let closed = |end: End| matches!(end, End::Flat | End::DrillPoint);
            if same_diameter(a.diameter, b.diameter)
                && !closed(classify_end(part, a, a.s_hi, true))
                && !closed(classify_end(part, b, b.s_lo, false))
                && shares_interruption(a, b)
            {
                let mut joined = a.clone();
                joined.s_hi = b.s_hi;
                joined.faces.extend(&b.faces);
                let mut next: Vec<Segment> = cur
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != ai)
                    .map(|(_, s)| s.clone())
                    .collect();
                next.push(joined);
                next.extend(
                    nxt.iter()
                        .enumerate()
                        .filter(|(i, _)| *i != bi)
                        .map(|(_, s)| s.clone()),
                );
                cur = next;
            } else if b.s_lo - a.s_hi
                <= geom::length_tol(a.diameter.max(b.diameter), STACK_GAP_FRAC)
                    + (a.diameter - b.diameter).abs()
                && shared_transition(part, a, b)
            {
                cur.extend(nxt);
            } else {
                merged.push(std::mem::replace(&mut cur, nxt));
            }
        }
        merged.push(cur);
    }
    merged
}

/// Which end of a stack is the opening and what closes the other (`_drilled_from`). With both
/// ends open the wider segment's end wins (counterbores sit at the opening); a tie falls to the
/// high end.
fn drilled_from(part: &Part, stack: &[Segment]) -> (bool, usize, f64, &'static str) {
    let lo = py::first_min(stack, |s| s.s_lo);
    let hi = py::first_max(stack, |s| s.s_hi);
    let lo_state = classify_end(part, &stack[lo], stack[lo].s_lo, false);
    let hi_state = classify_end(part, &stack[hi], stack[hi].s_hi, true);
    let from_hi = match (lo_state == End::Open, hi_state == End::Open) {
        (true, false) => false,
        (false, true) => true,
        _ => stack[hi].diameter >= stack[lo].diameter,
    };
    let (opening, opening_s, bottom) = if from_hi {
        (hi, stack[hi].s_hi, lo_state)
    } else {
        (lo, stack[lo].s_lo, hi_state)
    };
    let bottom = match bottom {
        End::Open => "through",
        End::Flat => "flat",
        End::DrillPoint => "drill_point",
        End::Unknown => "unknown",
    };
    (from_hi, opening, opening_s, bottom)
}

/// The counterbore and spotface between the opening and the bore (`_near_side_steps`). Steps
/// must narrow inward (a wider one is a groove); lands of one diameter are unioned.
fn near_side_steps(steps: &[Segment]) -> (Option<CounterBore>, Option<CounterBore>, Vec<usize>) {
    let mut spans: Vec<(f64, f64, f64, Vec<usize>)> = Vec::new(); // key, lo, hi, faces
    let mut min_d = f64::INFINITY;
    for step in steps {
        if step.diameter > min_d && !same_diameter(step.diameter, min_d) {
            continue;
        }
        min_d = step.diameter;
        let key = py::quantise(step.diameter, 4);
        match spans.iter_mut().find(|s| s.0 == key) {
            Some(s) => {
                s.1 = s.1.min(step.s_lo);
                s.2 = s.2.max(step.s_hi);
                s.3.extend(&step.faces);
            }
            None => spans.push((key, step.s_lo, step.s_hi, step.faces.clone())),
        }
    }
    let (mut cbore, mut spotface, mut faces) = (None, None, Vec::new());
    for (key, lo, hi, step_faces) in spans {
        let spec = CounterBore {
            diameter: key,
            depth: py::round_to(hi - lo, 2),
        };
        let slot = if spec.depth < SPOTFACE_MAX_RATIO * spec.diameter {
            &mut spotface
        } else {
            &mut cbore
        };
        if slot.is_none() {
            *slot = Some(spec);
            faces.extend(step_faces);
        }
    }
    (cbore, spotface, faces)
}

/// Depth from the top of the bore to the deep end (`_bore_depth`): the deep end is the whole
/// stack for a blind hole (a bottom relief groove counts) but only the bore for a through hole.
fn bore_depth(
    stack: &[Segment],
    bore: &Segment,
    through: bool,
    from_hi: bool,
) -> (f64, Vec<usize>) {
    let bore_segs: Vec<&Segment> = stack
        .iter()
        .filter(|s| same_diameter(s.diameter, bore.diameter))
        .collect();
    let deep: Vec<&Segment> = if through {
        bore_segs.clone()
    } else {
        stack.iter().collect()
    };
    let mut defining: Vec<&Segment> = bore_segs.clone();
    let depth = if from_hi {
        let deep_end = deep.iter().map(|s| s.s_lo).fold(f64::INFINITY, f64::min);
        if !through {
            defining.extend(deep.iter().filter(|s| s.s_lo == deep_end));
        }
        bore_segs
            .iter()
            .map(|s| s.s_hi)
            .fold(f64::NEG_INFINITY, f64::max)
            - deep_end
    } else {
        let deep_end = deep
            .iter()
            .map(|s| s.s_hi)
            .fold(f64::NEG_INFINITY, f64::max);
        if !through {
            defining.extend(deep.iter().filter(|s| s.s_hi == deep_end));
        }
        deep_end
            - bore_segs
                .iter()
                .map(|s| s.s_lo)
                .fold(f64::INFINITY, f64::min)
    };
    (
        depth,
        defining
            .iter()
            .flat_map(|s| s.faces.iter().copied())
            .collect(),
    )
}

/// Holes in stack order, each with its defining cylinder faces; *csinks* are composed onto the
/// holes they flare.
pub fn discover(ctx: &Context<'_>, csinks: &[CounterSink]) -> Vec<Occurrence<HoleRecord>> {
    let part = ctx.part;
    let cyls = ctx.cylinders();
    let (z, cross): (Vec<_>, Vec<_>) = cyls.iter().cloned().partition(|c| c.axis == 2);
    let internal: Vec<_> = full_cylinders(&z)
        .into_iter()
        .chain(full_cylinders(&cross))
        .filter(|c| !c.external)
        .collect();
    if internal.is_empty() {
        return Vec::new();
    }
    let stacks = merge_stacks(part, merge_runs(&segments(&internal), line_key));
    let mut out = Vec::new();
    for stack in stacks {
        let d = stack[0].direction;
        let (from_hi, opening, opening_s, bottom) = drilled_from(part, &stack);
        let mut ordered = stack.clone();
        // A stable sort by s_hi, descending when drilled from the high end.
        ordered.sort_by(|a, b| {
            // Python's reverse=True keeps equal keys in input order, as reversing does here.
            let o = py::order(a.s_hi, b.s_hi);
            if from_hi { o.reverse() } else { o }
        });
        let bore_i = py::first_min(&ordered, |s| s.diameter);
        let bore = &ordered[bore_i];
        let (cbore, spotface, step_faces) = near_side_steps(&ordered[..bore_i]);
        let (depth, mut defining) = bore_depth(&stack, bore, bottom == "through", from_hi);
        defining.extend(step_faces);
        let axis = if from_hi { geom::scale(d, -1.0) } else { d }.map(py::without_negative_zero);
        let location = axis_point_at(&stack[opening], opening_s).map(|c| py::quantise(c, 11));
        let mut record = HoleRecord {
            axis,
            location,
            diameter: bore.diameter,
            depth: py::round_to(depth, 2),
            bottom: bottom.to_owned(),
            cbore,
            spotface,
            csink: None,
        };
        let mouth = HoleMouth {
            axis: record.axis,
            location: record.location,
            diameter: record.diameter,
            depth: record.depth,
            through: record.bottom == "through",
        };
        record.csink = csinks
            .iter()
            .find(|cs| countersink_matches_hole(cs, &mouth))
            .cloned();
        out.push(Occurrence {
            record,
            defining,
            context: Vec::new(),
        });
    }
    out
}
