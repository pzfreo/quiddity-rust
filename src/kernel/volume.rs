//! The volume a probe region shares with a solid — what the Python implementation asks
//! `BRepAlgoAPI_Common` for (`_volume_probe.probe_volume`), without a boolean.
//!
//! The shared volume is the integral over height of the shared area of each slice, and that
//! area the integral across the slice of the shared length of lines within it. The length on
//! each line is exact (ray crossings). It bends only where a line passes a corner of the
//! solid ∩ probe arrangement, so both integrals are split at those corners. In height: the two
//! shapes' vertices and edge turning points, and where either shape's edges pierce the other's
//! faces. Across a slice: where either shape's edges cross it, and where the line two planar
//! faces (one from each shape) share crosses it. Between those breaks, with planar faces only,
//! the length is linear and the area quadratic, so two-point Gauss is exact; curved faces refine
//! adaptively. The two exact shortcuts Python takes come first.

use std::f64::consts::{PI, TAU};

use super::brep::Edge;
use super::classify::{Classifier, State};
use super::geom::{self, Bounds, COORD_FLOOR, Curve, Frame, Surface, V3};
use super::rays::RayCaster;

/// A probe region: an axis-aligned box, or a solid (its part's first solid).
pub enum Probe<'a> {
    Box(Bounds),
    Solid(RayCaster<'a>),
}

/// Halvings allowed within one interval between breaks, where curved faces bend the length.
const MAX_DEPTH: usize = 5;

/// The volume *probe* shares with the material classified by *solid*.
///
/// Emptiness and fullness are decided exactly, on the probe drawn in by `COORD_FLOOR`: if no
/// material reaches that interior the probe is empty, and if no air does it is full. Material
/// or air that reaches no deeper than the coordinate tolerance is where the probe's boundary
/// lies on the solid's within the file's own precision, and is what Python's 1e-6 insets exist
/// to ignore. Anything between is measured on the probe itself.
pub fn common_volume(solid: &Classifier<'_>, probe: &Probe<'_>) -> f64 {
    let [material, air] = shared(solid, probe, COORD_FLOOR);
    if material == 0.0 {
        0.0
    } else if air == 0.0 {
        probe_volume(probe)
    } else {
        shared(solid, probe, 0.0)[0]
    }
}

/// The probe's own volume.
pub fn probe_volume(probe: &Probe<'_>) -> f64 {
    match probe {
        Probe::Box(b) => {
            let s = geom::sub(b.max, b.min);
            s[0] * s[1] * s[2]
        }
        Probe::Solid(rays) => rays.part.solid_mass(0).map_or(0.0, |m| m.0),
    }
}

impl Probe<'_> {
    fn bounds(&self) -> Bounds {
        match self {
            Probe::Box(b) => *b,
            Probe::Solid(rays) => rays.bounds(),
        }
    }

    /// The probe's intervals along a line that starts outside it.
    fn intervals(&self, origin: V3, dir: V3, reach: f64) -> Vec<(f64, f64)> {
        match self {
            Probe::Box(b) => {
                // Slabs: the parameter range inside every axis's pair of planes.
                let (mut lo, mut hi) = (0.0f64, reach);
                for i in 0..3 {
                    if dir[i].abs() < 1e-300 {
                        if origin[i] < b.min[i] || origin[i] > b.max[i] {
                            return Vec::new();
                        }
                        continue;
                    }
                    let (a, c) = (
                        (b.min[i] - origin[i]) / dir[i],
                        (b.max[i] - origin[i]) / dir[i],
                    );
                    (lo, hi) = (lo.max(a.min(c)), hi.min(a.max(c)));
                }
                if lo < hi { vec![(lo, hi)] } else { Vec::new() }
            }
            Probe::Solid(rays) => intervals(rays, origin, dir, reach),
        }
    }

    /// Where a straight segment p–q pierces the probe's faces.
    fn pierce(&self, p: V3, q: V3) -> Vec<V3> {
        let Some(dir) = geom::unit(geom::sub(q, p)) else {
            return Vec::new();
        };
        let span = geom::dist(p, q);
        let ts: Vec<f64> = match self {
            Probe::Box(_) => self
                .intervals(p, dir, span)
                .into_iter()
                .flat_map(|(a, b)| [a, b])
                .collect(),
            Probe::Solid(rays) => rays
                .hits(p, dir, span)
                .unwrap_or_default()
                .iter()
                .map(|h| h.t)
                .collect(),
        };
        ts.into_iter()
            .map(|t| geom::add(p, geom::scale(dir, t)))
            .collect()
    }
}

