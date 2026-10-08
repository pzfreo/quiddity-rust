//! Areas and volumes (`BRepGProp`): surface integrals over each face's trimmed parameter
//! domain, reduced by Green's theorem to integrals along the exact edge curves (the region
//! their foot points on the surface bound, which is where OpenCascade's own answer lands once
//! a face's pcurves are projected from those curves) and evaluated by Gauss quadrature to near
//! machine precision (the body signatures families publish round them to twelve significant
//! figures). `tests/face_areas.rs` checks every corpus face against OpenCascade.
//!
//! ∬_D f du dv = −∮ H du with H(u, v) = ∫ f(u, s) ds from a fixed v, or ∮ G dv with
//! G(u, v) = ∫ f(s, v) ds from a fixed u. The first ignores boundary pieces of constant u
//! (seams, which files may omit) and serves faces whose loops run round u; the second ignores
//! pieces of constant v (a sphere's poles, a cone's apex) and serves every other face. Where a
//! loop runs round u and ends at a singular line, H starts from that line, and G starts from a
//! B-spline side that collapses to a point, so the line itself contributes nothing and the
//! parameter left undetermined there does not matter. Along each edge the integral is
//! adaptive (Gauss–Kronrod), split where the foot point turns a corner onto or off the
//! surface's boundary or crosses a knot line. Each loop's direction is read from the geometry
//! (which side of it the face lies on), not from the file's orientation flags.

use std::sync::OnceLock;

use super::brep::Part;
use super::geom::{self, Curve, Surface, V3};
use super::nurbs::breaks;
use super::sampling::{edge_interval, extreme_parameters};
use super::uv::UvLoop;

/// Gauss–Legendre nodes and weights on [-1, 1], in ascending order.
fn gauss_nodes() -> &'static [(f64, f64)] {
    static NODES: OnceLock<Vec<(f64, f64)>> = OnceLock::new();
    NODES.get_or_init(|| {
        let n = 16;
        (0..n)
            .map(|i| {
                // Newton on the Legendre polynomial from Chebyshev's estimate of the root.
                let mut x = (std::f64::consts::PI * (i as f64 + 0.75) / (n as f64 + 0.5)).cos();
                let mut derivative = 0.0;
                for _ in 0..100 {
                    let (mut p0, mut p1) = (1.0, x);
                    for k in 2..=n {
                        let k = k as f64;
                        (p0, p1) = (p1, ((2.0 * k - 1.0) * x * p1 - (k - 1.0) * p0) / k);
                    }
                    derivative = n as f64 * (x * p1 - p0) / (x * x - 1.0);
                    let step = p1 / derivative;
                    x -= step;
                    if step.abs() < 1e-16 {
                        break;
                    }
                }
                (x, 2.0 / ((1.0 - x * x) * derivative * derivative))
            })
            .rev() // ascending, so a panel's nodes are visited in walking order
            .collect()
    })
}

/// ∫ f over the interval running through `cuts` in order, one Gauss panel between each
/// consecutive pair, for every component of f at once.
fn integrate<const N: usize>(cuts: &[f64], f: impl Fn(f64) -> [f64; N]) -> [f64; N] {
    let mut total = [0.0; N];
    for w in cuts.windows(2) {
        let (half, centre) = (0.5 * (w[1] - w[0]), 0.5 * (w[1] + w[0]));
        for &(x, weight) in gauss_nodes() {
            let y = f(centre + half * x);
            for k in 0..N {
                total[k] += weight * half * y[k];
            }
        }
    }
    total
}

/// The cuts from a to b: `panels` equal pieces, each further split at any of `breaks` (where
/// the integrand's polynomial pieces meet) strictly inside it.
fn cuts(a: f64, b: f64, panels: usize, breaks: &[f64]) -> Vec<f64> {
    let panels = panels.max(1);
    let mut out: Vec<f64> = (0..=panels)
        .map(|k| a + (b - a) * k as f64 / panels as f64)
        .chain(breaks.iter().copied().filter(|&x| (x - a) * (x - b) < 0.0))
        .collect();
    out.sort_by(|x, y| {
        if b < a {
            y.total_cmp(x)
        } else {
            x.total_cmp(y)
        }
    });
    out.dedup();
    out
}

/// How an integral along one surface parameter is cut: in one piece where the integrands are
/// polynomial in it (planes, and the straight direction of cylinders and cones), at the knots
/// of a B-spline surface, else every radian.
fn surface_cuts(surface: &Surface, along_v: bool, a: f64, b: f64) -> Vec<f64> {
    match surface {
        Surface::Plane { .. } => vec![a, b],
        Surface::Cylinder { .. } | Surface::Cone { .. } if along_v => vec![a, b],
        Surface::Freeform { surface, .. } => {
            let knots = if along_v {
                breaks(&surface.knots_v, surface.degree_v)
            } else {
                breaks(&surface.knots_u, surface.degree_u)
            };
            cuts(a, b, 1, &knots)
        }
        _ => cuts(a, b, (b - a).abs().ceil() as usize, &[]),
    }
}

