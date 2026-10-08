//! Horizontal face levels and step risers (`quiddity.levels`): the Z levels of a prismatic
//! part's horizontal planes, and the walls and ramps that rise between them, each found within
//! one solid so that support never bridges two bodies.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::Context;
use super::body::BodyKey;
use crate::kernel::brep::Part;
use crate::kernel::geom::{
    AXIS_ALIGNED_COS, AXIS_ZERO_COS, Bounds, Surface, clears_threshold, cluster_coordinates,
};
use crate::kernel::py;

/// Band within which two horizontal faces are one level (absolute, ADR 0008).
const TOL: f64 = 0.5;
/// A level whose faces cover less of the plan footprint is an incidental face, not a step.
const STEP_MIN_AREA_FRAC: f64 = 0.01;
/// A bounded slanted face is a structural ramp only at this fraction of the part on each axis.
const STRUCTURAL_RAMP_MIN_FRAC: f64 = 0.1;
/// A bounded riser must fill this fraction of its own footprint.
const BOUNDED_RISER_AREA_FRAC: f64 = 0.5;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FaceLevel {
    pub z: f64,
    pub x_span: Option<(f64, f64)>,
    pub y_span: Option<(f64, f64)>,
    pub body_key: Option<BodyKey>,
}

impl FaceLevel {
    /// Python's `_order_key` comparison.
    fn order(&self, other: &Self) -> Ordering {
        let span = |s: &Option<(f64, f64)>| s.map(|(a, b)| vec![a, b]).unwrap_or_default();
        py::order(self.z, other.z)
            .then(self.x_span.is_some().cmp(&other.x_span.is_some()))
            .then_with(|| py::tuple_order(&span(&self.x_span), &span(&other.x_span)))
            .then(self.y_span.is_some().cmp(&other.y_span.is_some()))
            .then_with(|| py::tuple_order(&span(&self.y_span), &span(&other.y_span)))
            .then(self.body_key.is_some().cmp(&other.body_key.is_some()))
            .then_with(|| key_order(&self.body_key, &other.body_key))
    }
}

fn key_order(a: &Option<BodyKey>, b: &Option<BodyKey>) -> Ordering {
    py::tuple_order(a.as_deref().unwrap_or(&[]), b.as_deref().unwrap_or(&[]))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RiserEvidence {
    pub vertical: bool,
    pub axis: char,
    pub positions: Vec<f64>,
    pub other_axis: char,
    pub other_positions: Vec<f64>,
    pub z_lo: f64,
    pub z_hi: f64,
    pub lo_at_envelope: bool,
    pub hi_at_envelope: bool,
    pub tol: f64,
    pub body_levels: Vec<FaceLevel>,
    pub body_key: Option<BodyKey>,
}

impl RiserEvidence {
    /// Python's `_order_key` comparison (recogniser records always carry `body_levels`).
    fn order(&self, other: &Self) -> Ordering {
        self.vertical
            .cmp(&other.vertical)
            .then(self.axis.cmp(&other.axis))
            .then_with(|| py::tuple_order(&self.positions, &other.positions))
            .then(self.other_axis.cmp(&other.other_axis))
            .then_with(|| py::tuple_order(&self.other_positions, &other.other_positions))
            .then(py::order(self.z_lo, other.z_lo))
            .then(py::order(self.z_hi, other.z_hi))
            .then(self.lo_at_envelope.cmp(&other.lo_at_envelope))
            .then(self.hi_at_envelope.cmp(&other.hi_at_envelope))
            .then(py::order(self.tol, other.tol))
            .then_with(|| {
                // Tuples of levels: element by element, then by length.
                self.body_levels
                    .iter()
                    .zip(&other.body_levels)
                    .map(|(a, b)| a.order(b))
                    .find(|o| o.is_ne())
                    .unwrap_or(self.body_levels.len().cmp(&other.body_levels.len()))
            })
            .then(self.body_key.is_some().cmp(&other.body_key.is_some()))
            .then_with(|| key_order(&self.body_key, &other.body_key))
    }
}

/// Options named as `recognise_face_levels`' keyword arguments.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct FaceLevelOptions {
    pub tol: Option<f64>,
    pub min_area_frac: f64,
}

/// Options named as `recognise_risers`' keyword arguments.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RiserOptions {
    pub min_area_frac: f64,
    pub tol: Option<f64>,
}

impl Default for RiserOptions {
    fn default() -> Self {
        RiserOptions {
            min_area_frac: 0.15,
            tol: None,
        }
    }
}

/// One body the levels are read within: a solid, or the whole part when it has none.
struct Scope {
    faces: Vec<usize>,
    bounds: Bounds,
    key: Option<BodyKey>,
}

