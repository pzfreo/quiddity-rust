//! Paired ramp steps (`quiddity.paired_ramp_steps`): two oblique planar ramps meeting in a
//! concave valley along one axis, closed by an internal flat at one end and open to the part's
//! exterior at the other.

use serde::Serialize;

use super::Context;
use super::evidence::{self, EvidenceError, Occurrence};
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{AXIS_ALIGNED_COS, Curve, Surface, V3};
use crate::kernel::py;

/// Normal components at or below this are zero (`SMOOTH_ARC_GAP`).
const SMOOTH_ARC_GAP: f64 = 1e-9;
/// The shared valley edge must run along the ramps' axis this closely.
const RUN_DIRECTION_COS: f64 = 1.0 - SMOOTH_ARC_GAP;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PairedRampStep {
    pub axis: char,
    pub angle: f64,
    pub length: f64,
    pub at: V3,
    pub opening_direction: V3,
    pub half_width: f64,
    /// Each side's width, low side first, when the two ramps differ.
    pub half_widths: Option<(f64, f64)>,
}

/// `recognise_paired_ramp_steps`.
pub fn recognise_paired_ramp_steps(part: &Part) -> Vec<PairedRampStep> {
    super::records(discover(&Context::new(part)))
}

/// The evidence path: both ramps and the internal terminal define each step.
pub fn discover_verified(
    ctx: &Context<'_>,
) -> Result<Vec<Occurrence<PairedRampStep>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}

/// A planar face running along exactly one axis: (axis, normal, per-axis spans).
struct Ramp {
    axis: usize,
    normal: V3,
    spans: [(f64, f64); 3],
}

fn read_ramp(part: &Part, face: usize) -> Option<Ramp> {
    let Surface::Plane { .. } = part.faces[face].surface else {
        return None;
    };
    let normal = part.face_normal(face, 0.0, 0.0)?;
    let run: Vec<usize> = (0..3)
        .filter(|&i| normal[i].abs() <= SMOOTH_ARC_GAP)
        .collect();
    let [axis] = run[..] else { return None };
    let b = part.face_bounds(face);
    Some(Ramp {
        axis,
        normal,
        spans: [0, 1, 2].map(|i| (b.min[i], b.max[i])),
    })
}

fn mid(span: (f64, f64)) -> f64 {
    0.5 * (span.0 + span.1)
}

pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<PairedRampStep>> {
    let part = ctx.part;
    let ramps: Vec<Option<Ramp>> = (0..part.faces.len()).map(|f| read_ramp(part, f)).collect();
    let mut found = Vec::new();
    for left in 0..part.faces.len() {
        let Some(left_read) = &ramps[left] else {
            continue;
        };
        for right in part.neighbours(left) {
            if right <= left {
                continue;
            }
            let Some(right_read) = &ramps[right] else {
                continue;
            };
            if let Some(o) = candidate(part, left, right, left_read, right_read) {
                found.push(o);
            }
        }
    }
    found.sort_by(|a, b| order(&a.record, &b.record));
    found
}

/// The dataclass's field order (`order=True`).
fn order(a: &PairedRampStep, b: &PairedRampStep) -> std::cmp::Ordering {
    a.axis
        .cmp(&b.axis)
        .then(py::order(a.angle, b.angle))
        .then(py::order(a.length, b.length))
        .then_with(|| py::tuple_order(&a.at, &b.at))
        .then_with(|| py::tuple_order(&a.opening_direction, &b.opening_direction))
        .then(py::order(a.half_width, b.half_width))
}

