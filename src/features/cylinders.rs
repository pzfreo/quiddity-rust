//! The cylindrical-face inventory (`quiddity._cylinder_substrate.analyse_cylinders`): native
//! analytic cylinders, and freeform faces recovered as cylinders whose material side is proved.

use std::f64::consts::TAU;

use super::policy;
use super::probes::probe_samples;
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, Frame, Surface, V3, dominant_axis_preferring_z};
use crate::kernel::py;
use crate::kernel::recover;

#[derive(Clone, Debug, PartialEq)]
pub struct CylinderEvidence {
    pub face: usize,
    pub solid: usize,
    pub diameter: f64,
    /// Dominant axis letter index (0 = x, 1 = y, 2 = z).
    pub axis: usize,
    /// The face's angular span in radians.
    pub u_extent: f64,
    pub axis_point: V3,
    /// Unit axis direction with its dominant component positive.
    pub direction: V3,
    pub s_lo: f64,
    pub s_hi: f64,
    pub external: bool,
}

fn canonical(value: f64, floor: f64) -> f64 {
    if value.abs() <= floor {
        0.0
    } else {
        py::quantise(value, 12)
    }
}

/// Every native cylinder face, grouped by owning solid (or all faces when there are none).
/// A recovered cylinder's extent along *direction* (`_recovered_axis_bounds`): its boundary's,
/// where a cylinder face's extremes along its axis lie.
fn recovered_extent(part: &Part, face: usize, direction: V3) -> (f64, f64) {
    part.face_edges(face)
        .into_iter()
        .flat_map(|e| part.edges[e].samples.iter())
        .map(|p| geom::dot(*p, direction))
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), s| {
            (lo.min(s), hi.max(s))
        })
}

/// A recovered cylinder's angular span (`_recovered_angular_lower_bound`): a whole turn across a
/// seam, otherwise the smallest arc holding its boundary. `None` when the boundary strays off
/// the radius.
fn recovered_span(part: &Part, face: usize, frame: &Frame, radius: f64) -> Option<f64> {
    let mut seen = std::collections::BTreeMap::<usize, usize>::new();
    for lp in &part.faces[face].loops {
        for &(e, _) in &lp.edges {
            *seen.entry(e).or_default() += 1;
        }
    }
    if seen.values().any(|&n| n > 1) {
        return Some(TAU);
    }
    let tolerance = recover::tolerance(part, face)?;
    let mut angles: Vec<f64> = Vec::new();
    for e in seen.keys() {
        for p in &part.edges[*e].samples {
            let l = frame.to_local(*p);
            if (l[0].hypot(l[1]) - radius).abs() > tolerance {
                return None;
            }
            angles.push(l[1].atan2(l[0]).rem_euclid(TAU));
        }
    }
    angles.sort_by(f64::total_cmp);
    angles.dedup();
    if angles.len() < 2 {
        return None;
    }
    let mut gaps: Vec<f64> = angles.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.push(angles[0] + TAU - angles[angles.len() - 1]);
    Some(TAU - gaps.iter().copied().fold(0.0, f64::max))
}

/// Whether a recovered cylinder is external (its material inside it): the face's own outward
/// normal points away from the axis, at every probe and agreeing.
fn recovered_external(part: &Part, face: usize, frame: &Frame) -> Option<bool> {
    let sides: Vec<bool> = probe_samples(part, face)
        .iter()
        .filter_map(|s| {
            let l = frame.to_local(s.point);
            let away = frame.dir_to_world([l[0], l[1], 0.0]);
            let n = part.face_normal(face, s.uv.0, s.uv.1)?;
            Some(geom::dot(n, away) > 0.0)
        })
        .collect();
    (sides.len() >= 2 && sides.iter().all(|&x| x == sides[0])).then(|| sides[0])
}