/// How an integral along an edge is cut: lines in one piece, conics every half radian, B-spline
/// curves at their knots (and in a few pieces besides, for the surface's own variation) — and
/// wherever the edge passes through a sphere's pole, where u jumps and the integrand with it.
fn curve_cuts(surface: &Surface, curve: &Curve, t0: f64, t1: f64) -> Vec<f64> {
    let mut breaks = match surface {
        // A pole is where the curve is extreme along the axis; a pass near it (within the
        // band boundary samples route along the pole) is cut there too, which is harmless.
        Surface::Sphere { frame, radius } => extreme_parameters(curve, (t0, t1), &[frame.z])
            .into_iter()
            .filter(|&t| {
                let height = geom::dot(geom::sub(curve.value(t), frame.origin), frame.z);
                height.abs() > radius * 0.02f64.cos()
            })
            .collect(),
        _ => Vec::new(),
    };
    match curve {
        Curve::Line { .. } => cuts(t0, t1, 1, &breaks),
        Curve::Circle { .. } | Curve::Ellipse { .. } => {
            cuts(t0, t1, ((t1 - t0).abs() / 0.5).ceil() as usize, &breaks)
        }
        Curve::Nurbs(n) => {
            // A closed curve's edge may run past its domain's end.
            let period = curve.period().unwrap_or(0.0);
            breaks.extend(
                n.breaks()
                    .into_iter()
                    .flat_map(|k| [k - period, k, k + period]),
            );
            cuts(t0, t1, 4, &breaks)
        }
    }
}

/// A B-spline surface's side of constant u, and of constant v, that collapses to a single
/// point (as at the tip of a surface closing like a cone), if any.
fn collapsed_sides(surface: &Surface) -> (Option<f64>, Option<f64>) {
    let Surface::Freeform { surface, .. } = surface else {
        return (None, None);
    };
    let (u0, u1, v0, v1) = surface.domain();
    let (lo, hi) = surface.control_bounds();
    let tiny = 1e-9 * geom::dist(lo, hi).max(1e-300);
    let point = |f: &dyn Fn(f64) -> V3| {
        let first = f(0.0);
        (1..=8).all(|k| geom::dist(f(k as f64 / 8.0), first) <= tiny)
    };
    let side_u = |u: f64| point(&|s| surface.value(u, v0 + (v1 - v0) * s));
    let side_v = |v: f64| point(&|s| surface.value(u0 + (u1 - u0) * s, v));
    (
        [u0, u1].into_iter().find(|&u| side_u(u)),
        [v0, v1].into_iter().find(|&v| side_v(v)),
    )
}

/// The parameter spans across which a B-spline surface closes on itself (its sides of
/// constant u, or of constant v, coincide), infinite where it does not.
fn closed_spans(surface: &Surface) -> (f64, f64) {
    let Surface::Freeform { surface, .. } = surface else {
        return (f64::INFINITY, f64::INFINITY);
    };
    let (u0, u1, v0, v1) = surface.domain();
    let (lo, hi) = surface.control_bounds();
    let tiny = 1e-9 * geom::dist(lo, hi).max(1e-300);
    let same = |f: &dyn Fn(f64) -> (V3, V3)| {
        (0..=8).all(|k| {
            let (a, b) = f(k as f64 / 8.0);
            geom::dist(a, b) <= tiny
        })
    };
    let u_closed = same(&|s| {
        let v = v0 + (v1 - v0) * s;
        (surface.value(u0, v), surface.value(u1, v))
    });
    let v_closed = same(&|s| {
        let u = u0 + (u1 - u0) * s;
        (surface.value(u, v0), surface.value(u, v1))
    });
    (
        if u_closed { u1 - u0 } else { f64::INFINITY },
        if v_closed { v1 - v0 } else { f64::INFINITY },
    )
}

/// A boundary node met on the walk: its curve parameter, its surface parameters and their
/// velocity.
type Met = (f64, (f64, f64), (f64, f64));

/// Where the walk round an edge stands: the parameters the next node unwraps against, how far
/// it has got through the edge's anchors and past any singular point, and the first and latest
/// nodes it has met.
#[derive(Clone, Copy)]
struct Walk {
    last: (f64, f64),
    next_anchor: usize,
    regular_seen: bool,
    in_band: bool,
    crossed: bool,
    first: Option<Met>,
    latest: Option<Met>,
    /// ∫ u dv so far: the parameter-space area the walk sweeps (Green's theorem).
    swept: f64,
}

/// The 21-point Gauss–Kronrod rule on [-1, 1] (QUADPACK's `qk21`) in ascending order: each
/// node with its Kronrod weight and its weight in the embedded 10-point Gauss rule (zero at the
/// Kronrod-only nodes).
fn kronrod_nodes() -> &'static [(f64, f64, f64)] {
    static NODES: OnceLock<Vec<(f64, f64, f64)>> = OnceLock::new();
    NODES.get_or_init(|| {
        const X: [f64; 11] = [
            0.9956571630258081,
            0.9739065285171717,
            0.9301574913557082,
            0.8650633666889845,
            0.7808177265864169,
            0.6794095682990244,
            0.5627571346686047,
            0.4333953941292472,
            0.2943928627014602,
            0.14887433898163122,
            0.0,
        ];
        const K: [f64; 11] = [
            0.011694638867371874,
            0.032558162307964725,
            0.054755896574351995,
            0.07503967481091996,
            0.0931254545836976,
            0.10938715880229764,
            0.12349197626206584,
            0.13470921731147334,
            0.14277593857706009,
            0.14773910490133849,
            0.1494455540029169,
        ];
        const G: [f64; 5] = [
            0.06667134430868814,
            0.1494513491505806,
            0.21908636251598204,
            0.26926671930999635,
            0.29552422471475287,
        ];
        let gauss = |i: usize| if i % 2 == 1 { G[i / 2] } else { 0.0 };
        let lower = (0..11).map(|i| (-X[i], K[i], gauss(i)));
        let upper = (0..10).rev().map(|i| (X[i], K[i], gauss(i)));
        lower.chain(upper).collect()
    })
}