fn candidate(
    part: &Part,
    left: usize,
    right: usize,
    l: &Ramp,
    r: &Ramp,
) -> Option<Occurrence<PairedRampStep>> {
    if r.axis != l.axis {
        return None;
    }
    let axis = l.axis;
    let cross: Vec<usize> = (0..3).filter(|&i| i != axis).collect();
    let opposed: Vec<usize> = cross
        .iter()
        .copied()
        .filter(|&i| l.normal[i] * r.normal[i] < 0.0)
        .collect();
    let same: Vec<usize> = cross
        .iter()
        .copied()
        .filter(|&i| l.normal[i] * r.normal[i] > 0.0)
        .collect();
    let (&[opposed], &[same]) = (&opposed[..], &same[..]) else {
        return None;
    };
    if cross
        .iter()
        .any(|&i| (l.normal[i].abs() - r.normal[i].abs()).abs() > SMOOTH_ARC_GAP)
    {
        return None;
    }
    let shared = part.shared_edges(left, right);
    let [edge] = shared[..] else { return None };
    let ed = &part.edges[edge];
    let Curve::Line { .. } = ed.curve else {
        return None;
    };
    let tangent = py::unit(crate::kernel::geom::sub(ed.end, ed.start))?;
    if tangent[axis].abs() < RUN_DIRECTION_COS || part.arc(left, right) != Some(Arc::Concave) {
        return None;
    }
    let right_neighbours = part.neighbours(right);
    let mut terminals: Vec<usize> = part
        .neighbours(left)
        .into_iter()
        .filter(|n| right_neighbours.contains(n))
        .filter(|&n| {
            matches!(part.faces[n].surface, Surface::Plane { .. })
                && part
                    .face_normal(n, 0.0, 0.0)
                    .is_some_and(|nv| nv[axis].abs() >= AXIS_ALIGNED_COS)
        })
        .collect();
    terminals.sort_unstable();
    let [t0, t1] = terminals[..] else { return None };
    // One valid solid owns all four faces.
    let solid = part.faces[left].solid?;
    if !part.solid_is_valid(solid)
        || [right, t0, t1]
            .iter()
            .any(|&f| part.faces[f].solid != Some(solid))
    {
        return None;
    }
    let bounds = part.solid_bounds(solid);
    let extents = [0, 1, 2].map(|i| bounds.max[i] - bounds.min[i]);
    let tolerance = crate::kernel::geom::length_tol(bounds.max_extent(), 1e-9);
    if extents[axis]
        < cross
            .iter()
            .map(|&i| extents[i])
            .fold(f64::INFINITY, f64::min)
            - tolerance
    {
        return None;
    }
    let coordinate = |f: usize| {
        let b = part.face_bounds(f);
        mid((b.min[axis], b.max[axis]))
    };
    let on_exterior = |f: usize| {
        let c = coordinate(f);
        (c - bounds.min[axis])
            .abs()
            .min((c - bounds.max[axis]).abs())
            <= tolerance
    };
    let exterior: Vec<usize> = [t0, t1].into_iter().filter(|&f| on_exterior(f)).collect();
    let internal: Vec<usize> = [t0, t1]
        .into_iter()
        .filter(|f| !exterior.contains(f))
        .collect();
    let (&[outside], &[inside]) = (&exterior[..], &internal[..]) else {
        return None;
    };
    let arc = |a, b| part.arc(a, b);
    if arc(left, outside) != Some(Arc::Convex) || arc(right, outside) != Some(Arc::Convex) {
        return None;
    }
    if arc(left, inside) != Some(Arc::Concave) || arc(right, inside) != Some(Arc::Concave) {
        return None;
    }
    let edge_bounds = {
        let mut b = crate::kernel::geom::Bounds::empty();
        ed.samples.iter().for_each(|p| b.add(*p));
        [0, 1, 2].map(|i| (b.min[i], b.max[i]))
    };
    let run_tolerance = crate::kernel::geom::length_tol(
        (l.spans[axis].1 - l.spans[axis].0).max(r.spans[axis].1 - r.spans[axis].0),
        1e-9,
    );
    let run = edge_bounds[axis];
    if [l.spans[axis], r.spans[axis]].iter().any(|span| {
        (span.0 - run.0).abs() > run_tolerance || (span.1 - run.1).abs() > run_tolerance
    }) {
        return None;
    }
    let at = edge_bounds.map(|span| py::round_to3(mid(span)));
    let angle = l.normal[opposed]
        .abs()
        .atan2(l.normal[same].abs())
        .to_degrees();
    let sign = if coordinate(outside) > coordinate(inside) {
        1.0
    } else {
        -1.0
    };
    let opening_direction = [0, 1, 2].map(|i| if i == axis { sign } else { 0.0 });
    let widths = (
        l.spans[opposed].1 - l.spans[opposed].0,
        r.spans[opposed].1 - r.spans[opposed].0,
    );
    let mut half_widths = None;
    if (widths.0 - widths.1).abs() > tolerance {
        let ridge = mid(edge_bounds[opposed]);
        let (lm, rm) = (mid(l.spans[opposed]), mid(r.spans[opposed]));
        if lm < ridge - tolerance && rm > ridge + tolerance {
            half_widths = Some((py::round_to3(widths.0), py::round_to3(widths.1)));
        } else if rm < ridge - tolerance && lm > ridge + tolerance {
            half_widths = Some((py::round_to3(widths.1), py::round_to3(widths.0)));
        } else {
            return None;
        }
    }
    Some(Occurrence {
        record: PairedRampStep {
            axis: ['x', 'y', 'z'][axis],
            angle: py::round_to(angle, 2),
            length: py::round_to3(run.1 - run.0),
            at,
            opening_direction,
            half_width: py::round_to3(0.5 * (widths.0 + widths.1)),
            half_widths,
        },
        defining: vec![left, right, inside],
        context: vec![],
    })
}