/// One shape's part in the arrangement near the region: its edges, the planes of its planar
/// faces, and whether it has curved faces there.
struct Side<'a> {
    edges: Vec<Polyline<'a>>,
    planes: Vec<(V3, f64)>,
    curved: bool,
}

/// An edge as its samples, with its curve when crossings must be solved on it, and whether
/// it is a straight segment (whose piercing points are exact corners of the arrangement).
struct Polyline<'a> {
    points: std::borrow::Cow<'a, [V3]>,
    curve: Option<&'a Curve>,
    straight: bool,
}

impl<'a> Side<'a> {
    fn of_solid(rays: &'a RayCaster<'_>, region: &Bounds) -> Self {
        let faces = faces_meeting(rays, region);
        let part = rays.part;
        let mut ids: Vec<usize> = faces.iter().flat_map(|&f| part.face_edges(f)).collect();
        ids.sort_unstable();
        ids.dedup();
        let edges = ids
            .into_iter()
            .map(|e| Polyline::of(&part.edges[e]))
            .collect();
        let mut planes = Vec::new();
        let mut curved = false;
        for &f in &faces {
            match part.faces[f].surface {
                Surface::Plane { frame } => {
                    planes.push((frame.z, geom::dot(frame.z, frame.origin)))
                }
                _ => curved = true,
            }
        }
        Side {
            edges,
            planes,
            curved,
        }
    }

    fn of_box(b: &Bounds) -> Self {
        let corner =
            |i: usize| [0, 1, 2].map(|k| if i >> k & 1 == 0 { b.min[k] } else { b.max[k] });
        let mut edges = Vec::new();
        for i in 0..8 {
            for k in 0..3 {
                if i >> k & 1 == 0 {
                    let points = vec![corner(i), corner(i | 1 << k)];
                    edges.push(Polyline {
                        points: points.into(),
                        curve: None,
                        straight: true,
                    });
                }
            }
        }
        let unit = |k: usize| [0, 1, 2].map(|j| if j == k { 1.0 } else { 0.0 });
        let planes = (0..3)
            .flat_map(|k| [(unit(k), b.min[k]), (unit(k), b.max[k])])
            .collect();
        Side {
            edges,
            planes,
            curved: false,
        }
    }
}

impl<'a> Polyline<'a> {
    fn of(edge: &'a Edge) -> Self {
        // Conics invert in closed form; other curves keep their samples' crossing, within the
        // chordal error, and the adaptive rule absorbs the difference.
        let conic = matches!(edge.curve, Curve::Circle { .. } | Curve::Ellipse { .. });
        let curve = conic.then_some(&edge.curve);
        let straight = matches!(edge.curve, Curve::Line { .. });
        Polyline {
            points: edge.samples.as_slice().into(),
            curve,
            straight,
        }
    }
}

