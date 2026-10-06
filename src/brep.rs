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
    /// Control polygons of the file's B-spline parameter-space curves (pcurves) on this face.
    /// OpenCascade sizes a face's parameter range from these polygons rather than from the
    /// curves, so they are what [`Part::uv_bounds`] needs to agree with it.
    pub pcurve_poles: Vec<Vec<(f64, f64)>>,
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
/// Edge samples stand in for the exact curve wherever a polyline is needed (parameter-space
/// loops, boundary distances); the chord may stray from the curve by at most this much (mm).
const CHORD_TOLERANCE: f64 = 2e-4;
/// Initial uniform segments per edge (per quarter turn for conics) before adaptive refinement.
const INITIAL_SEGMENTS: usize = 16;
const MAX_REFINE_DEPTH: usize = 14;

/// The curve parameter interval an edge covers, in its own start → end direction.
fn edge_interval(curve: &Curve, start: V3, end: V3, same_sense: bool) -> (f64, f64) {
    let (a, b) = (curve.parameter(start), curve.parameter(end));
    let Some(period) = curve.period() else {
        return (a, b);
    };
    // On a closed curve the edge runs from start to end in its sense, wrapping if it must; an
    // edge whose ends coincide is the whole curve.
    let closed = geom::dist(start, end) < 1e-9;
    let mut b = b;
    if same_sense {
        while b <= a + if closed { period * 1e-12 } else { 0.0 } {
            b += period;
        }
    } else {
        while b >= a - if closed { period * 1e-12 } else { 0.0 } {
            b -= period;
        }
    }
    (a, b)
}

pub fn sample_edge(curve: &Curve, start: V3, end: V3, same_sense: bool) -> Vec<V3> {
    let (a, b) = edge_interval(curve, start, end, same_sense);
    let n = match curve {
        Curve::Line { .. } | Curve::Other { .. } => return vec![start, end],
        Curve::Nurbs(_) => INITIAL_SEGMENTS,
        _ => ((b - a).abs() / (TAU / 4.0) * INITIAL_SEGMENTS as f64)
            .ceil()
            .max(4.0) as usize,
    };
    let params: Vec<f64> = (0..=n).map(|i| a + (b - a) * i as f64 / n as f64).collect();
    let mut out = vec![curve.value(a)];
    for w in params.windows(2) {
        refine(
            curve,
            (w[0], out[out.len() - 1]),
            (w[1], curve.value(w[1])),
            0,
            &mut out,
        );
    }
    // Pin the ends to the vertices so loops close exactly.
    out[0] = start;
    *out.last_mut().expect("at least two samples") = end;
    out
}

