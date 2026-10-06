//! The boundary-representation model the recognisers read: faces, edges and solids, in the
//! traversal order OpenCascade gives a STEP import, so face indices mean the same thing on both
//! sides of the port.

use std::f64::consts::TAU;
use std::sync::OnceLock;

use crate::geom::{self, Curve, Surface, V3};
use crate::trim::FaceDomain;

#[derive(Clone, Debug)]
pub struct Edge {
    pub curve: Curve,
    pub start: V3,
    pub end: V3,
    /// Whether start → end follows the curve's parameter direction (`EDGE_CURVE.same_sense`).
    pub same_sense: bool,
    /// Points along the edge, start → end, dense enough to stand in for the exact curve.
    pub samples: Vec<V3>,
}

#[derive(Clone, Debug)]
pub struct Loop {
    /// Edge indices with their use direction in this loop (`true` = start → end).
    pub edges: Vec<(usize, bool)>,
    /// A loop made of a single vertex (`VERTEX_LOOP`): the apex of a cone, the pole of a sphere.
    pub vertex: Option<V3>,
}

#[derive(Clone, Debug)]
pub struct Face {
    pub surface: Surface,
    /// `TopAbs_REVERSED`: the face's material side is against the surface normal.
    pub reversed: bool,
    pub loops: Vec<Loop>,
    pub solid: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Solid {
    pub faces: Vec<usize>,
}

/// One loop of a face in its surface's (u, v) space, unwrapped across periodic parameters.
#[derive(Clone, Debug)]
pub struct UvLoop {
    pub points: Vec<(f64, f64)>,
    /// A vertex loop's stand-in: the whole periodic line through a cone's apex.
    pub degenerate: bool,
}

/// Per-face values derived from the geometry, computed on first use.
#[derive(Debug, Default)]
struct FaceCache {
    uv_loops: OnceLock<Option<Vec<UvLoop>>>,
    domain: OnceLock<Option<FaceDomain>>,
    bounds: OnceLock<Bounds>,
}

#[derive(Debug)]
pub struct Part {
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    pub solids: Vec<Solid>,
    cache: Vec<FaceCache>,
}

/// An axis-aligned box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: V3,
    pub max: V3,
}

impl Bounds {
    pub fn empty() -> Self {
        Bounds {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        }
    }
    pub fn add(&mut self, p: V3) {
        for i in 0..3 {
            self.min[i] = self.min[i].min(p[i]);
            self.max[i] = self.max[i].max(p[i]);
        }
    }
    pub fn merge(&mut self, other: &Bounds) {
        self.add(other.min);
        self.add(other.max);
    }
    pub fn centre(&self) -> V3 {
        [
            0.5 * (self.min[0] + self.max[0]),
            0.5 * (self.min[1] + self.max[1]),
            0.5 * (self.min[2] + self.max[2]),
        ]
    }
    pub fn max_extent(&self) -> f64 {
        (0..3)
            .map(|i| self.max[i] - self.min[i])
            .fold(f64::NEG_INFINITY, f64::max)
    }
    pub fn diagonal(&self) -> f64 {
        geom::dist(self.min, self.max)
    }
}

/// Samples per full turn of a circle; arcs get a proportional share (at least four).
const SAMPLES_PER_TURN: f64 = 512.0;
const NURBS_SAMPLES: usize = 256;

/// The curve parameter interval an edge covers, in its own start → end direction.
fn edge_interval(curve: &Curve, start: V3, end: V3, same_sense: bool) -> (f64, f64) {
    match curve {
        Curve::Nurbs(n) => n.domain(),
        Curve::Line { .. } => (curve.parameter(start), curve.parameter(end)),
        _ => {
            let (a, b) = (curve.parameter(start), curve.parameter(end));
            let closed = geom::dist(start, end) < 1e-9;
            if same_sense {
                let mut b = b;
                while b <= a + if closed { 1e-12 } else { 0.0 } {
                    b += TAU;
                }
                (a, b)
            } else {
                let mut b = b;
                while b >= a - if closed { 1e-12 } else { 0.0 } {
                    b -= TAU;
                }
                (a, b)
            }
        }
    }
}

