//! Rational B-spline curves and surfaces: evaluation, point inversion and ray intersection.
//!
//! Every surface the analytic set does not cover (B-splines, extrusions, revolutions) reaches the
//! recognisers in this form, via `step-io`'s exact NURBS conversion.

use super::geom::{self, V3};

fn find_span(knots: &[f64], degree: usize, n: usize, t: f64) -> usize {
    // n = number of control points; valid spans are degree..n-1: the last whose knot is <= t.
    degree + knots[degree + 1..n.max(degree + 1)].partition_point(|&k| k <= t)
}

/// OpenCascade's largest B-spline degree; basis values live on the stack up to it.
const MAX_DEGREE: usize = 25;

/// Up to `MAX_DEGREE + 1` basis values, read as a slice.
#[derive(Clone, Copy)]
struct Basis {
    values: [f64; MAX_DEGREE + 1],
    len: usize,
}

impl std::ops::Deref for Basis {
    type Target = [f64];
    fn deref(&self) -> &[f64] {
        &self.values[..self.len]
    }
}

/// The `degree + 1` non-zero basis functions at *t* in *span* (Piegl & Tiller A2.2).
fn basis(knots: &[f64], degree: usize, span: usize, t: f64) -> Basis {
    let mut n = [0.0; MAX_DEGREE + 1];
    let mut left = [0.0; MAX_DEGREE + 1];
    let mut right = [0.0; MAX_DEGREE + 1];
    n[0] = 1.0;
    for j in 1..=degree {
        left[j] = t - knots[span + 1 - j];
        right[j] = knots[span + j] - t;
        let mut saved = 0.0;
        for r in 0..j {
            let denom = right[r + 1] + left[j - r];
            let temp = if denom.abs() < 1e-300 {
                0.0
            } else {
                n[r] / denom
            };
            n[r] = saved + right[r + 1] * temp;
            saved = left[j - r] * temp;
        }
        n[j] = saved;
    }
    Basis {
        values: n,
        len: degree + 1,
    }
}

/// The basis functions and their first derivatives at *t* in *span*.
fn basis_and_derivative(knots: &[f64], degree: usize, span: usize, t: f64) -> (Basis, Basis) {
    let n = basis(knots, degree, span, t);
    let mut d = Basis {
        values: [0.0; MAX_DEGREE + 1],
        len: degree + 1,
    };
    if degree == 0 {
        return (n, d);
    }
    // dN(i,p) = p (N(i,p-1) / (k[i+p]-k[i]) - N(i+1,p-1) / (k[i+p+1]-k[i+1])).
    let lower = basis(knots, degree - 1, span, t);
    let p = degree as f64;
    let ratio = |num: f64, den: f64| if den.abs() < 1e-300 { 0.0 } else { num / den };
    for j in 0..=degree {
        let i = span - degree + j;
        let left = if j >= 1 { lower[j - 1] } else { 0.0 };
        let right = if j < degree { lower[j] } else { 0.0 };
        d.values[j] = p
            * (ratio(left, knots[i + degree] - knots[i])
                - ratio(right, knots[i + degree + 1] - knots[i + 1]));
    }
    (n, d)
}

/// A rational B-spline curve in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct NurbsCurve {
    degree: usize,
    control_points: Vec<V3>,
    weights: Vec<f64>,
    knots: Vec<f64>,
    closed: bool,
}

/// Whether the arrays describe a well-formed B-spline: `knots = n + degree + 1`, positive
/// weights, non-decreasing knots and a non-empty domain.
fn well_formed(degree: usize, n: usize, weights: usize, knots: &[f64]) -> bool {
    degree <= MAX_DEGREE
        && n > degree
        && weights == n
        && knots.len() == n + degree + 1
        && knots.windows(2).all(|w| w[0] <= w[1])
        && knots[degree] < knots[knots.len() - degree - 1]
}

impl NurbsCurve {
    /// `None` for malformed arrays, so a bad file degrades to an unresolved curve rather than
    /// panicking in evaluation.
    pub fn new(
        degree: usize,
        control_points: Vec<V3>,
        weights: Vec<f64>,
        knots: Vec<f64>,
    ) -> Option<Self> {
        if !well_formed(degree, control_points.len(), weights.len(), &knots)
            || weights.iter().any(|w| *w <= 0.0)
        {
            return None;
        }
        let mut curve = NurbsCurve {
            degree,
            control_points,
            weights,
            knots,
            closed: false,
        };
        let (lo, hi) = curve.domain();
        let size = curve
            .control_points
            .iter()
            .flatten()
            .fold(1.0f64, |m, c| m.max(c.abs()));
        curve.closed = geom::dist(curve.value(lo), curve.value(hi)) <= 1e-9 * size;
        Some(curve)
    }

