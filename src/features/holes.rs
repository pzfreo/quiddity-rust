//! Drilled-hole recognition (`quiddity.holes`).
//!
//! Coaxial internal cylinders are grouped into stacks — drill, optional counterbore and
//! optional spotface become one hole, and a bore interrupted by a crossing hole is recombined.
//! Each stack's ends are classified from the faces beyond them, which decides the opening, the
//! bottom and the depth.
//!
//! Python's default inventory retries with `local_degradation` set when this family's strict
//! evidence refuses on a part whose solids are not all valid; three corpus parts take it
//! (`tests/local_degradation.rs`). [`discover_locally_degraded`] is that run's evidence path: a
//! hole proving no one valid solid is skipped, not refused. Not ported: the retry itself (the
//! port's `recognise` checks no evidence, so has no refusal to retry on) and the degraded run's
//! admission of an invalid solid with a small bad-face region (the kernel finds no bad faces).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::Context;
use super::countersinks::{self, CounterSink, HoleMouth, countersink_matches_hole};
use super::cylinders::{
    STACK_GAP_FRAC, Segment, axis_point_at, full_cylinders, line_key, merge_runs, segments,
    z_then_cross,
};
use super::evidence::{self, EvidenceError, Occurrence};
use super::policy;
use super::stacks::{End, classify_end, end_partners};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, Surface, V3};
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

/// What closes a hole's deep end.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bottom {
    Through,
    Flat,
    DrillPoint,
    /// The adjacent geometry matches none of the others.
    Unknown,
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
    pub bottom: Bottom,
    pub cbore: Option<CounterBore>,
    pub spotface: Option<CounterBore>,
    pub csink: Option<CounterSink>,
}

/// Options named as `recognise_holes`' keyword arguments.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct HoleOptions {
    /// Compose recognised countersinks onto the holes they flare: Python's
    /// `csinks=recognise_countersinks(part)`, spelled `"csinks": "auto"`. Off by default, as
    /// there.
    #[serde(rename = "csinks", deserialize_with = "auto_flag")]
    pub with_countersinks: bool,
}

/// `null` → off, `"auto"` → on.
fn auto_flag<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    match Option::<String>::deserialize(d)?.as_deref() {
        None => Ok(false),
        Some("auto") => Ok(true),
        Some(other) => Err(serde::de::Error::custom(format!(
            "csinks must be \"auto\" or null, not {other:?}"
        ))),
    }
}

/// The evidence path: holes with their defining cylinder faces and the faces that close them,
/// published only if each lies in one valid solid and no countersink seats two holes.
pub fn discover_verified(
    ctx: &Context<'_>,
    csinks: &[Occurrence<CounterSink>],
) -> Result<Vec<Occurrence<HoleRecord>>, EvidenceError> {
    let found = discover(ctx, csinks);
    seats_once(csinks, &found)?;
    evidence::verified(ctx.part, found)
}

/// The evidence path as Python's locally degraded run takes it (`_discover_holes` on a graph
/// with `local_degradation` set): a hole whose cylinder and closing faces prove no one valid
/// solid is skipped instead of refusing every hole, and the countersink check is made over the
/// holes kept. Python's degraded run also admits an invalid solid with at most three bad faces,
/// skipping only what touches them; the kernel's validity is topological and whole-solid, with
/// no bad-face region, so here a hole on an invalid solid is always skipped. The seat face of a
/// composed countersink is proved with the hole, so a hole whose seat lies on another solid is
/// skipped, where Python refuses it (no captured call or corpus part has one).
pub fn discover_locally_degraded(
    ctx: &Context<'_>,
    csinks: &[Occurrence<CounterSink>],
) -> Result<Vec<Occurrence<HoleRecord>>, EvidenceError> {
    let mut found = discover(ctx, csinks);
    found.retain(|h| {
        let faces: Vec<usize> = h.defining.iter().chain(&h.context).copied().collect();
        !h.defining.is_empty() && evidence::common_valid_solid(ctx.part, &faces).is_some()
    });
    seats_once(csinks, &found)?;
    Ok(found)
}

/// No countersink may seat two holes.
fn seats_once(
    csinks: &[Occurrence<CounterSink>],
    found: &[Occurrence<HoleRecord>],
) -> Result<(), EvidenceError> {
    for cs in csinks {
        let seats = found
            .iter()
            .filter(|h| cs.defining.iter().all(|f| h.context.contains(f)))
            .count();
        if seats > 1 {
            return Err(EvidenceError::SharedEvidence);
        }
    }
    Ok(())
}