/// Append the samples after *lo* up to and including *hi*, splitting while the chord's midpoint
/// strays from the curve by more than the tolerance.
fn refine(curve: &Curve, lo: (f64, V3), hi: (f64, V3), depth: usize, out: &mut Vec<V3>) {
    let tm = 0.5 * (lo.0 + hi.0);
    let pm = curve.value(tm);
    let chord_mid = geom::scale(geom::add(lo.1, hi.1), 0.5);
    if depth < MAX_REFINE_DEPTH && geom::dist(pm, chord_mid) > CHORD_TOLERANCE {
        refine(curve, lo, (tm, pm), depth + 1, out);
        refine(curve, (tm, pm), hi, depth + 1, out);
    } else {
        out.push(hi.1);
    }
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
                    if domain.contains(u, v) || touches_singular_point(&f.surface, domain, v) {
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
            let raw = route_singular_points(&f.surface, raw, f.reversed);
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
            close_through_pole(&f.surface, &mut pts, f.reversed);
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
        // OpenCascade's range covers each B-spline pcurve's control polygon, which can overshoot
        // the curve itself. Align each polygon to the loops' period before taking it in.
        let centre = (0.5 * (range[0] + range[1]), 0.5 * (range[2] + range[3]));
        for poles in &f.pcurve_poles {
            let (mut lo_u, mut hi_u, mut lo_v, mut hi_v) = (
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            );
            for &(u, v) in poles {
                (lo_u, hi_u, lo_v, hi_v) = (lo_u.min(u), hi_u.max(u), lo_v.min(v), hi_v.max(v));
            }
            let mid = (0.5 * (lo_u + hi_u), 0.5 * (lo_v + hi_v));
            let su = if pu {
                geom::nearest_turn(mid.0, centre.0) - mid.0
            } else {
                0.0
            };
            let sv = if pv {
                geom::nearest_turn(mid.1, centre.1) - mid.1
            } else {
                0.0
            };
            range[0] = range[0].min(lo_u + su);
            range[1] = range[1].max(hi_u + su);
            range[2] = range[2].min(lo_v + sv);
            range[3] = range[3].max(hi_v + sv);
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

    /// The topological half of `BRepCheck` validity that the evidence path depends on: every
    /// edge of the solid is used by exactly two faces (or twice by one, as a seam), and two
    /// different faces sharing an open edge run it in opposite directions, so the shell is closed
    /// and orientable.
    ///
    /// A closed edge (a full circle) is exempt from the direction test: it joins itself
    /// whichever way it is run, so its recorded direction is not evidence. OpenCascade writes
    /// some toroidal faces' circles the wrong way round and re-derives the direction from the
    /// parameter-space curves on reading.
    pub fn solid_is_valid(&self, solid: usize) -> bool {
        let mut uses: std::collections::HashMap<usize, Vec<(usize, bool)>> = Default::default();
        for &f in &self.solids[solid].faces {
            for lp in &self.faces[f].loops {
                for &(e, forward) in &lp.edges {
                    uses.entry(e).or_default().push((f, forward));
                }
            }
        }
        !uses.is_empty()
            && uses.iter().all(|(&e, u)| match u.as_slice() {
                [(fa, da), (fb, db)] => {
                    fa == fb
                        || da != db
                        || geom::dist(self.edges[e].start, self.edges[e].end) < 1e-9
                }
                _ => false,
            })
    }
}

/// Replace each run of singular samples (where u is undefined) by the singular v line from the
/// u the boundary arrives at to the u it leaves at.
///
/// Which way round is a question only orientation answers — a hemisphere bounded by one
/// meridian circle arrives and leaves half a turn apart — so the face's side decides it: STEP
/// keeps the face on a loop's left, which in parameter space is the -u side of a boundary
/// rising to the top line (+u for a reversed face), mirrored at the bottom.
fn route_singular_points(
    surface: &Surface,
    raw: Vec<(f64, f64)>,
    reversed: bool,
) -> Vec<(f64, f64)> {
    let n = raw.len();
    let singular: Vec<bool> = raw
        .iter()
        .map(|&(_, v)| surface.singular_v(v).is_some())
        .collect();
    if !singular.iter().any(|s| *s) || singular.iter().all(|s| *s) {
        return raw;
    }
    // Start on a regular sample so that no singular run straddles the loop's ends.
    let first_regular = singular.iter().position(|s| !s).expect("a regular sample");
    let raw: Vec<(f64, f64)> = raw[first_regular..]
        .iter()
        .chain(&raw[..first_regular])
        .copied()
        .collect();
    let singular: Vec<bool> = singular[first_regular..]
        .iter()
        .chain(&singular[..first_regular])
        .copied()
        .collect();
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
        let v = surface.singular_v(raw[start].1).expect("a singular sample");
        let (ua, ub) = (raw[before].0, raw[after].0);
        let mut ub = geom::nearest_turn(ub, ua);
        if (ub - ua).abs() > 1e-6 {
            // Rising to the top line (v above the boundary's) runs -u for a forward face.
            let rising = v > raw[before].1;
            let toward = if rising != reversed { -1.0 } else { 1.0 };
            if (ub - ua) * toward < 0.0 {
                ub += toward * TAU;
            }
        }
        out.extend(periodic_line(ua, ub, v));
    }
    // Close explicitly: the loop's first sample is regular, so it is where the walk returns.
    if out.last() != out.first() {
        out.push(out[0]);
    }
    out
}

/// Whether the face reaches the singular point (pole or apex) at *v*: the point is a whole
/// parameter line, which lies on the face's boundary rather than strictly inside it, so look
/// just off the line instead.
fn touches_singular_point(surface: &Surface, domain: &FaceDomain, v: f64) -> bool {
    let Some(line) = surface.singular_v(v) else {
        return false;
    };
    let off = line - line.signum() * 1e-4;
    (0..64).any(|k| domain.contains(TAU * k as f64 / 64.0, off))
}

/// A loop that runs once round a sphere ends one turn from where it began; close it along a
/// pole's parameter line so that it bounds a region. The pole is the one it reaches along a
/// seam if any; otherwise the one on the face's side of it (the left of a loop running +u on a
/// forward face is +v, the north pole).
fn close_through_pole(surface: &Surface, pts: &mut Vec<(f64, f64)>, reversed: bool) {
    if !matches!(surface, Surface::Sphere { .. }) || pts.len() < 2 {
        return;
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    if (last.0 - first.0).abs() < std::f64::consts::PI {
        return;
    }
    let half = std::f64::consts::FRAC_PI_2;
    let touched = pts
        .iter()
        .map(|p| p.1)
        .find(|v| (v.abs() - half).abs() < 1e-6);
    let pole = match touched {
        Some(v) => v.signum() * half,
        None if (last.0 > first.0) != reversed => half,
        None => -half,
    };
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