    pub fn domain(&self) -> (f64, f64) {
        (
            self.knots[self.degree],
            self.knots[self.knots.len() - self.degree - 1],
        )
    }

    pub fn value(&self, t: f64) -> V3 {
        let (lo, hi) = self.domain();
        let t = t.clamp(lo, hi);
        let n = self.control_points.len();
        let span = find_span(&self.knots, self.degree, n, t);
        let b = basis(&self.knots, self.degree, span, t);
        let mut acc = [0.0; 4];
        for (j, bj) in b.iter().enumerate() {
            let i = span - self.degree + j;
            let w = self.weights[i] * bj;
            let c = self.control_points[i];
            acc = [
                acc[0] + c[0] * w,
                acc[1] + c[1] * w,
                acc[2] + c[2] * w,
                acc[3] + w,
            ];
        }
        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]]
    }

    /// The exact first derivative (quotient rule on the homogeneous form).
    pub fn derivative(&self, t: f64) -> V3 {
        let (lo, hi) = self.domain();
        let t = t.clamp(lo, hi);
        let span = find_span(&self.knots, self.degree, self.control_points.len(), t);
        let (b, db) = basis_and_derivative(&self.knots, self.degree, span, t);
        let (mut a, mut a_t) = ([0.0; 4], [0.0; 4]);
        for j in 0..b.len() {
            let i = span - self.degree + j;
            let w = self.weights[i];
            let c = self.control_points[i];
            let h = [c[0] * w, c[1] * w, c[2] * w, w];
            for k in 0..4 {
                a[k] += b[j] * h[k];
                a_t[k] += db[j] * h[k];
            }
        }
        [0, 1, 2].map(|k| (a_t[k] - a_t[3] * a[k] / a[3]) / a[3])
    }
}

/// The distinct knots across a B-spline's domain, where its polynomial pieces meet.
pub fn breaks(knots: &[f64], degree: usize) -> Vec<f64> {
    let (lo, hi) = (knots[degree], knots[knots.len() - degree - 1]);
    let mut out: Vec<f64> = knots
        .iter()
        .copied()
        .filter(|k| (lo..=hi).contains(k))
        .collect();
    out.dedup();
    out
}

impl NurbsCurve {
    pub fn breaks(&self) -> Vec<f64> {
        breaks(&self.knots, self.degree)
    }

    /// Whether the curve ends where it starts, so that it can be run round as a periodic curve.
    pub fn is_closed(&self) -> bool {
        let (lo, hi) = self.domain();
        let size = self
            .control_points
            .iter()
            .flatten()
            .fold(1.0f64, |m, c| m.max(c.abs()));
        geom::dist(self.value(lo), self.value(hi)) <= 1e-9 * size
    }

    /// The parameter of the curve point nearest *p*: the closest of a dense sampling, refined
    /// by Newton steps on the squared distance.
    pub fn invert(&self, p: V3) -> f64 {
        let (lo, hi) = self.domain();
        let n = 512;
        let mut t = (0..=n)
            .map(|i| lo + (hi - lo) * i as f64 / n as f64)
            .map(|t| (geom::dist(self.value(t), p), t))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .expect("samples")
            .1;
        let h = (hi - lo) * 1e-7;
        for _ in 0..30 {
            let (ta, tb) = ((t - h).max(lo), (t + h).min(hi));
            let d = geom::scale(geom::sub(self.value(tb), self.value(ta)), 1.0 / (tb - ta));
            let dd = geom::dot(d, d);
            if dd < 1e-300 {
                break;
            }
            let step = geom::dot(geom::sub(p, self.value(t)), d) / dd;
            let next = (t + step).clamp(lo, hi);
            if (next - t).abs() < 1e-15 * (1.0 + t.abs()) {
                t = next;
                break;
            }
            t = next;
        }
        t
    }
}