pub fn analyse_cylinders(part: &Part) -> Vec<CylinderEvidence> {
    let faces: Vec<(usize, usize)> = if part.solids.is_empty() {
        (0..part.faces.len()).map(|f| (0, f)).collect()
    } else {
        part.solids
            .iter()
            .enumerate()
            .flat_map(|(s, solid)| solid.faces.iter().map(move |&f| (s, f)))
            .collect()
    };
    let mut out = Vec::new();
    for (solid, face) in faces {
        let (frame, radius, u_extent, axial_v, external) = match part.faces[face].surface {
            Surface::Cylinder { frame, radius } => {
                let Some((u0, u1, v0, v1)) = part.uv_bounds(face) else {
                    continue;
                };
                let external = part.frame_points_outward(face).unwrap_or(false);
                (frame, radius, u1 - u0, Some((v0, v1)), external)
            }
            Surface::Freeform { .. } => {
                let Some(&Surface::Cylinder { frame, radius }) = part.recovered(face) else {
                    continue;
                };
                let Some(u_extent) = recovered_span(part, face, &frame, radius) else {
                    continue;
                };
                let Some(external) = recovered_external(part, face, &frame) else {
                    continue;
                };
                (frame, radius, u_extent, None, external)
            }
            _ => continue,
        };
        let axis = dominant_axis_preferring_z(frame.z);
        let sign = if frame.z[axis] > 0.0 { 1.0 } else { -1.0 };
        let direction = geom::scale(frame.z, sign);
        let axial = match axial_v {
            Some((v0, v1)) => {
                let s_ap = py::fsum((0..3).map(|i| frame.origin[i] * direction[i]));
                (s_ap + sign * v0, s_ap + sign * v1)
            }
            None => recovered_extent(part, face, direction),
        };
        let axial = (
            canonical(axial.0, COORD_FLOOR),
            canonical(axial.1, COORD_FLOOR),
        );
        out.push(CylinderEvidence {
            face,
            solid,
            diameter: py::quantise6(radius * 2.0),
            axis,
            u_extent,
            axis_point: frame.origin.map(|c| canonical(c, COORD_FLOOR)),
            direction: direction.map(|c| canonical(c, 1e-12)),
            s_lo: axial.0.min(axial.1),
            s_hi: axial.0.max(axial.1),
            external,
        });
    }
    out
}

/// Whether two unit-direction axis lines are parallel and within *tol* (`_coaxial_axis_lines`).
pub fn coaxial_axis_lines(pa: V3, da: V3, pb: V3, db: V3, tol: f64) -> bool {
    if py::sum((0..3).map(|i| da[i] * db[i])).abs() < 1.0 - 1e-6 {
        return false;
    }
    let offset = geom::sub(pa, pb);
    let along = py::sum((0..3).map(|i| offset[i] * db[i]));
    let d2 = py::sum((0..3).map(|i| (offset[i] - along * db[i]).powi(2)));
    d2 <= tol * tol
}

/// Coaxial patches whose axial ranges meet within this fraction of the band's diameter belong
/// to one stack (`_STACK_GAP_FRAC`, ADR 0008).
pub const STACK_GAP_FRAC: f64 = 0.0125;
/// A run of patches round one axis is a hole or boss only if it totals more than half a turn.
const FULL_CYL_MIN_EXTENT: f64 = std::f64::consts::PI * 1.05;

/// A coaxial segment: patches of one axis line and diameter over one contiguous axial range,
/// merged (`SegmentEvidence`). Its identity fields are the first patch's.
#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub solid: usize,
    pub diameter: f64,
    pub axis: usize,
    pub axis_point: V3,
    pub direction: V3,
    pub s_lo: f64,
    pub s_hi: f64,
    pub external: bool,
    pub faces: Vec<usize>,
}

/// What groups patches and segments: the fields `_line_key`/`_cyl_group_key` read.
pub trait Coaxial {
    fn solid(&self) -> usize;
    fn axis(&self) -> usize;
    fn axis_point(&self) -> V3;
    fn direction(&self) -> V3;
    fn diameter(&self) -> f64;
    fn span(&self) -> (f64, f64);
}

impl Coaxial for CylinderEvidence {
    fn solid(&self) -> usize {
        self.solid
    }
    fn axis(&self) -> usize {
        self.axis
    }
    fn axis_point(&self) -> V3 {
        self.axis_point
    }
    fn direction(&self) -> V3 {
        self.direction
    }
    fn diameter(&self) -> f64 {
        self.diameter
    }
    fn span(&self) -> (f64, f64) {
        (self.s_lo, self.s_hi)
    }
}

impl Coaxial for Segment {
    fn solid(&self) -> usize {
        self.solid
    }
    fn axis(&self) -> usize {
        self.axis
    }
    fn axis_point(&self) -> V3 {
        self.axis_point
    }
    fn direction(&self) -> V3 {
        self.direction
    }
    fn diameter(&self) -> f64 {
        self.diameter
    }
    fn span(&self) -> (f64, f64) {
        (self.s_lo, self.s_hi)
    }
}

/// `_line_key`: solid, axis letter, full direction and the axis point projected onto the plane
/// through the origin perpendicular to the axis, rounded to 3 decimals. Compared with `==`, so
/// -0.0 and 0.0 are one key, as in a Python dict.
pub fn line_key(c: &impl Coaxial) -> Vec<f64> {
    let (p, d) = (c.axis_point(), c.direction());
    let t = geom::dot(p, d);
    vec![
        c.solid() as f64,
        c.axis() as f64,
        d[0],
        d[1],
        d[2],
        py::round_to(p[0] - t * d[0], 3),
        py::round_to(p[1] - t * d[1], 3),
        py::round_to(p[2] - t * d[2], 3),
    ]
}