/// `recognise_holes`.
pub fn recognise_holes(part: &Part, opts: &HoleOptions) -> Vec<HoleRecord> {
    let ctx = Context::new(part);
    let csinks = if opts.with_countersinks {
        countersinks::discover(&ctx)
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
                if let Some(index) = owner_of(seg.solid, other)
                    && Some(index) != own
                {
                    found.insert(index);
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
            let tolerance = policy::length_tol(cut.diameter, STACK_GAP_FRAC);
            let radius = cut.diameter / 2.0;
            let radial_tolerance = policy::length_tol(radius, 5e-6);
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
                && !closed(classify_end(part, a, a.s_hi, true).0)
                && !closed(classify_end(part, b, b.s_lo, false).0)
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
                <= policy::length_tol(a.diameter.max(b.diameter), STACK_GAP_FRAC)
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
/// high end, so a stack symmetric end to end reads from whichever end is high in world space.
fn drilled_from(part: &Part, stack: &[Segment]) -> (bool, usize, f64, Bottom, Vec<usize>) {
    let lo = end_segment(stack, |s| -s.s_lo);
    let hi = end_segment(stack, |s| s.s_hi);
    let (lo_state, lo_faces) = classify_end(part, &stack[lo], stack[lo].s_lo, false);
    let (hi_state, hi_faces) = classify_end(part, &stack[hi], stack[hi].s_hi, true);
    let from_hi = match (lo_state == End::Open, hi_state == End::Open) {
        (true, false) => false,
        (false, true) => true,
        _ => stack[hi].diameter >= stack[lo].diameter,
    };
    let (opening, opening_s, bottom, faces) = if from_hi {
        (hi, stack[hi].s_hi, lo_state, lo_faces)
    } else {
        (lo, stack[lo].s_lo, hi_state, hi_faces)
    };
    let bottom = match bottom {
        End::Open => Bottom::Through,
        End::Flat => Bottom::Flat,
        End::DrillPoint => Bottom::DrillPoint,
        End::Unknown => Bottom::Unknown,
    };
    // Only a closing face is evidence of the bottom.
    let terminal = if matches!(bottom, Bottom::Flat | Bottom::DrillPoint) {
        faces
    } else {
        vec![]
    };
    (from_hi, opening, opening_s, bottom, terminal)
}

/// The segment that forms one end of a stack: the one reaching furthest by *reach*, and of
/// those reaching it (within COORD_FLOOR) the widest, which is the mouth. Python takes the first
/// in stack order, which is face order, so turning the part over can pick a different segment.
fn end_segment(stack: &[Segment], reach: impl Fn(&Segment) -> f64) -> usize {
    let far = stack.iter().map(&reach).fold(f64::NEG_INFINITY, f64::max);
    let mut best: Option<usize> = None;
    for (i, s) in stack.iter().enumerate() {
        if far - reach(s) <= COORD_FLOOR && best.is_none_or(|b| s.diameter > stack[b].diameter) {
            best = Some(i);
        }
    }
    best.expect("a stack has segments")
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
pub fn discover(
    ctx: &Context<'_>,
    csinks: &[Occurrence<CounterSink>],
) -> Vec<Occurrence<HoleRecord>> {
    let part = ctx.part;
    let cyls = ctx.cylinders();
    let (z, cross) = z_then_cross(cyls);
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
        let (from_hi, opening, opening_s, bottom, mut terminal) = drilled_from(part, &stack);
        let mut ordered = stack.clone();
        // Nearest the opening first: by the near end of each segment, which is s_hi drilling
        // down from the high end and s_lo drilling up from the low end. (Python sorts by s_hi
        // both ways, so overlapping segments order differently once the part is turned over.)
        // Segments starting level with each other put the wider first, as the mouth is outermost
        // (Python keeps them in face order). The sort is stable.
        ordered.sort_by(|a, b| {
            let near = if from_hi {
                py::order(a.s_hi, b.s_hi).reverse()
            } else {
                py::order(a.s_lo, b.s_lo)
            };
            let level = if from_hi {
                a.s_hi - b.s_hi
            } else {
                a.s_lo - b.s_lo
            };
            if level.abs() <= COORD_FLOOR {
                py::order(a.diameter, b.diameter).reverse()
            } else {
                near
            }
        });
        let bore_i = py::first_min(&ordered, |s| s.diameter);
        let bore = &ordered[bore_i];
        let (cbore, spotface, step_faces) = near_side_steps(&ordered[..bore_i]);
        let (depth, mut defining) = bore_depth(&stack, bore, bottom == Bottom::Through, from_hi);
        defining.extend(step_faces);
        // A face counts once however many selections name it (Python resolves them to a set).
        let mut seen = std::collections::BTreeSet::new();
        defining.retain(|f| seen.insert(*f));
        let axis = if from_hi { geom::scale(d, -1.0) } else { d }.map(py::without_negative_zero);
        let location = axis_point_at(&stack[opening], opening_s).map(|c| py::quantise(c, 11));
        let mut record = HoleRecord {
            axis,
            location,
            diameter: bore.diameter,
            depth: py::round_to(depth, 2),
            bottom,
            cbore,
            spotface,
            csink: None,
        };
        let mouth = HoleMouth {
            axis: record.axis,
            location: record.location,
            diameter: record.diameter,
            depth: record.depth,
            through: record.bottom == Bottom::Through,
        };
        if let Some(cs) = csinks
            .iter()
            .find(|cs| countersink_matches_hole(&cs.record, &mouth))
        {
            record.csink = Some(cs.record.clone());
            // The seat's face is consulted evidence: it must share the hole's solid.
            terminal.extend(&cs.defining);
        }
        out.push(Occurrence {
            record,
            defining,
            context: terminal,
        });
    }
    out
}