/// A rational B-spline surface, control points indexed `[u][v]`.
#[derive(Clone, Debug)]
pub struct NurbsSurface {
    pub degree_u: usize,
    pub degree_v: usize,
    pub control_points: Vec<Vec<V3>>,
    pub weights: Vec<Vec<f64>>,
    pub knots_u: Vec<f64>,
    pub knots_v: Vec<f64>,
    /// A sampled grid over the domain: seeds for inversion and ray intersection, built on
    /// first use so that reading a file pays nothing for surfaces no query touches.
    grid: std::sync::OnceLock<Grid>,
}

#[derive(Clone, Debug, Default)]
struct Grid {
    us: Vec<f64>,
    vs: Vec<f64>,
    points: Vec<Vec<V3>>,
}

/// How far (mm) a hinted inversion may land from its point before a global search is tried.
const HINT_RETRY_DISTANCE: f64 = 0.05;

/// Grid subdivisions per knot span in each direction.
const GRID_PER_SPAN: usize = 6;

fn grid_params(knots: &[f64], degree: usize) -> Vec<f64> {
    let (lo, hi) = (knots[degree], knots[knots.len() - degree - 1]);
    let mut out = Vec::new();
    for w in breaks(knots, degree).windows(2) {
        for i in 0..GRID_PER_SPAN {
            out.push(w[0] + (w[1] - w[0]) * i as f64 / GRID_PER_SPAN as f64);
        }
    }
    out.push(hi);
    // Very coarse surfaces (one span) still need enough samples to seed Newton.
    if out.len() < 17 {
        out = (0..=16).map(|i| lo + (hi - lo) * i as f64 / 16.0).collect();
    }
    out
}

impl NurbsSurface {
    /// `None` for malformed arrays (see [`NurbsCurve::new`]).
    pub fn new(
        degree_u: usize,
        degree_v: usize,
        control_points: Vec<Vec<V3>>,
        weights: Vec<Vec<f64>>,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
    ) -> Option<Self> {
        let nv = control_points.first().map_or(0, Vec::len);
        let rows_ok = control_points.len() == weights.len()
            && control_points
                .iter()
                .zip(&weights)
                .all(|(c, w)| c.len() == nv && w.len() == nv)
            && weights.iter().flatten().all(|w| *w > 0.0);
        if !rows_ok
            || !well_formed(degree_u, control_points.len(), weights.len(), &knots_u)
            || !well_formed(degree_v, nv, nv, &knots_v)
        {
            return None;
        }
        Some(NurbsSurface {
            degree_u,
            degree_v,
            control_points,
            weights,
            knots_u,
            knots_v,
            grid: std::sync::OnceLock::new(),
        })
    }

    /// A box that certainly contains the surface: its control points' (the convex-hull property).
    pub fn control_bounds(&self) -> (V3, V3) {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in self.control_points.iter().flatten() {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        (lo, hi)
    }

    fn grid(&self) -> &Grid {
        self.grid.get_or_init(|| {
            let us = grid_params(&self.knots_u, self.degree_u);
            let vs = grid_params(&self.knots_v, self.degree_v);
            let points = us
                .iter()
                .map(|&u| vs.iter().map(|&v| self.value(u, v)).collect())
                .collect();
            Grid { us, vs, points }
        })
    }

    pub fn domain(&self) -> (f64, f64, f64, f64) {
        let (pu, pv) = (self.degree_u, self.degree_v);
        (
            self.knots_u[pu],
            self.knots_u[self.knots_u.len() - pu - 1],
            self.knots_v[pv],
            self.knots_v[self.knots_v.len() - pv - 1],
        )
    }

    pub fn value(&self, u: f64, v: f64) -> V3 {
        let (u0, u1, v0, v1) = self.domain();
        let (u, v) = (u.clamp(u0, u1), v.clamp(v0, v1));
        let nu = self.control_points.len();
        let nv = self.control_points[0].len();
        let su = find_span(&self.knots_u, self.degree_u, nu, u);
        let sv = find_span(&self.knots_v, self.degree_v, nv, v);
        let bu = basis(&self.knots_u, self.degree_u, su, u);
        let bv = basis(&self.knots_v, self.degree_v, sv, v);
        let mut acc = [0.0; 4];
        for (a, bua) in bu.iter().enumerate() {
            let i = su - self.degree_u + a;
            for (b, bvb) in bv.iter().enumerate() {
                let j = sv - self.degree_v + b;
                let w = self.weights[i][j] * bua * bvb;
                let c = self.control_points[i][j];
                acc = [
                    acc[0] + c[0] * w,
                    acc[1] + c[1] * w,
                    acc[2] + c[2] * w,
                    acc[3] + w,
                ];
            }
        }
        [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3]]
    }