/// One Gauss–Kronrod rule over a panel: the Kronrod sum, the error estimate of its first
/// component (the difference from the embedded Gauss rule), the sum of that component's terms'
/// magnitudes (the scale its error is judged against) and the state after its last node. The
/// first component, an area density wherever it is used, steers the refinement for all: what
/// needs refining is the boundary path, the same for every integrand, and the others may cancel
/// to round-off (the flux through a plane through the origin).
type Panel<S, const N: usize> = ([f64; N], f64, f64, S);

/// The rule over (a, b) of `term`, visited in ascending order of the panel's parameter from
/// `state`.
fn kronrod<S, const N: usize>(
    mut state: S,
    a: f64,
    b: f64,
    mut term: impl FnMut(&mut S, f64) -> Option<[f64; N]>,
) -> Option<Panel<S, N>> {
    let (half, centre) = (0.5 * (b - a), 0.5 * (b + a));
    let (mut sum, mut gauss, mut size) = ([0.0; N], 0.0, 0.0);
    for &(x, wk, wg) in kronrod_nodes() {
        let y = term(&mut state, centre + half * x)?;
        for k in 0..N {
            sum[k] += wk * half * y[k];
        }
        gauss += wg * half * y[0];
        size += (wk * half * y[0]).abs();
    }
    Some((sum, (sum[0] - gauss).abs(), size, state))
}

/// ∫ over the interval running through `cuts`: the sum, the largest error estimate of a panel
/// not resolved relative to its own terms, the sum of the terms' magnitudes, and the state at
/// the end. Without a `floor`, one rule per panel. With one, adaptively: a panel is halved,
/// each half in turn, until its error estimate is below its terms' scale or the floor (the
/// scale below which the integral is not resolved). The integrand is visited in walking order,
/// carrying a state (`eval` runs one rule over (a, b) from a state, and may name a kink inside,
/// where the panel is then split instead of halved), so a refined half restarts from where the
/// walk stood.
fn adaptive<S: Copy, const N: usize>(
    cuts: &[f64],
    start: S,
    floor: Option<f64>,
    eval: &mut impl FnMut(S, f64, f64) -> Option<(Panel<S, N>, Option<f64>)>,
) -> Option<Panel<S, N>> {
    let mut state = start;
    let (mut total, mut excess, mut magnitude) = ([0.0; N], 0.0f64, 0.0);
    let resolved = |error: f64, size: f64, floor: f64| error <= (1e-9 * size).max(floor);
    for w in cuts.windows(2) {
        // Pending panels, the next on top, with how often they have been halved and split at a
        // kink (each finite in number, so splitting there spends no halving).
        let mut stack = vec![(w[0], w[1], 0, 0)];
        while let Some((a, b, depth, kinks)) = stack.pop() {
            let ((sum, error, size, end), kink) = eval(state, a, b)?;
            let settled = match floor {
                None => true,
                Some(floor) => depth >= MAX_DEPTH || resolved(error, size, floor),
            };
            if settled {
                for k in 0..N {
                    total[k] += sum[k];
                }
                magnitude += size;
                if !resolved(error, size, 0.0) {
                    excess = excess.max(error);
                }
                state = end;
            } else {
                let inside = |t: f64| (t - a) * (t - b) < 0.0 && t != a && t != b;
                let (m, depth, kinks) = match kink.filter(|&t| inside(t) && kinks < MAX_KINKS) {
                    Some(t) => (t, depth, kinks + 1),
                    None => (0.5 * (a + b), depth + 1, kinks),
                };
                stack.push((m, b, depth, kinks));
                stack.push((a, m, depth, kinks));
            }
        }
    }
    Some((total, excess, magnitude, state))
}

/// How many times a boundary panel may be halved (to a 4096th), and split at a kink.
const MAX_DEPTH: usize = 12;
const MAX_KINKS: usize = 64;

/// The foot point of `p` on a B-spline surface, given the inversion's answer `(u, v)`: where
/// that is held on the domain's boundary in one parameter, the nearest point along that side.
/// The inversion clamps its two-parameter steps, so the free parameter it settles at is not
/// the side's nearest point, and the boundary integrals need the point whose motion
/// [`uv_velocity`] gives.
fn held_foot(surface: &Surface, p: V3, (mut u, mut v): (f64, f64)) -> (f64, f64) {
    let Surface::Freeform { surface: s, .. } = surface else {
        return (u, v);
    };
    let (u0, u1, v0, v1) = s.domain();
    // Within round-off of a side is on it (the inversion may stop a hair inside).
    let snap = |x: &mut f64, lo: f64, hi: f64| {
        let near = 1e-12 * (hi - lo);
        if *x <= lo + near {
            *x = lo;
        } else if *x >= hi - near {
            *x = hi;
        }
        *x == lo || *x == hi
    };
    let (on_u, on_v) = (snap(&mut u, u0, u1), snap(&mut v, v0, v1));
    if on_u == on_v {
        return (u, v);
    }
    for _ in 0..50 {
        let (q, su, sv) = s.value_and_partials(u, v);
        let r = geom::sub(p, q);
        let (before, x) = if on_v { (u, &mut u) } else { (v, &mut v) };
        let (d, lo, hi) = if on_v { (su, u0, u1) } else { (sv, v0, v1) };
        let dd = geom::dot(d, d);
        if dd <= 0.0 {
            break;
        }
        *x = (before + geom::dot(d, r) / dd).clamp(lo, hi);
        if (*x - before).abs() <= 1e-15 * (hi - lo) {
            break;
        }
    }
    (u, v)
}