fn scopes(ctx: &Context<'_>) -> Vec<Scope> {
    let part = ctx.part;
    if part.solids.is_empty() {
        return vec![Scope {
            faces: (0..part.faces.len()).collect(),
            bounds: part.bounds(),
            key: None,
        }];
    }
    let keys = ctx.body_keys(true);
    part.solids
        .iter()
        .enumerate()
        .map(|(s, solid)| Scope {
            faces: solid.faces.clone(),
            bounds: part.solid_bounds(s),
            key: keys[s].clone(),
        })
        .collect()
}

/// The levels of one scope with the faces supporting each (`_face_level_proposals_one`).
fn level_proposals(
    part: &Part,
    scope: &Scope,
    tol: f64,
    min_area_frac: f64,
) -> Vec<(FaceLevel, Vec<usize>)> {
    let mut zs = Vec::new();
    let mut faces = Vec::new();
    for &f in &scope.faces {
        if let Surface::Plane { frame } = part.faces[f].surface
            && frame.z[2].abs() > AXIS_ALIGNED_COS
        {
            zs.push(frame.origin[2]);
            faces.push(f);
        }
    }
    let b = &scope.bounds;
    let threshold = min_area_frac * (b.max[0] - b.min[0]) * (b.max[1] - b.min[1]);
    let area = |f: usize| part.face_mass(f).map_or(0.0, |m| m[0]);
    let mut out = Vec::new();
    for cluster in cluster_coordinates(&zs, tol) {
        if min_area_frac > 0.0
            && !clears_threshold(py::sum(cluster.iter().map(|&i| area(faces[i]))), threshold)
        {
            continue;
        }
        let spans: Vec<Bounds> = cluster
            .iter()
            .map(|&i| part.face_bounds(faces[i]))
            .collect();
        // Python's `min`/`max`: the first of equals wins, which decides a zero's sign.
        let fold = |pick: fn(&Bounds) -> f64, max: bool| {
            let mut it = spans.iter().map(pick);
            let first = it.next().unwrap();
            it.fold(first, |a, x| {
                let better = if max { x > a } else { x < a };
                if better { x } else { a }
            })
        };
        out.push((
            FaceLevel {
                z: zs[cluster[py::first_min(&cluster, |&i| zs[i])]],
                x_span: Some((fold(|b| b.min[0], false), fold(|b| b.max[0], true))),
                y_span: Some((fold(|b| b.min[1], false), fold(|b| b.max[1], true))),
                body_key: scope.key.clone(),
            },
            cluster.iter().map(|&i| faces[i]).collect(),
        ));
    }
    out
}

/// `recognise_face_levels`.
pub fn recognise_face_levels(part: &Part, opts: &FaceLevelOptions) -> Vec<FaceLevel> {
    face_levels_with_faces(part, opts)
        .into_iter()
        .map(|(level, _)| level)
        .collect()
}

/// `recognise_face_levels`' levels, each with the faces it was read from (not an evidence
/// path: the invariance test compares them).
pub fn face_levels_with_faces(
    part: &Part,
    opts: &FaceLevelOptions,
) -> Vec<(FaceLevel, Vec<usize>)> {
    let ctx = Context::new(part);
    let tol = opts.tol.unwrap_or(TOL);
    let mut out: Vec<(FaceLevel, Vec<usize>)> = scopes(&ctx)
        .iter()
        .flat_map(|scope| level_proposals(part, scope, tol, opts.min_area_frac))
        .collect();
    out.sort_by(|a, b| a.0.order(&b.0));
    out
}

/// The scope's interior step levels: area-filtered, strictly inside its height by *tol*.
fn body_levels(part: &Part, scope: &Scope, tol: f64) -> Vec<FaceLevel> {
    let (lo, hi) = (scope.bounds.min[2], scope.bounds.max[2]);
    level_proposals(part, scope, TOL, STEP_MIN_AREA_FRAC)
        .into_iter()
        .map(|(level, _)| level)
        .filter(|level| lo + tol < level.z && level.z < hi - tol)
        .collect()
}

/// `recognise_risers`.
pub fn recognise_risers(part: &Part, opts: &RiserOptions) -> Vec<RiserEvidence> {
    risers_with_faces(part, opts)
        .into_iter()
        .map(|(riser, _)| riser)
        .collect()
}