    /// The point and its exact first partial derivatives (quotient rule on the homogeneous form).
    pub fn value_and_partials(&self, u: f64, v: f64) -> (V3, V3, V3) {
        let (u0, u1, v0, v1) = self.domain();
        let (u, v) = (u.clamp(u0, u1), v.clamp(v0, v1));
        let nu = self.control_points.len();
        let nv = self.control_points[0].len();
        let su = find_span(&self.knots_u, self.degree_u, nu, u);
        let sv = find_span(&self.knots_v, self.degree_v, nv, v);
        let (bu, du) = basis_and_derivative(&self.knots_u, self.degree_u, su, u);
        let (bv, dv) = basis_and_derivative(&self.knots_v, self.degree_v, sv, v);
        let (mut a, mut a_u, mut a_v) = ([0.0; 4], [0.0; 4], [0.0; 4]);
        for i in 0..bu.len() {
            let ci = su - self.degree_u + i;
            for j in 0..bv.len() {
                let cj = sv - self.degree_v + j;
                let w = self.weights[ci][cj];
                let c = self.control_points[ci][cj];
                let h = [c[0] * w, c[1] * w, c[2] * w, w];
                for k in 0..4 {
                    a[k] += bu[i] * bv[j] * h[k];
                    a_u[k] += du[i] * bv[j] * h[k];
                    a_v[k] += bu[i] * dv[j] * h[k];
                }
            }
        }
        let point = [a[0] / a[3], a[1] / a[3], a[2] / a[3]];
        let derive = |d: [f64; 4]| [0, 1, 2].map(|k| (d[k] - d[3] * point[k]) / a[3]);
        (point, derive(a_u), derive(a_v))
    }

    pub fn invert(&self, p: V3, hint: Option<(f64, f64)>) -> (f64, f64) {
        let (mut u, mut v) = hint.unwrap_or_else(|| self.nearest_grid(p));
        let (u0, u1, v0, v1) = self.domain();
        let settle = 1e-12 * (u1 - u0).abs().max((v1 - v0).abs()).max(1e-300);
        let mut residual = f64::INFINITY;
        for _ in 0..50 {
            let (s, su, sv) = self.value_and_partials(u, v);
            let r = geom::sub(p, s);
            residual = geom::norm(r);
            let (a, b, c) = (geom::dot(su, su), geom::dot(su, sv), geom::dot(sv, sv));
            let (g0, g1) = (geom::dot(su, r), geom::dot(sv, r));
            let det = a * c - b * b;
            if det.abs() < 1e-300 {
                break;
            }
            let du = (c * g0 - b * g1) / det;
            let dv = (a * g1 - b * g0) / det;
            let (nu, nv) = ((u + du).clamp(u0, u1), (v + dv).clamp(v0, v1));
            let moved = (nu - u).abs() + (nv - v).abs();
            u = nu;
            v = nv;
            if moved < settle {
                break;
            }
        }
        // Boundary samples sit off their surface by up to the edge tolerance; only a hinted
        // start that lands clearly further away has found a wrong local minimum.
        if hint.is_some() && residual > HINT_RETRY_DISTANCE {
            let fresh = self.invert(p, None);
            if geom::dist(self.value(fresh.0, fresh.1), p) < residual {
                return fresh;
            }
        }
        (u, v)
    }

    fn nearest_grid(&self, p: V3) -> (f64, f64) {
        let g = self.grid();
        let mut best = (f64::INFINITY, 0, 0);
        for (i, row) in g.points.iter().enumerate() {
            for (j, q) in row.iter().enumerate() {
                let d = geom::dist(*q, p);
                if d < best.0 {
                    best = (d, i, j);
                }
            }
        }
        (g.us[best.1], g.vs[best.2])
    }

