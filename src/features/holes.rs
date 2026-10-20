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
//! hole proving no one valid solid is skipped, not refused. The aggregate's inventory takes the
//! retry where `strict_refusal_retries` says Python's would (`super::inventory_in`).

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
/// `"drill_point"` or `"unknown"`. For interchangeable mouths, axis/location are a
/// presentation end gauge on the bore itself; the bore is retained, and unproved one-sided
/// counterbore/spotface attribution is absent. No material drilling-entry end is asserted.
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
/// holes kept. Python's degraded run also admits a solid with at most three faulted faces,
/// skipping only the holes that touch them ([`evidence::locally_valid_solid`]). The seat face of a
/// composed countersink is proved with the hole, so a hole whose seat lies on another solid is
/// skipped, where Python refuses it (no captured call or corpus part has one).
pub fn discover_locally_degraded(
    ctx: &Context<'_>,
    csinks: &[Occurrence<CounterSink>],
) -> Result<Vec<Occurrence<HoleRecord>>, EvidenceError> {
    let mut found = discover(ctx, csinks);
    found.retain(|h| evidence::locally_proved(ctx.part, h));
    seats_once(csinks, &found)?;
    Ok(found)
}

/// Whether the aggregate retries locally degraded on these holes (`_take_inventory`): Python's
/// strict hole evidence refuses for want of one valid solid (countersinks seating one hole each,
/// [`discover_verified`]'s order), and the part's solids are not all valid
/// (`BRepCheck_Analyzer(solid).IsValid()`, so faulted faces count; a part with no solid
/// retries too).
pub(crate) fn strict_refusal_retries(
    part: &Part,
    csinks: &[Occurrence<CounterSink>],
    found: &[Occurrence<HoleRecord>],
) -> bool {
    let unproved = |h: &Occurrence<HoleRecord>| {
        let faces: Vec<usize> = h.defining.iter().chain(&h.context).copied().collect();
        h.defining.is_empty() || evidence::common_valid_solid(part, &faces).is_none()
    };
    let all_valid = !part.solids.is_empty()
        && (0..part.solids.len()).all(|s| part.solid_is_geometrically_valid(s));
    seats_once(csinks, found).is_ok() && found.iter().any(unproved) && !all_valid
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
            let ai = end_segment(&cur, |s| s.s_hi);
            let bi = end_segment(&nxt, |s| -s.s_lo);
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

/// Which end is the opening. Mouth diameter, then the complete inward axial profile decide
/// intrinsically. An unresolved side is an end gauge: retain the bore, without attributing
/// one-sided steps to either mouth.
struct Drilling {
    from_hi: bool,
    opening: usize,
    opening_s: f64,
    bottom: Bottom,
    terminal: Vec<usize>,
    presentation: bool,
}

fn drilled_from(part: &Part, stack: &[Segment]) -> Drilling {
    let lo = end_segment(stack, |s| -s.s_lo);
    let hi = end_segment(stack, |s| s.s_hi);
    let (lo_state, lo_faces) = classify_end(part, &stack[lo], stack[lo].s_lo, false);
    let (hi_state, hi_faces) = classify_end(part, &stack[hi], stack[hi].s_hi, true);
    let (from_hi, presentation) = match (lo_state == End::Open, hi_state == End::Open) {
        (true, false) => (false, false),
        (false, true) => (true, false),
        _ if lo == hi => (true, true),
        _ => match mouth_order(stack, lo, hi) {
            Some(side) => (side, false),
            None => (true, true),
        },
    };
    let gauge_faces = if presentation {
        let mut faces: Vec<_> = lo_faces.iter().chain(&hi_faces).copied().collect();
        faces.sort_unstable();
        faces.dedup();
        faces
    } else {
        vec![]
    };
    let (opening, opening_s, bottom, faces) = if from_hi {
        (hi, stack[hi].s_hi, lo_state, lo_faces)
    } else {
        (lo, stack[lo].s_lo, hi_state, hi_faces)
    };
    let bottom = match bottom {
        _ if presentation && lo_state != hi_state => Bottom::Unknown,
        End::Open => Bottom::Through,
        End::Flat => Bottom::Flat,
        End::DrillPoint => Bottom::DrillPoint,
        End::Unknown => Bottom::Unknown,
    };
    // Only a closing face is evidence of the bottom.
    let terminal = if matches!(bottom, Bottom::Flat | Bottom::DrillPoint) {
        if presentation { gauge_faces } else { faces }
    } else {
        vec![]
    };
    Drilling {
        from_hi,
        opening,
        opening_s,
        bottom,
        terminal,
        presentation,
    }
}

/// Compare the two inward profiles using only distances and diameters. Source support signs,
/// face indices and the input order carry no machining-side evidence. Equal profiles leave
/// one-sided attribution unresolved, while retaining the bore.
fn mouth_order(stack: &[Segment], lo: usize, hi: usize) -> Option<bool> {
    let profile = |from_hi: bool| {
        let mut values: Vec<[f64; 3]> = stack
            .iter()
            .map(|s| {
                let (near, far) = if from_hi {
                    (stack[hi].s_hi - s.s_hi, stack[hi].s_hi - s.s_lo)
                } else {
                    (s.s_lo - stack[lo].s_lo, s.s_hi - stack[lo].s_lo)
                };
                [near, -s.diameter, -far]
            })
            .collect();
        values.sort_by(|a, b| py::tuple_order(a, b));
        values
    };
    let (low, high) = (profile(false), profile(true));
    for (a, b) in low.iter().zip(&high) {
        for (a, b) in a.iter().zip(b) {
            if (a - b).abs() > COORD_FLOOR {
                return Some(b < a);
            }
        }
    }
    None
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
            depth: policy::round_measurement(hi - lo, 2),
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
        let Drilling {
            from_hi,
            opening,
            opening_s,
            bottom,
            mut terminal,
            presentation,
        } = drilled_from(part, &stack);
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
        let (cbore, spotface, step_faces) = if presentation {
            (None, None, vec![])
        } else {
            near_side_steps(&ordered[..bore_i])
        };
        let (depth, mut defining) = bore_depth(
            &stack,
            bore,
            bottom == Bottom::Through || presentation,
            from_hi,
        );
        defining.extend(step_faces);
        // A face counts once however many selections name it (Python resolves them to a set).
        let mut seen = std::collections::BTreeSet::new();
        defining.retain(|f| seen.insert(*f));
        let axis = if from_hi { geom::scale(d, -1.0) } else { d }.map(py::without_negative_zero);
        let location = if presentation {
            axis_point_at(bore, if from_hi { bore.s_hi } else { bore.s_lo })
        } else {
            axis_point_at(&stack[opening], opening_s)
        }
        .map(|c| py::quantise(c, 11));
        let mut record = HoleRecord {
            axis,
            location,
            diameter: bore.diameter,
            depth: policy::round_measurement(depth, 2),
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
        let mut matching: Vec<_> = csinks
            .iter()
            .filter(|cs| countersink_matches_hole(&cs.record, &mouth))
            .collect();
        let rank = |cs: &Occurrence<CounterSink>| {
            [
                cs.record.major_diameter,
                cs.record.depth,
                cs.record.included_angle,
            ]
        };
        matching.sort_by(|a, b| py::tuple_order(&rank(b), &rank(a)));
        let ambiguous_seat = matching.len() > 1
            && rank(matching[0])
                .iter()
                .zip(rank(matching[1]))
                .all(|(a, b)| (a - b).abs() <= COORD_FLOOR);
        // Ambiguous attribution cannot remove the independently established bore.
        if let Some(cs) = matching.first().filter(|_| !ambiguous_seat) {
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