pub fn sample_edge(curve: &Curve, start: V3, end: V3, same_sense: bool) -> Vec<V3> {
    let (a, b) = edge_interval(curve, start, end, same_sense);
    let n = match curve {
        Curve::Line { .. } => 1,
        Curve::Nurbs(_) => NURBS_SAMPLES,
        Curve::Other { .. } => return vec![start, end],
        _ => ((b - a).abs() / TAU * SAMPLES_PER_TURN).ceil().max(4.0) as usize,
    };
    let mut out: Vec<V3> = (0..=n)
        .map(|i| curve.value(a + (b - a) * i as f64 / n as f64))
        .collect();
    // Pin the ends to the vertices so loops close exactly.
    out[0] = start;
    *out.last_mut().expect("at least two samples") = end;
    out
}

/// Exact extremes of a circle arc per world axis — the samples alone would undercut a bulge.
fn arc_extremes(curve: &Curve, start: V3, end: V3, same_sense: bool) -> Vec<V3> {
    let Curve::Circle { frame, .. } = curve else {
        return vec![];
    };
    let (a, b) = edge_interval(curve, start, end, same_sense);
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut out = Vec::new();
    for i in 0..3 {
        let base = frame.y[i].atan2(frame.x[i]);
        for k in -2..=3 {
            for t in [
                base + k as f64 * TAU,
                base + std::f64::consts::PI + k as f64 * TAU,
            ] {
                if t > lo && t < hi {
                    out.push(curve.value(t));
                }
            }
        }
    }
    out
}

impl Part {
    pub fn new(faces: Vec<Face>, edges: Vec<Edge>, solids: Vec<Solid>) -> Self {
        let cache = faces.iter().map(|_| FaceCache::default()).collect();
        Part {
            faces,
            edges,
            solids,
            cache,
        }
    }

    /// The face's axis-aligned box: its boundary plus any interior axis extremes.
    pub fn face_bounds(&self, face: usize) -> Bounds {
        *self.cache[face]
            .bounds
            .get_or_init(|| self.compute_face_bounds(face))
    }

    /// The face's trimmed region in parameter space.
    pub fn domain(&self, face: usize) -> Option<&FaceDomain> {
        self.cache[face]
            .domain
            .get_or_init(|| {
                let f = &self.faces[face];
                FaceDomain::new(self.uv_loops(face)?, f.surface.periodic())
            })
            .as_ref()
    }