/// The surface's point and first partials at (u, v).
fn point_and_partials(surface: &Surface, u: f64, v: f64) -> (V3, V3, V3) {
    match surface {
        Surface::Freeform { surface, .. } => surface.value_and_partials(u, v),
        _ => {
            let (su, sv) = surface.partials(u, v);
            (surface.value(u, v), su, sv)
        }
    }
}

/// The parameter-space velocity (du/dt, dv/dt) of the foot point (u, v) on the surface of a
/// curve point `p` moving at `c`. With r = p − S(u, v) normal to the surface, differentiating
/// r·Su = r·Sv = 0 gives [Su·Su − r·Suu, Su·Sv − r·Suv; Su·Sv − r·Suv, Sv·Sv − r·Svv]·(du, dv)
/// = (Su·c, Sv·c): an edge off its surface (within its tolerance) has its foot point moving
/// faster or slower than its tangent's projection, by the distance over the radius of
/// curvature, and the boundary integrals must follow the nodes they are evaluated at. The
/// second partials, needed only in that small correction, are central differences. A foot
/// point held on a B-spline surface's boundary (the edge runs just outside it) moves only
/// along it: the held parameter's row drops out.
fn uv_velocity(surface: &Surface, u: f64, v: f64, p: V3, c: V3) -> (f64, f64) {
    let (s, su, sv) = point_and_partials(surface, u, v);
    let r = geom::sub(p, s);
    let (mut a, mut b, mut d) = (geom::dot(su, su), geom::dot(su, sv), geom::dot(sv, sv));
    let (p, q) = (geom::dot(su, c), geom::dot(sv, c));
    // Undisturbed, the first fundamental form: at a degenerate point it decides alone.
    let scale = a.max(d);
    let (hu, hv, (u0, u1, v0, v1)) = match surface {
        Surface::Freeform { surface, .. } => {
            let (u0, u1, v0, v1) = surface.domain();
            (1e-5 * (u1 - u0), 1e-5 * (v1 - v0), (u0, u1, v0, v1))
        }
        _ => (1e-5, 1e-5, (f64::MIN, f64::MAX, f64::MIN, f64::MAX)),
    };
    if geom::norm(r) > 0.0 {
        // Differences inside the domain, one-sided at its edges.
        let (ua, ub) = ((u - hu).max(u0), (u + hu).min(u1));
        let (va, vb) = ((v - hv).max(v0), (v + hv).min(v1));
        let (_, su_a, _) = point_and_partials(surface, ua, v);
        let (_, su_b, _) = point_and_partials(surface, ub, v);
        let (_, su_c, sv_c) = point_and_partials(surface, u, va);
        let (_, su_d, sv_d) = point_and_partials(surface, u, vb);
        let along = |x: V3, y: V3, h: f64| geom::dot(r, geom::sub(y, x)) / h;
        a -= along(su_a, su_b, ub - ua);
        b -= along(su_c, su_d, vb - va);
        d -= along(sv_c, sv_d, vb - va);
    }
    let one = |num: f64, den: f64| {
        if den.abs() <= 1e-6 * scale {
            0.0
        } else {
            num / den
        }
    };
    match (u <= u0 || u >= u1, v <= v0 || v >= v1) {
        (true, true) => return (0.0, 0.0),
        (true, false) => return (0.0, one(q, d)),
        (false, true) => return (one(p, a), 0.0),
        (false, false) => {}
    }
    let det = a * d - b * b;
    // At a degenerate point (a pole, a collapsed B-spline edge) the parameters are not
    // determined; such points contribute nothing to the boundary integrals.
    if det.abs() <= 1e-12 * scale * scale {
        return (0.0, 0.0);
    }
    ((d * p - b * q) / det, (a * q - b * p) / det)
}

impl Part {
    /// The solid's volume and area (`solid.volume`, `solid.area`). The volume is a third of the
    /// flux of the position vector out through its faces (a void's faces subtract).
    pub fn solid_mass(&self, solid: usize) -> Option<(f64, f64)> {
        let (mut flux, mut area) = (0.0, 0.0);
        for &face in &self.solids[solid].faces {
            let [a, f] = self.face_mass(face)?;
            area += a;
            flux += if self.faces[face].reversed { -f } else { f };
        }
        Some((flux / 3.0, area))
    }

    /// The face's area and the flux of the position vector through it along the surface's
    /// natural normal.
    pub fn face_mass(&self, face: usize) -> Option<[f64; 2]> {
        *self.cache[face]
            .mass
            .get_or_init(|| self.compute_face_mass(face))
    }

    fn compute_face_mass(&self, face: usize) -> Option<[f64; 2]> {
        let surface = &self.faces[face].surface;
        // A whole sphere has no boundary to integrate along; its area and flux (3V) are exact.
        if let Surface::Sphere { frame, radius } = surface
            && self.whole_sphere(face)
        {
            let sense = if frame.direct() { 1.0 } else { -1.0 };
            return Some([
                4.0 * std::f64::consts::PI * radius * radius,
                sense * 4.0 * std::f64::consts::PI * radius.powi(3),
            ]);
        }
        self.face_integral(face, |u, v| {
            let (p, su, sv) = point_and_partials(surface, u, v);
            let n = geom::cross(su, sv);
            [geom::norm(n), geom::dot(p, n)]
        })
    }

