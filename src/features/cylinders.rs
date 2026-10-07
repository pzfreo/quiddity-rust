//! The cylindrical-face inventory (`quiddity._cylinder_substrate.analyse_cylinders`), native
//! analytic cylinders only.

use crate::kernel::brep::Part;
use crate::kernel::geom::{self, COORD_FLOOR, Surface, V3, dominant_axis_preferring_z};
use crate::kernel::py;

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
        let Surface::Cylinder { frame, radius } = part.faces[face].surface else {
            continue;
        };
        let Some((u0, u1, v0, v1)) = part.uv_bounds(face) else {
            continue;
        };
        let axis = dominant_axis_preferring_z(frame.z);
        let sign = if frame.z[axis] > 0.0 { 1.0 } else { -1.0 };
        let direction = geom::scale(frame.z, sign);
        let s_ap = py::fsum((0..3).map(|i| frame.origin[i] * direction[i]));
        let axial = (s_ap + sign * v0, s_ap + sign * v1);
        let axial = (
            canonical(axial.0, COORD_FLOOR),
            canonical(axial.1, COORD_FLOOR),
        );
        out.push(CylinderEvidence {
            face,
            solid,
            diameter: py::quantise6(radius * 2.0),
            axis,
            u_extent: u1 - u0,
            axis_point: frame.origin.map(|c| canonical(c, COORD_FLOOR)),
            direction: direction.map(|c| canonical(c, 1e-12)),
            s_lo: axial.0.min(axial.1),
            s_hi: axial.0.max(axial.1),
            external: part.frame_points_outward(face).unwrap_or(false),
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
            if c.span().0 <= hi + geom::length_tol(c.diameter(), STACK_GAP_FRAC) {
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