/// The material and the air shared with the probe drawn in by *inset* on every side.
fn shared(solid: &Classifier<'_>, probe: &Probe<'_>, inset: f64) -> Pair {
    let rays = solid.rays();
    let (sb, pb) = (rays.bounds(), probe.bounds());
    let whole = probe_volume(probe);
    let Some(region) = overlap(&sb, &pb) else {
        return [0.0, whole];
    };
    // A probe whose box meets no face's box is wholly inside or wholly outside.
    if !rays.any_face_box_meets(&pb) {
        return match solid.classify(pb.centre()) {
            State::In => [whole, 0.0],
            _ => [0.0, whole],
        };
    }
    let mine = Side::of_solid(rays, &region);
    let theirs = match probe {
        Probe::Box(b) => Side::of_box(b),
        Probe::Solid(p) => Side::of_solid(p, &region),
    };
    // Slices across the probe's longest side, lines along the next.
    let size = geom::sub(pb.max, pb.min);
    let z = (0..3).max_by(|&i, &j| size[i].total_cmp(&size[j])).unwrap();
    let unit = |i: usize| [0, 1, 2].map(|k| if k == i { 1.0 } else { 0.0 });
    let frame = Frame {
        origin: [0.0; 3],
        x: unit((z + 1) % 3),
        y: unit((z + 2) % 3),
        z: unit(z),
    };
    let (h0, h1) = (region.min[z] + inset, region.max[z] - inset);
    let across = (
        region.min[(z + 1) % 3] + inset,
        region.max[(z + 1) % 3] - inset,
    );
    if h1 <= h0 || across.1 <= across.0 {
        return [0.0, 0.0];
    }
    let y = (z + 2) % 3;
    let start = sb.min[y].min(pb.min[y]) - 1.0;
    let reach = sb.max[y].max(pb.max[y]) + 1.0 - start;
    let length = |h: f64, s: f64| {
        let origin = geom::add(
            geom::add(geom::scale(frame.z, h), geom::scale(frame.x, s)),
            geom::scale(frame.y, start),
        );
        let material = intervals(rays, origin, frame.y, reach);
        let inside: Vec<(f64, f64)> = probe
            .intervals(origin, frame.y, reach)
            .into_iter()
            .map(|(a, b)| (a + inset, b - inset))
            .filter(|(a, b)| a < b)
            .collect();
        [
            overlap_length(&material, &inside),
            overlap_length(&gaps(&material, reach), &inside),
        ]
    };
    let rule = if mine.curved || theirs.curved {
        Rule::Adaptive(1e-10 * whole)
    } else {
        Rule::Exact
    };

    // Breaks in height: both shapes' vertices and turning points, and each shape's straight
    // edges piercing the other's faces.
    let at = |v: V3, axis: V3| geom::dot(v, axis);
    let mut hs: Vec<f64> = Vec::new();
    for e in mine.edges.iter().chain(&theirs.edges) {
        hs.extend(turning_points(&e.points, frame.z));
    }
    for e in mine.edges.iter().filter(|e| e.straight) {
        for w in e.points.windows(2) {
            hs.extend(probe.pierce(w[0], w[1]).iter().map(|v| at(*v, frame.z)));
        }
    }
    for e in theirs.edges.iter().filter(|e| e.straight) {
        for w in e.points.windows(2) {
            if let Some(dir) = geom::unit(geom::sub(w[1], w[0])) {
                let hits = rays
                    .hits(w[0], dir, geom::dist(w[0], w[1]))
                    .unwrap_or_default();
                hs.extend(
                    hits.iter()
                        .map(|hit| at(geom::add(w[0], geom::scale(dir, hit.t)), frame.z)),
                );
            }
        }
    }
    // Breaks across a slice: both shapes' edges crossing it, and the lines shared by a plane of
    // each crossing it.
    let area = |h: f64| {
        let mut ss: Vec<f64> = Vec::new();
        for e in mine.edges.iter().chain(&theirs.edges) {
            ss.extend(crossings(e, frame.z, h, frame.x));
        }
        for &(n1, d1) in &mine.planes {
            for &(n2, d2) in &theirs.planes {
                ss.extend(three_planes(n1, d1, n2, d2, frame.z, h).map(|v| at(v, frame.x)));
            }
        }
        integrate(&|s| length(h, s), across, ss, rule.per(h1 - h0))
    };
    integrate(&area, (h0, h1), hs, rule)
}

/// The point three planes `n · p = d` share, if they meet in one.
fn three_planes(n1: V3, d1: f64, n2: V3, d2: f64, n3: V3, d3: f64) -> Option<V3> {
    let det = geom::dot(n1, geom::cross(n2, n3));
    if det.abs() <= 1e-12 {
        return None;
    }
    let p = geom::add(
        geom::add(
            geom::scale(geom::cross(n2, n3), d1),
            geom::scale(geom::cross(n3, n1), d2),
        ),
        geom::scale(geom::cross(n1, n2), d3),
    );
    Some(geom::scale(p, 1.0 / det))
}

fn overlap(a: &Bounds, b: &Bounds) -> Option<Bounds> {
    let min = [0, 1, 2].map(|i| a.min[i].max(b.min[i]));
    let max = [0, 1, 2].map(|i| a.max[i].min(b.max[i]));
    (0..3)
        .all(|i| min[i] < max[i])
        .then_some(Bounds { min, max })
}