/// `_cyl_group_key`: the line key plus the diameter to four significant figures.
pub fn group_key(c: &impl Coaxial) -> Vec<f64> {
    let mut key = line_key(c);
    key.push(py::quantise(c.diameter(), 4));
    key
}

/// `_merge_runs`: group by *key* in first-seen order, sort each group by `s_lo` (stably), and
/// split it wherever the next item starts beyond the run's reach plus its own gap tolerance.
pub fn merge_runs<T: Coaxial + Clone>(items: &[T], key: impl Fn(&T) -> Vec<f64>) -> Vec<Vec<T>> {
    let mut groups: Vec<(Vec<f64>, Vec<T>)> = Vec::new();
    for item in items {
        let k = key(item);
        match groups.iter_mut().find(|(g, _)| *g == k) {
            Some((_, members)) => members.push(item.clone()),
            None => groups.push((k, vec![item.clone()])),
        }
    }
    let mut runs = Vec::new();
    for (_, mut group) in groups {
        group.sort_by(|a, b| py::order(a.span().0, b.span().0));
        let mut run = vec![group[0].clone()];
        let mut hi = group[0].span().1;
        for c in &group[1..] {
            if c.span().0 <= hi + policy::length_tol(c.diameter(), STACK_GAP_FRAC) {
                run.push(c.clone());
                hi = hi.max(c.span().1);
            } else {
                runs.push(std::mem::replace(&mut run, vec![c.clone()]));
                hi = c.span().1;
            }
        }
        runs.push(run);
    }
    runs
}

/// `full_cylinders`: the patches belonging to a hole or boss — runs round one axis totalling
/// more than half a turn — in run order.
pub fn full_cylinders(cyls: &[CylinderEvidence]) -> Vec<CylinderEvidence> {
    merge_runs(cyls, group_key)
        .into_iter()
        .filter(|run| run.iter().map(|c| c.u_extent).sum::<f64>() >= FULL_CYL_MIN_EXTENT)
        .flatten()
        .collect()
}

/// `_segments`: one segment per run of same-line, same-diameter patches.
pub fn segments(cyls: &[CylinderEvidence]) -> Vec<Segment> {
    merge_runs(cyls, group_key)
        .into_iter()
        .map(|run| {
            let first = &run[0];
            Segment {
                solid: first.solid,
                diameter: first.diameter,
                axis: first.axis,
                axis_point: first.axis_point,
                direction: first.direction,
                s_lo: run.iter().map(|p| p.s_lo).fold(f64::INFINITY, f64::min),
                s_hi: run.iter().map(|p| p.s_hi).fold(f64::NEG_INFINITY, f64::max),
                external: first.external,
                faces: run.iter().map(|p| p.face).collect(),
            }
        })
        .collect()
}

/// The point on a segment's axis at axial coordinate *s* (`_axis_point`).
pub fn axis_point_at(c: &impl Coaxial, s: f64) -> V3 {
    let (p, d) = (c.axis_point(), c.direction());
    geom::add(p, geom::scale(d, s - geom::dot(p, d)))
}

/// The inventory as Python's `(z_cyls, cross_cyls)` pair concatenated: z-axis cylinders first,
/// each group in solid and face order — the order families that scan it rely on.
pub fn z_then_cross(cyls: &[CylinderEvidence]) -> (Vec<CylinderEvidence>, Vec<CylinderEvidence>) {
    cyls.iter().cloned().partition(|c| c.axis == 2)
}

/// `_canonical_axis_direction`: dominant component positive, components rounded to 6 decimals
/// with sub-half-micro values zeroed.
pub fn canonical_axis_direction(axis: usize, d: V3) -> V3 {
    let norm = py::hypot(&d);
    let sign = if d[axis] < 0.0 { -1.0 } else { 1.0 };
    let unit = if (norm - 1.0).abs() <= 2e-6 {
        d.map(|c| sign * c)
    } else {
        d.map(|c| sign * c / norm)
    };
    unit.map(|c| {
        if c.abs() < 0.5e-6 {
            0.0
        } else {
            py::round_to(c, 6)
        }
    })
}

/// `_axis_line_coordinates`: where the axis line crosses the plane through the origin, as the
/// two coordinates other than the axis's own, rounded to 3 decimals.
pub fn axis_line_coordinates(axis: usize, point: V3, d: V3) -> (f64, f64) {
    let norm = py::hypot(&d);
    let sign = if d[axis] < 0.0 { -1.0 } else { 1.0 };
    let v = d.map(|c| sign * c / norm);
    let along = point[0] * v[0] + point[1] * v[1] + point[2] * v[2];
    let foot = [0, 1, 2].map(|i| point[i] - along * v[i]);
    let keep: Vec<usize> = (0..3).filter(|&i| i != axis).collect();
    (
        py::without_negative_zero(py::round_to(foot[keep[0]], 3)),
        py::without_negative_zero(py::round_to(foot[keep[1]], 3)),
    )
}
