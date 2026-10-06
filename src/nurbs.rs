//! Rational B-spline curves and surfaces: evaluation, point inversion and ray intersection.
//!
//! Every surface the analytic set does not cover (B-splines, extrusions, revolutions) reaches the
//! recognisers in this form, via `step-io`'s exact NURBS conversion.

use crate::geom::{self, V3};

fn find_span(knots: &[f64], degree: usize, n: usize, t: f64) -> usize {
    // n = number of control points; valid spans are degree..n-1.
    let mut span = degree;
    while span + 1 < n && knots[span + 1] <= t {
        span += 1;
    }
    span
}

/// The `degree + 1` non-zero basis functions at *t* in *span* (Piegl & Tiller A2.2).
fn basis(knots: &[f64], degree: usize, span: usize, t: f64) -> Vec<f64> {
    let mut n = vec![0.0; degree + 1];
    let mut left = vec![0.0; degree + 1];
    let mut right = vec![0.0; degree + 1];
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
    n
}

/// A rational B-spline curve in world coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct NurbsCurve {
    pub degree: usize,
    pub control_points: Vec<V3>,
    pub weights: Vec<f64>,
    pub knots: Vec<f64>,
}

impl NurbsCurve {
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
}

impl NurbsCurve {
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
            .min_by(|a, b| geom::dist(self.value(*a), p).total_cmp(&geom::dist(self.value(*b), p)))
            .expect("samples");
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

/// Grid subdivisions per knot span in each direction.
const GRID_PER_SPAN: usize = 6;

fn grid_params(knots: &[f64], degree: usize) -> Vec<f64> {
    let (lo, hi) = (knots[degree], knots[knots.len() - degree - 1]);
    let mut breaks: Vec<f64> = knots
        .iter()
        .copied()
        .filter(|k| *k >= lo && *k <= hi)
        .collect();
    breaks.dedup();
    let mut out = Vec::new();
    for w in breaks.windows(2) {
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
    pub fn new(
        degree_u: usize,
        degree_v: usize,
        control_points: Vec<Vec<V3>>,
        weights: Vec<Vec<f64>>,
        knots_u: Vec<f64>,
        knots_v: Vec<f64>,
    ) -> Self {
        NurbsSurface {
            degree_u,
            degree_v,
            control_points,
            weights,
            knots_u,
            knots_v,
            grid: std::sync::OnceLock::new(),
        }
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

    /// Central-difference partial derivatives, one-sided at the domain edges.
    fn partials(&self, u: f64, v: f64) -> (V3, V3) {
        let (u0, u1, v0, v1) = self.domain();
        let hu = (u1 - u0) * 1e-6;
        let hv = (v1 - v0) * 1e-6;
        let (ua, ub) = ((u - hu).max(u0), (u + hu).min(u1));
        let (va, vb) = ((v - hv).max(v0), (v + hv).min(v1));
        let su = geom::scale(
            geom::sub(self.value(ub, v), self.value(ua, v)),
            1.0 / (ub - ua),
        );
        let sv = geom::scale(
            geom::sub(self.value(u, vb), self.value(u, va)),
            1.0 / (vb - va),
        );
        (su, sv)
    }

    /// The parameters of the surface point closest to *p*, starting from *hint* when given.
    pub fn invert(&self, p: V3, hint: Option<(f64, f64)>) -> (f64, f64) {
        let start = hint.unwrap_or_else(|| self.nearest_grid(p));
        let (mut u, mut v) = start;
        let (u0, u1, v0, v1) = self.domain();
        for _ in 0..50 {
            let s = self.value(u, v);
            let r = geom::sub(p, s);
            let (su, sv) = self.partials(u, v);
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
            if moved < 1e-14 * (1.0 + u.abs() + v.abs()) {
                break;
            }
        }
        // A hinted start can converge to a far local minimum; fall back to the global seed.
        if hint.is_some() && geom::dist(self.value(u, v), p) > 1e-6 {
            let fresh = self.invert(p, None);
            if geom::dist(self.value(fresh.0, fresh.1), p) < geom::dist(self.value(u, v), p) {
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
            let mut det_last = 0.0;
            for _ in 0..40 {
                let s = self.value(u, v);
                let f = geom::sub(s, geom::add(origin, geom::scale(dir, t)));
                if geom::norm(f) < 1e-10 {
                    converged = true;
                    break;
                }
                let (su, sv) = self.partials(u, v);
                // Solve [su sv -dir] (du dv dt) = -f by Cramer's rule.
                let m = [su, sv, geom::scale(dir, -1.0)];
                let det = geom::dot(m[0], geom::cross(m[1], m[2]));
                det_last = det;
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
            let (su, sv) = self.partials(u, v);
            let normal = geom::cross(su, sv);
            let cos = geom::dot(normal, dir).abs() / (geom::norm(normal).max(1e-300));
            if cos < 1e-6 || det_last.abs() < 1e-14 {
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