/// The faces whose boxes reach the region: only they can bend the length inside it.
fn faces_meeting(rays: &RayCaster<'_>, region: &Bounds) -> Vec<usize> {
    let meets = |b: &Bounds| (0..3).all(|i| b.min[i] <= region.max[i] && region.min[i] <= b.max[i]);
    rays.faces
        .iter()
        .copied()
        .filter(|&f| meets(&rays.face_boxes[f]))
        .collect()
}

/// The material intervals along a ray that starts outside the solid, by the parity of its
/// crossings. Crossings closer than the kernel can tell apart are one (a ray through an edge
/// meets both faces there); a line still left odd (a graze) is nudged off it.
fn intervals(rays: &RayCaster<'_>, origin: V3, dir: V3, reach: f64) -> Vec<(f64, f64)> {
    for nudge in 0..4 {
        let o = geom::add(
            origin,
            [
                1e-9 * nudge as f64,
                2e-9 * nudge as f64,
                3e-9 * nudge as f64,
            ],
        );
        let Some(hits) = rays.trimmed_hits(o, dir, reach) else {
            return Vec::new();
        };
        let mut ts: Vec<f64> = Vec::new();
        for h in hits {
            if ts.last().is_none_or(|&last| h.t - last > 1e-9) {
                ts.push(h.t);
            }
        }
        if ts.len().is_multiple_of(2) {
            return ts.chunks(2).map(|c| (c[0], c[1])).collect();
        }
    }
    Vec::new()
}

/// The total length the two sets of intervals share. A shared stretch shorter than a nanometre
/// is two coincident ends (a probe face lying on a solid face) seen through rounding, and shares
/// nothing.
fn overlap_length(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    let mut total = 0.0;
    for &(a0, a1) in a {
        for &(b0, b1) in b {
            let shared = a1.min(b1) - a0.max(b0);
            if shared > 1e-9 {
                total += shared;
            }
        }
    }
    total
}

/// The stretches of [0, reach] the intervals leave uncovered.
fn gaps(intervals: &[(f64, f64)], reach: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    let mut from = 0.0;
    for &(a, b) in intervals {
        out.push((from, a));
        from = b;
    }
    out.push((from, reach));
    out
}

/// A polyline's ends and turning points along *axis*: where its crossings of a plane normal to
/// *axis* appear, vanish or swap (an edge's samples include a circle's extremes).
fn turning_points(points: &[V3], axis: V3) -> Vec<f64> {
    let c: Vec<f64> = points.iter().map(|p| geom::dot(*p, axis)).collect();
    let mut out = vec![c[0], c[c.len() - 1]];
    // Where the direction strictly reverses; a run level along the axis is no turn.
    let mut rising: Option<bool> = None;
    for w in c.windows(2) {
        if w[1] == w[0] {
            continue;
        }
        let up = w[1] > w[0];
        if rising.is_some_and(|r| r != up) {
            out.push(w[0]);
        }
        rising = Some(up);
    }
    out
}

/// Where an edge crosses the plane `axis · p = x`, as coordinates along *across*: found between
/// its samples, then, on a curve, solved on the curve itself.
fn crossings(edge: &Polyline<'_>, axis: V3, x: f64, across: V3) -> Vec<f64> {
    let mut out = Vec::new();
    for w in edge.points.windows(2) {
        let (p, q) = (w[0], w[1]);
        let (dp, dq) = (geom::dot(p, axis) - x, geom::dot(q, axis) - x);
        if dp * dq > 0.0 || dp == dq {
            continue;
        }
        let linear = geom::add(p, geom::scale(geom::sub(q, p), dp / (dp - dq)));
        let at = match edge.curve {
            None => linear,
            Some(curve) => on_curve(curve, p, q, axis, x).unwrap_or(linear),
        };
        out.push(geom::dot(at, across));
    }
    out
}