    /// ∬ f du dv over the face's trimmed parameter domain.
    fn face_integral<const N: usize>(
        &self,
        face: usize,
        f: impl Fn(f64, f64) -> [f64; N],
    ) -> Option<[f64; N]> {
        let fc = &self.faces[face];
        let surface = &fc.surface;
        let (pu, pv) = surface.periodic();
        let uv = self.uv_loops(face)?;
        let domain = self.domain(face)?;
        let (_, shifts) = self.loop_placement(face)?;
        // A loop runs round u when its boundary does (on a closed B-spline surface, which the
        // reader does not mark periodic, when it ends most of the parameter range from where
        // it began).
        let open = |lp: &UvLoop, pick: fn(&(f64, f64)) -> f64, far: f64| match (
            lp.points.first(),
            lp.points.last(),
        ) {
            (Some(a), Some(b)) => (pick(b) - pick(a)).abs() > far,
            _ => false,
        };
        let half = |range: (f64, f64)| 0.5 * (range.1 - range.0);
        let far_v = if pv { 1.0 } else { half(domain.v_range()) };
        let along_v = if pu {
            uv.iter().any(|lp| lp.winds_u)
        } else {
            uv.iter()
                .any(|lp| open(lp, |p| p.0, half(domain.u_range())))
        };
        if along_v && uv.iter().any(|lp| open(lp, |p| p.1, far_v)) {
            return None;
        }
        // A face on a closed B-spline surface whose own seam (an edge its loop uses twice) does
        // not lie on the surface's seam crosses the surface's seam somewhere inside its
        // boundary, which the walk does not unwrap: it is not integrated.
        if let Surface::Freeform { surface: s, .. } = surface {
            let spans = closed_spans(surface);
            let (u0, _, v0, _) = s.domain();
            let on_seam = |x: f64, lo: f64, span: f64| {
                let k = ((x - lo) / span).round();
                (x - lo - k * span).abs() <= 1e-6 * span
            };
            for lp in &fc.loops {
                for &(e, _) in &lp.edges {
                    if lp.edges.iter().filter(|x| x.0 == e).count() < 2 {
                        continue;
                    }
                    let ed = &self.edges[e];
                    let (t0, t1) =
                        edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
                    let (u, v) = surface.parameters(ed.curve.value(0.5 * (t0 + t1)), None)?;
                    let seam_u = spans.0.is_finite() && on_seam(u, u0, spans.0);
                    let seam_v = spans.1.is_finite() && on_seam(v, v0, spans.1);
                    if (spans.0.is_finite() || spans.1.is_finite()) && !seam_u && !seam_v {
                        return None;
                    }
                }
            }
        }
        let first = (0..fc.loops.len()).find(|&i| fc.loops[i].vertex.is_none())?;
        let start = uv[first].points.first().copied()?;
        let start = (start.0 + shifts[first].0, start.1 + shifts[first].1);
        // Where a B-spline side collapses to a point, the parameter along it is undetermined
        // beside it, and the inner integral starts from that side, so that it vanishes there as
        // it does from a sphere's pole.
        let collapsed = collapsed_sides(surface);
        let reference = if along_v {
            let (lo, hi) = domain.v_range();
            [lo, hi]
                .into_iter()
                .find_map(|v| surface.singular_v(v).filter(|s| (s - v).abs() < 1e-6))
                .or(collapsed.1)
                .unwrap_or(start.1)
        } else {
            collapsed.0.unwrap_or(start.0)
        };
        let term = |u: f64, v: f64, du: f64, dv: f64| {
            let (weight, y) = if along_v {
                let cuts = surface_cuts(surface, true, reference, v);
                (-du, integrate(&cuts, |s| f(u, s)))
            } else {
                let cuts = surface_cuts(surface, false, reference, u);
                (dv, integrate(&cuts, |s| f(s, v)))
            };
            y.map(|y| weight * y)
        };
        let loops: Vec<usize> = (0..fc.loops.len())
            .filter(|&i| fc.loops[i].vertex.is_none())
            .collect();
        // One rule per panel; then again, refining where needed, for a loop with a panel
        // unresolved at the scale of the whole face's terms.
        let mut passes = Vec::with_capacity(loops.len());
        for &i in &loops {
            passes.push(self.boundary_integral(face, i, shifts[i], &term, None)?);
        }
        let floor = 1e-11 * passes.iter().map(|p| p.2).sum::<f64>();
        let mut total = [0.0; N];
        for (&i, (mut sum, excess, _, swept)) in loops.iter().zip(passes) {
            if excess > floor {
                let refined = self.boundary_integral(face, i, shifts[i], &term, Some(floor))?;
                // A panel still unresolved at the floor after the deepest halving leaves the
                // area unknown to that scale: refused rather than reported as if exact.
                if refined.1 > floor {
                    return None;
                }
                sum = refined.0;
            }
            // A loop that winds round either parameter has no signed area to read.
            let winding = along_v || uv[i].winds_v;
            let direction = self.loop_direction(face, i, (!winding).then_some(swept))?;
            for k in 0..N {
                total[k] += direction * sum[k];
            }
        }
        Some(total)
    }