    fn compute_face_bounds(&self, face: usize) -> Bounds {
        let f = &self.faces[face];
        let mut b = Bounds::empty();
        for lp in &f.loops {
            if let Some(p) = lp.vertex {
                b.add(p);
            }
            for &(e, _) in &lp.edges {
                let edge = &self.edges[e];
                for p in &edge.samples {
                    b.add(*p);
                }
                for p in arc_extremes(&edge.curve, edge.start, edge.end, edge.same_sense) {
                    b.add(p);
                }
            }
        }
        // Doubly-curved faces can bulge past their boundary: add their interior axis extremes
        // (exact for spheres and tori, sampled for freeform surfaces) that lie on the face.
        let candidates: Vec<(f64, f64)> = match &f.surface {
            Surface::Sphere { .. } | Surface::Torus { .. } => f.surface.axis_extreme_parameters(),
            // A cone face can run to its apex without a vertex there to bound it.
            Surface::Cone {
                radius, semi_angle, ..
            } => {
                let apex = -radius / semi_angle.sin();
                let nudge = 1e-9 * (1.0 + apex.abs());
                [apex - nudge, apex + nudge].map(|v| (0.0, v)).to_vec()
            }
            Surface::Freeform { surface, .. } => {
                let (u0, u1, v0, v1) = surface.domain();
                let n = 12;
                (0..=n)
                    .flat_map(|i| (0..=n).map(move |j| (i, j)))
                    .map(|(i, j)| {
                        (
                            u0 + (u1 - u0) * i as f64 / n as f64,
                            v0 + (v1 - v0) * j as f64 / n as f64,
                        )
                    })
                    .collect()
            }
            _ => Vec::new(),
        };
        if !candidates.is_empty() {
            if let Some(domain) = self.domain(face) {
                for (u, v) in candidates {
                    if domain.contains(u, v) {
                        b.add(f.surface.value(u, v));
                    }
                }
            }
        }
        b
    }

    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for i in 0..self.faces.len() {
            b.merge(&self.face_bounds(i));
        }
        b
    }

    /// Each loop as a closed polyline in (u, v), with periodic parameters unwrapped so each
    /// polyline is continuous. A loop around a periodic direction ends one period away from where
    /// it began.
    pub fn uv_loops(&self, face: usize) -> Option<&[UvLoop]> {
        self.cache[face]
            .uv_loops
            .get_or_init(|| self.compute_uv_loops(face))
            .as_deref()
    }

    fn compute_uv_loops(&self, face: usize) -> Option<Vec<UvLoop>> {
        let f = &self.faces[face];
        let (pu, pv) = f.surface.periodic();
        let mut loops = Vec::new();
        for lp in &f.loops {
            let mut pts: Vec<(f64, f64)> = Vec::new();
            if let Some(apex) = lp.vertex {
                // A degenerate loop: the whole periodic u line at the apex's v.
                let (_, v) = f.surface.parameters(apex, None)?;
                if pu {
                    loops.push(UvLoop {
                        points: periodic_line(0.0, TAU, v),
                        degenerate: true,
                    });
                }
                continue;
            }
            let mut raw: Vec<(f64, f64)> = Vec::new();
            for &(e, forward) in &lp.edges {
                let samples = &self.edges[e].samples;
                let ordered: Box<dyn Iterator<Item = &V3>> = if forward {
                    Box::new(samples.iter())
                } else {
                    Box::new(samples.iter().rev())
                };
                for p in ordered {
                    let hint = raw.last().copied();
                    raw.push(f.surface.parameters(*p, hint)?);
                }
            }
            let raw = route_singular_points(&f.surface, raw);
            for (mut u, mut v) in raw {
                if let Some(&(lu, lv)) = pts.last() {
                    if pu {
                        u = geom::nearest_turn(u, lu);
                    }
                    if pv {
                        v = geom::nearest_turn(v, lv);
                    }
                }
                pts.push((u, v));
            }
            close_through_pole(&f.surface, &mut pts);
            loops.push(UvLoop {
                points: pts,
                degenerate: false,
            });
        }
        Some(loops)
    }

    /// `BRepTools::UVBounds` — (u_first, u_last, v_first, v_last) of the face's trimmed range.
    ///
    /// A periodic direction the face closes around (a seam edge, or a loop that winds once
    /// round) spans exactly one period starting at the seam.
    pub fn uv_bounds(&self, face: usize) -> Option<(f64, f64, f64, f64)> {
        let f = &self.faces[face];
        let (pu, pv) = f.surface.periodic();
        let loops = self.uv_loops(face)?;
        let mut range = [
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ];
        // Unwrap every loop next to the first so separate loops share one period.
        let reference = loops.first().and_then(|l| l.points.first()).copied()?;
        let mut winds = (false, false);
        for lp in loops {
            let lp = &lp.points;
            let (Some(first), Some(last)) = (lp.first(), lp.last()) else {
                continue;
            };
            let shift_u = if pu {
                geom::nearest_turn(first.0, reference.0) - first.0
            } else {
                0.0
            };
            let shift_v = if pv {
                geom::nearest_turn(first.1, reference.1) - first.1
            } else {
                0.0
            };
            winds.0 |= pu && (last.0 - first.0).abs() > 1.0;
            winds.1 |= pv && (last.1 - first.1).abs() > 1.0;
            for &(u, v) in lp {
                range[0] = range[0].min(u + shift_u);
                range[1] = range[1].max(u + shift_u);
                range[2] = range[2].min(v + shift_v);
                range[3] = range[3].max(v + shift_v);
            }
        }
        let seam = self.seam_parameters(face);
        for (periodic, wound, lo, hi, seam_value) in
            [(pu, winds.0, 0, 1, seam.0), (pv, winds.1, 2, 3, seam.1)]
        {
            if !periodic {
                continue;
            }
            if wound || seam_value.is_some() || range[hi] - range[lo] > TAU - 1e-9 {
                let start = seam_value.unwrap_or(0.0);
                range[lo] = start;
                range[hi] = start + TAU;
            }
        }
        Some((range[0], range[1], range[2], range[3]))
    }

    /// The constant periodic parameter of a seam edge (an edge used twice by this face), if
    /// any, normalised to `[0, 2π)` — the side OpenCascade starts its parameter range from.
    fn seam_parameters(&self, face: usize) -> (Option<f64>, Option<f64>) {
        let f = &self.faces[face];
        let mut seen = std::collections::HashMap::new();
        for lp in &f.loops {
            for &(e, _) in &lp.edges {
                *seen.entry(e).or_insert(0) += 1;
            }
        }
        let mut out = (None, None);
        for (&e, &count) in &seen {
            if count < 2 {
                continue;
            }
            let samples = &self.edges[e].samples;
            let Some((u0, v0)) = f.surface.parameters(samples[0], None) else {
                continue;
            };
            let mid = samples[samples.len() / 2];
            let Some((u1, v1)) = f.surface.parameters(mid, None) else {
                continue;
            };
            if (geom::nearest_turn(u1, u0) - u0).abs() < 1e-7 {
                out.0 = Some(round_seam(u0));
            } else if (geom::nearest_turn(v1, v0) - v0).abs() < 1e-7 {
                out.1 = Some(round_seam(v0));
            }
        }
        out
    }

    /// Faces of each edge, in face order (`edge_face_map`). A seam edge lists its face once.
    pub fn edge_faces(&self) -> Vec<Vec<usize>> {
        let mut out = vec![Vec::new(); self.edges.len()];
        for (i, f) in self.faces.iter().enumerate() {
            for lp in &f.loops {
                for &(e, _) in &lp.edges {
                    out[e].push(i);
                }
            }
        }
        out
    }

    /// Whether every edge of the solid is shared by exactly two face uses — the closed-shell
    /// part of `BRepCheck` validity that the evidence path depends on.
    pub fn solid_is_closed(&self, solid: usize) -> bool {
        let mut uses = std::collections::HashMap::new();
        for &f in &self.solids[solid].faces {
            for lp in &self.faces[f].loops {
                for &(e, _) in &lp.edges {
                    *uses.entry(e).or_insert(0usize) += 1;
                }
            }
        }
        !uses.is_empty() && uses.values().all(|&n| n == 2)
    }
}