/// The point of *curve* between its points *p* and *q* where `axis · point = x`, by bisection.
fn on_curve(curve: &Curve, p: V3, q: V3, axis: V3, x: f64) -> Option<V3> {
    let (mut t0, mut t1) = (curve.parameter(p), curve.parameter(q));
    // A closed curve's parameter wraps; the samples are close, so take the short way round.
    if matches!(curve, Curve::Circle { .. } | Curve::Ellipse { .. }) && (t1 - t0).abs() > PI {
        t1 += if t1 < t0 { TAU } else { -TAU };
    }
    let f = |t: f64| geom::dot(curve.value(t), axis) - x;
    let mut f0 = f(t0);
    if f0 * f(t1) > 0.0 {
        return None;
    }
    for _ in 0..60 {
        let tm = 0.5 * (t0 + t1);
        let fm = f(tm);
        if fm * f0 <= 0.0 {
            t1 = tm;
        } else {
            (t0, f0) = (tm, fm);
        }
    }
    Some(curve.value(0.5 * (t0 + t1)))
}

/// How an interval between breaks is integrated: exactly by two-point Gauss (planar faces and a
/// straight outline), or by four-point Gauss halved until the halves agree.
#[derive(Clone, Copy)]
enum Rule {
    Exact,
    Adaptive(f64),
}

impl Rule {
    /// The rule for an inner integral, its tolerance shared over an outer extent.
    fn per(self, extent: f64) -> Rule {
        match self {
            Rule::Exact => Rule::Exact,
            Rule::Adaptive(tol) => Rule::Adaptive(tol / extent),
        }
    }
}

const GAUSS4: [(f64, f64); 4] = [
    (-0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
    (-0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
    (0.339_981_043_584_856_3, 0.652_145_154_862_546_1),
    (0.861_136_311_594_052_6, 0.347_854_845_137_453_9),
];

/// ∫ f over [lo, hi], by *rule* on each interval between the *breaks* that fall inside it.
fn integrate(f: &dyn Fn(f64) -> Pair, (lo, hi): (f64, f64), breaks: Vec<f64>, rule: Rule) -> Pair {
    if hi <= lo {
        return [0.0, 0.0];
    }
    let mut breaks: Vec<f64> = breaks.into_iter().filter(|&x| lo < x && x < hi).collect();
    breaks.extend([lo, hi]);
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|x, y| *x - *y <= 1e-9 * (hi - lo));
    breaks
        .windows(2)
        .map(|w| match rule {
            Rule::Exact => gauss2(f, w[0], w[1]),
            Rule::Adaptive(tol) => {
                let share = tol * (w[1] - w[0]) / (hi - lo);
                let (coarse, fine) = (gauss2(f, w[0], w[1]), gauss4(f, w[0], w[1]));
                if (0..2).all(|i| (fine[i] - coarse[i]).abs() <= share) {
                    fine
                } else {
                    adapt(f, w[0], w[1], fine, share, 0)
                }
            }
        })
        .fold([0.0, 0.0], add)
}

/// Material and air, integrated together from the same lines.
type Pair = [f64; 2];

fn add(a: Pair, b: Pair) -> Pair {
    [a[0] + b[0], a[1] + b[1]]
}

fn scaled(a: Pair, k: f64) -> Pair {
    [a[0] * k, a[1] * k]
}

fn gauss2(f: &dyn Fn(f64) -> Pair, x0: f64, x1: f64) -> Pair {
    let (h, c) = (0.5 * (x1 - x0), 0.5 * (x0 + x1));
    let t = h / 3f64.sqrt();
    scaled(add(f(c - t), f(c + t)), h)
}

fn gauss4(f: &dyn Fn(f64) -> Pair, x0: f64, x1: f64) -> Pair {
    let (h, c) = (0.5 * (x1 - x0), 0.5 * (x0 + x1));
    let sum = GAUSS4
        .iter()
        .fold([0.0, 0.0], |acc, &(t, w)| add(acc, scaled(f(c + h * t), w)));
    scaled(sum, h)
}

fn adapt(f: &dyn Fn(f64) -> Pair, x0: f64, x1: f64, whole: Pair, tol: f64, depth: usize) -> Pair {
    let xm = 0.5 * (x0 + x1);
    let (l, r) = (gauss4(f, x0, xm), gauss4(f, xm, x1));
    let both = add(l, r);
    let settled = (0..2).all(|i| (both[i] - whole[i]).abs() <= tol);
    if settled || depth >= MAX_DEPTH {
        return both;
    }
    add(
        adapt(f, x0, xm, l, tol / 2.0, depth + 1),
        adapt(f, xm, x1, r, tol / 2.0, depth + 1),
    )
}