    /// ∮ `term`(u, v, du/dt, dv/dt) dt round loop *i* of the face in parameter space (placed by
    /// `shift`): along each edge's exact curve, and straight across any gap between one edge's
    /// end and the next one's start (a collapsed B-spline side, a pole), less whole turns.
    fn boundary_integral<const N: usize>(
        &self,
        face: usize,
        i: usize,
        shift: (f64, f64),
        term: &dyn Fn(f64, f64, f64, f64) -> [f64; N],
        floor: Option<f64>,
    ) -> Option<Panel<f64, N>> {
        let fc = &self.faces[face];
        let surface = &fc.surface;
        let (pu, pv) = surface.periodic();
        let lp = &self.uv_loops(face)?[i];
        let unwrap = |(u, v): (f64, f64), near: (f64, f64)| {
            (
                if pu { geom::nearest_turn(u, near.0) } else { u },
                if pv { geom::nearest_turn(v, near.1) } else { v },
            )
        };
        let placed = |p: &(f64, f64)| (p.0 + shift.0, p.1 + shift.1);
        let spans = closed_spans(surface);
        let mut total: Panel<f64, N> = ([0.0; N], 0.0, 0.0, 0.0);
        let mut add = |(sum, excess, size, swept): Panel<f64, N>| {
            for (t, s) in total.0.iter_mut().zip(sum) {
                *t += s;
            }
            total.1 = total.1.max(excess);
            total.2 += size;
            total.3 += swept;
        };
        let mut ends: Vec<((f64, f64), (f64, f64))> = Vec::new();
        let mut last = placed(lp.points.first()?);
        for (k, &(e, forward)) in fc.loops[i].edges.iter().enumerate() {
            // Turns exist only on periodic surfaces; elsewhere the previous node is the better
            // inversion seed (an edge may start on a collapsed B-spline side). The edge starts on
            // the turn of its first regular sample and, past each pole or apex it crosses, takes
            // up the turn of the next.
            let anchors = match lp.anchors.get(k) {
                Some(a) if pu || pv => a.as_slice(),
                _ => &[],
            };
            if let Some(p) = anchors.first() {
                last = placed(p);
            }
            let ed = &self.edges[e];
            let (mut t0, mut t1) =
                edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
            if !forward {
                std::mem::swap(&mut t0, &mut t1);
            }
            // One node: its parameters, unwrapped against the walk's, and their velocity.
            let node = |walk: &mut Walk, t: f64| -> Option<((f64, f64), (f64, f64))> {
                let point = ed.curve.value(t);
                // On a collapsed B-spline side, where the other parameter is undetermined,
                // Newton cannot leave the seed; it starts a little inside instead.
                let seed = match surface {
                    Surface::Freeform { surface: s, .. } => {
                        let (u0, u1, v0, v1) = s.domain();
                        let (u, v) = walk.last;
                        let (_, su, sv) = point_and_partials(surface, u, v);
                        let (a, b) = (geom::norm(su), geom::norm(sv));
                        let inward = |x: f64, lo: f64, hi: f64| {
                            x + 1e-3 * (hi - lo) * if x - lo < hi - x { 1.0 } else { -1.0 }
                        };
                        if b <= 1e-6 * a.max(b) {
                            (inward(u, u0, u1), v)
                        } else if a <= 1e-6 * a.max(b) {
                            (u, inward(v, v0, v1))
                        } else {
                            (u, v)
                        }
                    }
                    _ => walk.last,
                };
                let (mut u, mut v) =
                    held_foot(surface, point, surface.parameters(point, Some(seed))?);
                // On a closed B-spline surface's own seam a point has both parameter values: it
                // takes the one beside the walk. A boundary that crosses that seam away from
                // the face's (a rotated seam) jumps by the span there, which the walk does not
                // unwrap: such a face is not integrated.
                if let Surface::Freeform { surface: s, .. } = surface {
                    let (u0, u1, v0, v1) = s.domain();
                    let side = |x: &mut f64, lo: f64, hi: f64, near: f64, span: f64| {
                        if !span.is_finite() || near == lo || near == hi {
                            return true;
                        }
                        if *x == lo || *x == hi {
                            *x = if (near - lo).abs() <= (near - hi).abs() {
                                lo
                            } else {
                                hi
                            };
                        }
                        (*x - near).abs() <= 0.5 * span
                    };
                    if !side(&mut u, u0, u1, walk.last.0, spans.0)
                        || !side(&mut v, v0, v1, walk.last.1, spans.1)
                    {
                        return None;
                    }
                }
                let (su, sv) = surface.partials(u, v);
                if surface.singular_v(v).is_some() && geom::norm(su) <= 1e-6 * geom::norm(sv) {
                    // At the singular point itself u is round-off: it stays where the walk is,
                    // rather than setting the turn the rest of the edge unwraps against.
                    u = walk.last.0;
                } else if surface.singular_v(v).is_some() {
                    // Through the singular point u jumps (once per pass near it, as the loop's
                    // samples are split).
                    walk.in_band = walk.regular_seen;
                    let jump = (geom::nearest_turn(u, walk.last.0) - walk.last.0).abs();
                    if walk.in_band && !walk.crossed && jump > std::f64::consts::FRAC_PI_2 {
                        if let Some(p) = anchors.get(walk.next_anchor) {
                            walk.last = placed(p);
                            walk.next_anchor += 1;
                        }
                        walk.crossed = true;
                    }
                } else {
                    if walk.in_band
                        && !walk.crossed
                        && let Some(p) = anchors.get(walk.next_anchor)
                    {
                        walk.last = placed(p);
                        walk.next_anchor += 1;
                    }
                    (walk.regular_seen, walk.in_band, walk.crossed) = (true, false, false);
                }
                let (u, v) = unwrap((u, v), walk.last);
                walk.last = (u, v);
                let velocity = uv_velocity(surface, u, v, point, ed.curve.derivative(t));
                walk.first.get_or_insert((t, (u, v), velocity));
                walk.latest = Some((t, (u, v), velocity));
                Some(((u, v), velocity))
            };
            // Where a foot point starts or stops being held on a B-spline surface's boundary
            // its path turns a corner, and where it crosses a knot line the integrand's
            // derivatives jump; a panel is split there, located by bisection on which piece of
            // the surface the foot point is in.
            let knots = match surface {
                Surface::Freeform { surface, .. } => (
                    breaks(&surface.knots_u, surface.degree_u),
                    breaks(&surface.knots_v, surface.degree_v),
                ),
                _ => (Vec::new(), Vec::new()),
            };
            let held = |(u, v): (f64, f64)| match surface {
                Surface::Freeform { surface, .. } => {
                    let (u0, u1, v0, v1) = surface.domain();
                    (
                        u <= u0 || u >= u1,
                        v <= v0 || v >= v1,
                        knots.0.partition_point(|&k| k <= u),
                        knots.1.partition_point(|&k| k <= v),
                    )
                }
                _ => (false, false, 0, 0),
            };
            let corner = |(ta, pa): (f64, (f64, f64)), tb: f64| {
                let (mut lo, mut hi) = (ta, tb);
                for _ in 0..60 {
                    let mid = 0.5 * (lo + hi);
                    match surface.parameters(ed.curve.value(mid), Some(pa)) {
                        Some(q) if held(q) == held(pa) => lo = mid,
                        _ => hi = mid,
                    }
                }
                0.5 * (lo + hi)
            };
            // Nodes are visited in walking order, so periodic parameters unwrap node by node.
            let mut eval = |walk: Walk, a: f64, b: f64| {
                let mut met: Vec<(f64, (f64, f64))> = Vec::with_capacity(21);
                let mut weights = kronrod_nodes().iter().map(|n| 0.5 * (b - a) * n.1);
                let panel = kronrod(walk, a, b, |walk, t| {
                    let ((u, v), (du, dv)) = node(walk, t)?;
                    walk.swept += weights.next().unwrap_or(0.0) * u * dv;
                    met.push((t, (u, v)));
                    Some(term(u, v, du, dv))
                })?;
                let kink = met
                    .windows(2)
                    .find(|w| held(w[0].1) != held(w[1].1))
                    .map(|w| corner(w[0], w[1].0));
                Some((panel, kink))
            };
            let walk = Walk {
                last,
                next_anchor: 1,
                regular_seen: false,
                in_band: false,
                crossed: false,
                first: None,
                latest: None,
                swept: 0.0,
            };
            let cuts = curve_cuts(surface, &ed.curve, t0, t1);
            let (sum, excess, size, walk) = adaptive(&cuts, walk, floor, &mut eval)?;
            add((sum, excess, size, walk.swept));
            last = walk.last;
            // The edge's exact ends, inverted from the nodes beside them; next to a collapsed
            // B-spline side an inversion can jump across it, so one far from where the node's
            // own motion leads is replaced by that extrapolation. An edge too short for its
            // interval to hold a node (its two ends invert to one curve parameter) stands where
            // the walk is.
            let still = (t0, last, (0.0, 0.0));
            let (t_first, p_first, v_first) = walk.first.unwrap_or(still);
            let (t_last, p_last, v_last) = walk.latest.unwrap_or(still);
            let end = |t: f64, from_t: f64, p: (f64, f64), vel: (f64, f64)| {
                let guess = (p.0 + (t - from_t) * vel.0, p.1 + (t - from_t) * vel.1);
                let step = (guess.0 - p.0).hypot(guess.1 - p.1);
                let exact = surface
                    .parameters(ed.curve.value(t), Some(p))
                    .map(|q| unwrap(q, p));
                match exact {
                    Some(q) if (q.0 - guess.0).hypot(q.1 - guess.1) <= 10.0 * step + 1e-9 => q,
                    _ => guess,
                }
            };
            ends.push((
                end(t0, t_first, p_first, v_first),
                end(t1, t_last, p_last, v_last),
            ));
        }
        for k in 0..ends.len() {
            let from = ends[k].1;
            let to = unwrap(ends[(k + 1) % ends.len()].0, from);
            // Across a closed B-spline surface's seam the two ends are one point.
            let across = |x: f64, near: f64, span: f64| {
                if span.is_finite() {
                    x - span * ((x - near) / span).round()
                } else {
                    x
                }
            };
            let to = (across(to.0, from.0, spans.0), across(to.1, from.1, spans.1));
            let (gu, gv) = (to.0 - from.0, to.1 - from.1);
            if gu.hypot(gv) < 1e-12 {
                continue;
            }
            let mut eval = |(): (), a: f64, b: f64| {
                let panel = kronrod((), a, b, |(), s| {
                    Some(term(from.0 + gu * s, from.1 + gv * s, gu, gv))
                })?;
                Some((panel, None))
            };
            let pieces = (gu.hypot(gv).ceil() as usize).max(1);
            let (sum, excess, size, ()) =
                adaptive(&cuts(0.0, 1.0, pieces, &[]), (), floor, &mut eval)?;
            add((sum, excess, size, gv * (from.0 + 0.5 * gu)));
        }
        Some(total)
    }