/// `recognise_risers`' risers, each with the faces it was read from (not an evidence path: the
/// invariance test compares them).
pub fn risers_with_faces(part: &Part, opts: &RiserOptions) -> Vec<(RiserEvidence, Vec<usize>)> {
    let ctx = Context::new(part);
    let tol = opts.tol.unwrap_or(TOL);
    let mut out: Vec<(RiserEvidence, Vec<usize>)> = scopes(&ctx)
        .iter()
        .flat_map(|scope| {
            let levels = body_levels(part, scope, tol);
            riser_proposals(part, scope, opts.min_area_frac, tol, levels)
        })
        .collect();
    out.sort_by(|a, b| a.0.order(&b.0));
    out
}

/// A riser's facing: `(vertical, axis)` when the normal lies along exactly one in-plane axis
/// (`_riser_orientation`).
fn riser_orientation(n: [f64; 3]) -> Option<(bool, usize)> {
    let on_x = n[0].abs() > AXIS_ZERO_COS && n[1].abs() <= AXIS_ZERO_COS;
    let on_y = n[1].abs() > AXIS_ZERO_COS && n[0].abs() <= AXIS_ZERO_COS;
    if !(on_x || on_y) {
        return None;
    }
    Some((n[2].abs() <= AXIS_ZERO_COS, if on_x { 0 } else { 1 }))
}

/// The risers of one scope, equal records merged with all their faces (`_riser_proposals_one`).
fn riser_proposals(
    part: &Part,
    scope: &Scope,
    min_area_frac: f64,
    tol: f64,
    body_levels: Vec<FaceLevel>,
) -> Vec<(RiserEvidence, Vec<usize>)> {
    let bb = &scope.bounds;
    let ext = |i: usize| bb.max[i] - bb.min[i];
    let mut found: Vec<(RiserEvidence, Vec<usize>)> = Vec::new();
    for &f in &scope.faces {
        let Surface::Plane { frame } = part.faces[f].surface else {
            continue;
        };
        let Some(normal) = part.face_normal(f, 0.0, 0.0) else {
            continue;
        };
        let Some((vertical, axis)) = riser_orientation(normal) else {
            continue;
        };
        let fb = part.face_bounds(f);
        let other = 1 - axis;
        let (flo, fhi) = (fb.min[other], fb.max[other]);
        // A step crosses its whole body on the other in-plane axis; a pad's or a blind
        // pocket's walls are bounded.
        let full_span = flo <= bb.min[other] + tol && fhi >= bb.max[other] - tol;
        let interior = |i: usize, p: f64| bb.min[i] + tol < p && p < bb.max[i] - tol;
        let height = fb.max[2] - fb.min[2];
        let (positions, other_positions) = if vertical {
            let pos = frame.origin[axis];
            if !full_span || !interior(axis, pos) {
                continue;
            }
            (vec![pos], vec![])
        } else {
            if tol >= height {
                continue;
            }
            let along = vec![fb.min[axis], fb.max[axis]];
            if full_span {
                (along, vec![])
            } else if fhi - flo < STRUCTURAL_RAMP_MIN_FRAC * ext(other)
                || along[1] - along[0] < STRUCTURAL_RAMP_MIN_FRAC * ext(axis)
                || STRUCTURAL_RAMP_MIN_FRAC * ext(2) > height
            {
                continue; // an edge-break chamfer, not a structural ramp
            } else {
                (along, vec![flo, fhi])
            }
        };
        let cross = ext(other) * ext(2);
        let area_floor = if full_span {
            min_area_frac * cross
        } else {
            BOUNDED_RISER_AREA_FRAC * ((fhi - flo) * height)
        };
        let Some([area, _]) = part.face_mass(f) else {
            continue;
        };
        if cross <= 0.0 || area < area_floor {
            continue;
        }
        let at_envelope = |z: f64| (z - bb.min[2]).abs() < tol || (z - bb.max[2]).abs() < tol;
        let record = RiserEvidence {
            vertical,
            axis: ['x', 'y'][axis],
            positions: positions
                .into_iter()
                .filter(|&p| interior(axis, p))
                .map(py::round_to3)
                .collect(),
            other_axis: ['x', 'y'][other],
            other_positions: other_positions
                .into_iter()
                .filter(|&p| interior(other, p))
                .map(py::round_to3)
                .collect(),
            z_lo: fb.min[2],
            z_hi: fb.max[2],
            lo_at_envelope: at_envelope(fb.min[2]),
            hi_at_envelope: at_envelope(fb.max[2]),
            tol,
            body_levels: body_levels.clone(),
            body_key: scope.key.clone(),
        };
        match found.iter_mut().find(|(r, _)| *r == record) {
            Some((_, faces)) => faces.push(f),
            None => found.push((record, vec![f])),
        }
    }
    found.sort_by(|a, b| a.0.order(&b.0));
    found
}