    /// Ray hits `(t, u, v)`, plus a grazing flag when any hit is near-tangent.
    pub fn ray_hits(&self, origin: V3, dir: V3, t_max: f64) -> (Vec<(f64, f64, f64)>, bool) {
        let g = self.grid();
        let mut seeds = Vec::new();
        for i in 0..g.us.len() - 1 {
            for j in 0..g.vs.len() - 1 {
                let quad = [
                    g.points[i][j],
                    g.points[i + 1][j],
                    g.points[i + 1][j + 1],
                    g.points[i][j + 1],
                ];
                // Seed from any triangle the ray passes near, padded for the chordal error.
                let pad = 0.25 * geom::dist(quad[0], quad[2]).max(geom::dist(quad[1], quad[3]));
                for tri in [[0, 1, 2], [0, 2, 3]] {
                    if let Some((t, a, b)) =
                        ray_triangle(origin, dir, quad[tri[0]], quad[tri[1]], quad[tri[2]], pad)
                    {
                        let (pu, pv) = match tri {
                            [0, 1, 2] => (
                                g.us[i] + (g.us[i + 1] - g.us[i]) * (a + b),
                                g.vs[j] + (g.vs[j + 1] - g.vs[j]) * b,
                            ),
                            _ => (
                                g.us[i] + (g.us[i + 1] - g.us[i]) * a,
                                g.vs[j] + (g.vs[j + 1] - g.vs[j]) * (a + b),
                            ),
                        };
                        seeds.push((
                            t,
                            pu.clamp(g.us[i], g.us[i + 1]),
                            pv.clamp(g.vs[j], g.vs[j + 1]),
                        ));
                    }
                }
            }
        }
        let (u0, u1, v0, v1) = self.domain();
        let mut hits: Vec<(f64, f64, f64)> = Vec::new();
        let mut grazing = false;
        for (t, u, v) in seeds {
            let (mut t, mut u, mut v) = (t, u, v);
            let mut converged = false;
            for _ in 0..40 {
                let s = self.value(u, v);
                let f = geom::sub(s, geom::add(origin, geom::scale(dir, t)));
                if geom::norm(f) < 1e-10 {
                    converged = true;
                    break;
                }
                let (_, su, sv) = self.value_and_partials(u, v);
                // Solve [su sv -dir] (du dv dt) = -f by Cramer's rule.
                let m = [su, sv, geom::scale(dir, -1.0)];
                let det = geom::dot(m[0], geom::cross(m[1], m[2]));
                if det.abs() < 1e-300 {
                    break;
                }
                let rhs = geom::scale(f, -1.0);
                let du = geom::dot(rhs, geom::cross(m[1], m[2])) / det;
                let dv = geom::dot(m[0], geom::cross(rhs, m[2])) / det;
                let dt = geom::dot(m[0], geom::cross(m[1], rhs)) / det;
                u = (u + du).clamp(u0, u1);
                v = (v + dv).clamp(v0, v1);
                t += dt;
            }
            if !converged || t <= 1e-12 || t > t_max {
                continue;
            }
            if hits
                .iter()
                .any(|h| (h.0 - t).abs() < 1e-7 * (1.0 + t.abs()))
            {
                continue;
            }
            let (_, su, sv) = self.value_and_partials(u, v);
            let normal = geom::cross(su, sv);
            let cos = geom::dot(normal, dir).abs() / (geom::norm(normal).max(1e-300));
            if cos < 1e-6 {
                grazing = true;
            }
            hits.push((t, u, v));
        }
        (hits, grazing)
    }
}