/// Replace each run of singular samples (where u is undefined) by the singular v line from the
/// u the boundary arrives at to the u it leaves at, the short way round.
fn route_singular_points(surface: &Surface, raw: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let n = raw.len();
    let singular: Vec<bool> = raw
        .iter()
        .map(|&(u, v)| surface.is_singular(u, v))
        .collect();
    if !singular.iter().any(|s| *s) || singular.iter().all(|s| *s) {
        return raw;
    }
    let mut out = Vec::with_capacity(n + 64);
    let mut i = 0;
    while i < n {
        if !singular[i] {
            out.push(raw[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < n && singular[i] {
            i += 1;
        }
        // Neighbours on either side, wrapping round the closed loop.
        let before = (0..n)
            .map(|k| (start + n - 1 - k) % n)
            .find(|&k| !singular[k])
            .expect("a regular sample");
        let after = (0..n)
            .map(|k| (i + k) % n)
            .find(|&k| !singular[k])
            .expect("a regular sample");
        let v = raw[start].1;
        let (ua, ub) = (raw[before].0, raw[after].0);
        let ub = geom::nearest_turn(ub, ua);
        out.extend(periodic_line(ua, ub, v));
    }
    out
}

/// A loop round a sphere that reaches a pole along its seam ends one turn from where it began;
/// close it along the pole's parameter line so that it bounds a region.
fn close_through_pole(surface: &Surface, pts: &mut Vec<(f64, f64)>) {
    if !matches!(surface, Surface::Sphere { .. }) || pts.len() < 2 {
        return;
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    if (last.0 - first.0).abs() < std::f64::consts::PI {
        return;
    }
    let half = std::f64::consts::FRAC_PI_2;
    let Some(pole) = pts
        .iter()
        .map(|p| p.1)
        .find(|v| (v.abs() - half).abs() < 1e-6)
    else {
        return;
    };
    let pole = pole.signum() * half;
    pts.extend(periodic_line(last.0, first.0, pole));
    pts.push(first);
}

/// Points along a constant-v line from u `a` to `b`, short enough that no segment spans half a
/// period — containment wraps each segment to the nearest turn, which a long one would defeat.
fn periodic_line(a: f64, b: f64, v: f64) -> Vec<(f64, f64)> {
    let n = 64;
    (0..=n)
        .map(|i| (a + (b - a) * i as f64 / n as f64, v))
        .collect()
}

/// A seam at 2π is the same seam as one at 0; OpenCascade starts the range at 0.
fn round_seam(value: f64) -> f64 {
    if (value - TAU).abs() < 1e-9 || value.abs() < 1e-9 {
        0.0
    } else {
        value
    }
}