    /// +1 when the face lies to the left of loop *i* walked in its listed direction (so the
    /// walk runs anticlockwise round the face's parameter domain), −1 when to the right.
    ///
    /// A loop closed in parameter space has the face on its left when it runs anticlockwise
    /// round the outer boundary or clockwise round a hole; `swept` is the signed parameter area
    /// its walk encloses (the walk's own lift of the loop, so it agrees with the integrals). A
    /// loop running round a periodic direction (`None`) encloses nothing, so step either side
    /// of its longest edge used once by the face.
    fn loop_direction(&self, face: usize, i: usize, swept: Option<f64>) -> Option<f64> {
        if let Some(swept) = swept {
            let (outer, _) = self.loop_placement(face)?;
            let anticlockwise = if swept > 0.0 { 1.0 } else { -1.0 };
            return Some(if i == outer {
                anticlockwise
            } else {
                -anticlockwise
            });
        }
        let fc = &self.faces[face];
        let surface = &fc.surface;
        let domain = self.domain(face)?;
        let uses = |e: usize| {
            fc.loops
                .iter()
                .flat_map(|l| &l.edges)
                .filter(|x| x.0 == e)
                .count()
        };
        let mut edges: Vec<(usize, bool)> = fc.loops[i]
            .edges
            .iter()
            .copied()
            .filter(|&(e, _)| uses(e) == 1)
            .collect();
        let length = |e: usize| {
            let s = &self.edges[e].samples;
            s.windows(2).map(|w| geom::dist(w[0], w[1])).sum::<f64>()
        };
        edges.sort_by(|a, b| length(b.0).total_cmp(&length(a.0)));
        let (du_range, dv_range) = (domain.u_range(), domain.v_range());
        let scale = (du_range.1 - du_range.0)
            .max(dv_range.1 - dv_range.0)
            .max(1e-9);
        for (e, forward) in edges {
            let ed = &self.edges[e];
            let (t0, t1) =
                edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
            let t = 0.5 * (t0 + t1);
            let Some((u, v)) = surface.parameters(ed.curve.value(t), None) else {
                continue;
            };
            let (mut du, mut dv) =
                uv_velocity(surface, u, v, ed.curve.value(t), ed.curve.derivative(t));
            if (t1 >= t0) != forward {
                (du, dv) = (-du, -dv);
            }
            let norm = du.hypot(dv);
            if norm == 0.0 {
                continue;
            }
            for step in [1e-6, 1e-4] {
                let (nu, nv) = (-dv / norm * step * scale, du / norm * step * scale);
                let left = domain.contains(u + nu, v + nv);
                let right = domain.contains(u - nu, v - nv);
                if left != right {
                    return Some(if left { 1.0 } else { -1.0 });
                }
            }
        }
        None
    }
}