/// Möller–Trumbore, returning `(t, a, b)` barycentrics; *pad* widens the triangle's acceptance.
fn ray_triangle(o: V3, d: V3, p0: V3, p1: V3, p2: V3, pad: f64) -> Option<(f64, f64, f64)> {
    let e1 = geom::sub(p1, p0);
    let e2 = geom::sub(p2, p0);
    let h = geom::cross(d, e2);
    let det = geom::dot(e1, h);
    if det.abs() < 1e-300 {
        return None;
    }
    let s = geom::sub(o, p0);
    let a = geom::dot(s, h) / det;
    let q = geom::cross(s, e1);
    let b = geom::dot(d, q) / det;
    let t = geom::dot(e2, q) / det;
    let size = geom::norm(e1).max(geom::norm(e2)).max(1e-300);
    let slack = pad / size;
    if a < -slack || b < -slack || a + b > 1.0 + slack {
        return None;
    }
    Some((t, a.clamp(0.0, 1.0), b.clamp(0.0, 1.0 - a.clamp(0.0, 1.0))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rational biquadratic quarter-cylinder patch: exact circle in u, line in v.
    fn quarter_cylinder() -> NurbsSurface {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let rows = [[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let control_points = rows
            .iter()
            .map(|&[x, y]| vec![[x, y, 0.0], [x, y, 2.0]])
            .collect();
        let weights = vec![vec![1.0, 1.0], vec![w, w], vec![1.0, 1.0]];
        NurbsSurface::new(
            2,
            1,
            control_points,
            weights,
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
            vec![0.0, 0.0, 1.0, 1.0],
        )
        .unwrap()
    }

    #[test]
    fn analytic_partials_match_finite_differences() {
        let s = quarter_cylinder();
        for (u, v) in [(0.2, 0.3), (0.5, 0.5), (0.9, 0.1)] {
            let (p, su, sv) = s.value_and_partials(u, v);
            assert!(
                (p[0].hypot(p[1]) - 1.0).abs() < 1e-12,
                "on the unit cylinder"
            );
            let h = 1e-6;
            let fu = geom::scale(geom::sub(s.value(u + h, v), s.value(u - h, v)), 0.5 / h);
            let fv = geom::scale(geom::sub(s.value(u, v + h), s.value(u, v - h)), 0.5 / h);
            assert!(geom::dist(su, fu) < 1e-6 && geom::dist(sv, fv) < 1e-6);
        }
    }

    #[test]
    fn inversion_recovers_parameters() {
        let s = quarter_cylinder();
        for (u, v) in [(0.1, 0.9), (0.6, 0.4)] {
            let (iu, iv) = s.invert(s.value(u, v), None);
            assert!((iu - u).abs() < 1e-9 && (iv - v).abs() < 1e-9);
        }
    }

    #[test]
    fn ray_hits_find_the_patch() {
        let s = quarter_cylinder();
        let (hits, grazing) =
            s.ray_hits([0.0, 0.0, 1.0], geom::unit([1.0, 1.0, 0.0]).unwrap(), 10.0);
        assert_eq!(hits.len(), 1);
        assert!(!grazing);
        assert!((hits[0].0 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn malformed_arrays_are_refused() {
        assert!(NurbsCurve::new(2, vec![[0.0; 3]; 3], vec![1.0; 3], vec![0.0, 1.0]).is_none());
        assert!(
            NurbsCurve::new(
                1,
                vec![[0.0; 3], [1.0; 3]],
                vec![1.0, -1.0],
                vec![0.0, 0.0, 1.0, 1.0]
            )
            .is_none()
        );
        assert!(
            NurbsSurface::new(
                1,
                1,
                vec![vec![[0.0; 3]; 2]; 2],
                vec![vec![1.0; 2]; 1],
                vec![0.0, 0.0, 1.0, 1.0],
                vec![0.0, 0.0, 1.0, 1.0]
            )
            .is_none()
        );
    }

    #[test]
    fn closed_curves_are_detected() {
        let square = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
        ];
        let closed =
            NurbsCurve::new(1, square, vec![1.0; 4], vec![0.0, 0.0, 1.0, 2.0, 3.0, 3.0]).unwrap();
        assert!(closed.is_closed());
        let open = NurbsCurve::new(
            1,
            vec![[0.0; 3], [1.0, 0.0, 0.0]],
            vec![1.0; 2],
            vec![0.0, 0.0, 1.0, 1.0],
        )
        .unwrap();
        assert!(!open.is_closed());
        assert!((open.invert([0.25, 0.1, 0.0]) - 0.25).abs() < 1e-12);
    }

    #[test]
    fn curve_derivative_matches_finite_differences() {
        let h = std::f64::consts::FRAC_1_SQRT_2;
        let arc = NurbsCurve::new(
            2,
            vec![[1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            vec![1.0, h, 1.0],
            vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        )
        .unwrap();
        for t in [0.1, 0.5, 0.9] {
            let d = arc.derivative(t);
            let e = 1e-6;
            let (p, q) = (arc.value(t + e), arc.value(t - e));
            for k in 0..3 {
                assert!((d[k] - (p[k] - q[k]) / (2.0 * e)).abs() < 1e-7);
            }
            // Tangent to the unit circle.
            assert!(crate::kernel::geom::dot(d, arc.value(t)).abs() < 1e-12);
        }
    }
}