/// A face's first moments: what revision matching fingerprints a face by.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceMoments {
    pub area: f64,
    /// The area-weighted mean point of the face (on a curved face, generally off it).
    pub centroid: V3,
    /// ∬ n dA with n the outward unit normal: the face's projected area along each axis. Zero for
    /// a closed face (a whole sphere) and for a full cylinder band.
    pub area_vector: V3,
}

impl Part {
    /// The face's area, centroid and outward area vector, by the same boundary quadrature as
    /// [`Part::face_mass`]; `None` where that cannot integrate the face.
    pub fn face_moments(&self, face: usize) -> Option<FaceMoments> {
        let fc = &self.faces[face];
        let surface = &fc.surface;
        if let Surface::Sphere { frame, radius } = surface
            && self.whole_sphere(face)
        {
            return Some(FaceMoments {
                area: 4.0 * std::f64::consts::PI * radius * radius,
                centroid: frame.origin,
                area_vector: [0.0; 3],
            });
        }
        // One pass for all seven, the area density first (it steers the refinement).
        let [area, mx, my, mz, nx, ny, nz] = self.face_integral(face, |u, v| {
            let (p, su, sv) = point_and_partials(surface, u, v);
            let n = geom::cross(su, sv);
            let w = geom::norm(n);
            [w, p[0] * w, p[1] * w, p[2] * w, n[0], n[1], n[2]]
        })?;
        if area == 0.0 || area.is_nan() {
            return None;
        }
        let sense = if fc.reversed { -1.0 } else { 1.0 };
        Some(FaceMoments {
            area: area.abs(),
            centroid: [mx / area, my / area, mz / area],
            area_vector: [nx, ny, nz].map(|c| sense * c * area.signum()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gauss_rule_is_exact_for_polynomials() {
        let pi = std::f64::consts::PI;
        let [x7, s] = integrate(&cuts(0.0, pi, 3, &[1.0]), |x| [x.powi(7), x.sin()]);
        assert!((x7 - pi.powi(8) / 8.0).abs() < 1e-11);
        assert!((s - 2.0).abs() < 1e-15);
        assert_eq!(cuts(2.0, 0.0, 2, &[0.5, 3.0]), vec![2.0, 1.0, 0.5, 0.0]);
    }

    #[test]
    fn kronrod_rule_and_its_gauss_rule_are_exact_to_their_degrees() {
        // The 21-point Kronrod rule integrates degree 31 exactly, its 10-point Gauss rule 19.
        let (sum, error, _, ()) = kronrod((), -1.0, 1.0, |(), x| Some([x.powi(30)])).unwrap();
        assert!((sum[0] - 2.0 / 31.0).abs() < 1e-15);
        assert!(error > 1e-6);
        let (sum, error, _, ()) = kronrod((), 0.0, 2.0, |(), x| Some([x.powi(19)])).unwrap();
        assert!((sum[0] - 2f64.powi(20) / 20.0).abs() < 1e-9);
        assert!(error < 1e-9);
        // Halving where the integrand turns a corner: |x − 1/3| on [0, 1].
        let mut eval = |(): (), a: f64, b: f64| {
            let panel = kronrod((), a, b, |(), x| Some([(x - 1.0 / 3.0).abs()]))?;
            Some((panel, None))
        };
        let (sum, ..) = adaptive(&[0.0, 1.0], (), Some(0.0), &mut eval).unwrap();
        assert!((sum[0] - 5.0 / 18.0).abs() < 1e-9);
        let mut split = |(): (), a: f64, b: f64| {
            let panel = kronrod((), a, b, |(), x| Some([(x - 1.0 / 3.0).abs()]))?;
            Some((panel, Some(1.0 / 3.0)))
        };
        let (sum, ..) = adaptive(&[0.0, 1.0], (), Some(0.0), &mut split).unwrap();
        assert!((sum[0] - 5.0 / 18.0).abs() < 1e-15);
    }
}
