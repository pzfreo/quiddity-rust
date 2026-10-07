//! Areas and volumes (`BRepGProp`): surface integrals over each face's trimmed parameter
//! domain, reduced by Green's theorem to integrals along the exact edge curves and evaluated by
//! Gauss–Legendre quadrature, so they agree with OpenCascade's to near machine precision (the
//! body signatures families publish round them to twelve significant figures).
//!
//! ∬_D f du dv = −∮ H du with H(u, v) = ∫ f(u, s) ds from a fixed v, or ∮ G dv with
//! G(u, v) = ∫ f(s, v) ds from a fixed u. The first ignores boundary pieces of constant u
//! (seams, which files may omit) and serves faces whose loops run round u; the second ignores
//! pieces of constant v (a sphere's poles, a cone's apex) and serves every other face. Where a
//! loop runs round u and ends at a singular line, H starts from that line, so the line itself
//! contributes nothing. Each loop's direction is read from the geometry (which side of it the
//! face lies on), not from the file's orientation flags.

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
/// consecutive pair, for both components of f at once.
fn integrate(cuts: &[f64], f: impl Fn(f64) -> [f64; 2]) -> [f64; 2] {
    let mut total = [0.0; 2];
    for w in cuts.windows(2) {
        let (half, centre) = (0.5 * (w[1] - w[0]), 0.5 * (w[1] + w[0]));
        for &(x, weight) in gauss_nodes() {
            let y = f(centre + half * x);
            total[0] += weight * half * y[0];
            total[1] += weight * half * y[1];
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

/// A quadrature node on a face's boundary: its parameters and weighted parameter displacement.
struct Node {
    u: f64,
    v: f64,
    du: f64,
    dv: f64,
}

/// The parameter-space velocity (du/dt, dv/dt) of a curve on the surface: the least-squares
/// solution of [Su Sv]·(du, dv) = C'(t).
fn uv_velocity(su: V3, sv: V3, c: V3) -> (f64, f64) {
    let (a, b, d) = (geom::dot(su, su), geom::dot(su, sv), geom::dot(sv, sv));
    let (p, q) = (geom::dot(su, c), geom::dot(sv, c));
    let det = a * d - b * b;
    // At a degenerate point (a pole, a collapsed B-spline edge) the parameters are not
    // determined; such points contribute nothing to the boundary integrals.
    if det.abs() <= 1e-12 * a.max(d) * a.max(d) {
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
            let (p, su, sv) = match surface {
                Surface::Freeform { surface, .. } => surface.value_and_partials(u, v),
                _ => {
                    let (su, sv) = surface.partials(u, v);
                    (surface.value(u, v), su, sv)
                }
            };
            let n = geom::cross(su, sv);
            [geom::norm(n), geom::dot(p, n)]
        })
    }

    /// ∬ f du dv over the face's trimmed parameter domain.
    fn face_integral(&self, face: usize, f: impl Fn(f64, f64) -> [f64; 2]) -> Option<[f64; 2]> {
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
        let along_v = if pu {
            uv.iter().any(|lp| lp.winds_u)
        } else {
            uv.iter()
                .any(|lp| open(lp, |p| p.0, half(domain.u_range())))
        };
        let far_v = if pv { 1.0 } else { half(domain.v_range()) };
        if along_v && uv.iter().any(|lp| open(lp, |p| p.1, far_v)) {
            return None;
        }
        let first = (0..fc.loops.len()).find(|&i| fc.loops[i].vertex.is_none())?;
        let start = uv[first].points.first().copied()?;
        let start = (start.0 + shifts[first].0, start.1 + shifts[first].1);
        let reference = if along_v {
            let (lo, hi) = domain.v_range();
            [lo, hi]
                .into_iter()
                .find_map(|v| surface.singular_v(v).filter(|s| (s - v).abs() < 1e-6))
                .unwrap_or(start.1)
        } else {
            start.0
        };
        let mut total = [0.0; 2];
        for i in 0..fc.loops.len() {
            if fc.loops[i].vertex.is_some() {
                continue;
            }
            let mut sum = [0.0; 2];
            for node in self.boundary_nodes(face, i, shifts[i])? {
                let (weight, [a, b]) = if along_v {
                    let cuts = surface_cuts(surface, true, reference, node.v);
                    (-node.du, integrate(&cuts, |s| f(node.u, s)))
                } else {
                    let cuts = surface_cuts(surface, false, reference, node.u);
                    (node.dv, integrate(&cuts, |s| f(s, node.v)))
                };
                sum = [sum[0] + weight * a, sum[1] + weight * b];
            }
            // A loop that winds round either parameter has no signed area to read.
            let direction = self.loop_direction(face, i, along_v || uv[i].winds_v)?;
            total = [total[0] + direction * sum[0], total[1] + direction * sum[1]];
        }
        Some(total)
    }

    /// Quadrature nodes round loop *i* of the face in parameter space (placed by `shift`): along
    /// each edge's exact curve, and straight across any gap between one edge's end and the
    /// next one's start (a collapsed B-spline side, a pole), less whole turns.
    fn boundary_nodes(&self, face: usize, i: usize, shift: (f64, f64)) -> Option<Vec<Node>> {
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
        let mut nodes = Vec::new();
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
            let mut next_anchor = 1;
            let (mut regular_seen, mut in_band, mut crossed) = (false, false, false);
            let ed = &self.edges[e];
            let (mut t0, mut t1) =
                edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
            if !forward {
                std::mem::swap(&mut t0, &mut t1);
            }
            let (mut first_node, mut last_node) = (None, None);
            // Nodes are visited in walking order, so periodic parameters unwrap node by node.
            for w in curve_cuts(surface, &ed.curve, t0, t1).windows(2) {
                let (half, centre) = (0.5 * (w[1] - w[0]), 0.5 * (w[1] + w[0]));
                for &(x, weight) in gauss_nodes() {
                    let t = centre + half * x;
                    let (u, v) = surface.parameters(ed.curve.value(t), Some(last))?;
                    if surface.singular_v(v).is_some() {
                        // Through the singular point u jumps (once per pass near it, as the
                        // loop's samples are split).
                        in_band = regular_seen;
                        let jump = (geom::nearest_turn(u, last.0) - last.0).abs();
                        if in_band && !crossed && jump > std::f64::consts::FRAC_PI_2 {
                            if let Some(p) = anchors.get(next_anchor) {
                                last = placed(p);
                                next_anchor += 1;
                            }
                            crossed = true;
                        }
                    } else {
                        if in_band
                            && !crossed
                            && let Some(p) = anchors.get(next_anchor)
                        {
                            last = placed(p);
                            next_anchor += 1;
                        }
                        (regular_seen, in_band, crossed) = (true, false, false);
                    }
                    let (u, v) = unwrap((u, v), last);
                    last = (u, v);
                    let (su, sv) = surface.partials(u, v);
                    let (du, dv) = uv_velocity(su, sv, ed.curve.derivative(t));
                    first_node.get_or_insert((t, last, (du, dv)));
                    last_node = Some((t, last, (du, dv)));
                    let w = weight * half;
                    nodes.push(Node {
                        u,
                        v,
                        du: w * du,
                        dv: w * dv,
                    });
                }
            }
            // The edge's exact ends, inverted from the nodes beside them; next to a collapsed
            // B-spline side an inversion can jump across it, so one far from where the node's
            // own motion leads is replaced by that extrapolation.
            let (Some((t_first, p_first, v_first)), Some((t_last, p_last, v_last))) =
                (first_node, last_node)
            else {
                return None;
            };
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
            let (gu, gv) = (to.0 - from.0, to.1 - from.1);
            if gu.hypot(gv) < 1e-12 {
                continue;
            }
            for w in cuts(0.0, 1.0, (gu.hypot(gv).ceil() as usize).max(1), &[]).windows(2) {
                let (half, centre) = (0.5 * (w[1] - w[0]), 0.5 * (w[1] + w[0]));
                for &(x, weight) in gauss_nodes() {
                    let s = centre + half * x;
                    let w = weight * half;
                    nodes.push(Node {
                        u: from.0 + gu * s,
                        v: from.1 + gv * s,
                        du: w * gu,
                        dv: w * gv,
                    });
                }
            }
        }
        Some(nodes)
    }

    /// +1 when the face lies to the left of loop *i* walked in its listed direction (so the
    /// walk runs anticlockwise round the face's parameter domain), −1 when to the right.
    ///
    /// A loop closed in parameter space has the face on its left when it runs anticlockwise
    /// round the outer boundary or clockwise round a hole. A loop running round a periodic
    /// direction encloses nothing, so step either side of its longest edge used once by the face.
    fn loop_direction(&self, face: usize, i: usize, winding: bool) -> Option<f64> {
        if !winding {
            let (outer, _) = self.loop_placement(face)?;
            let pts = &self.uv_loops(face)?[i].points;
            let area: f64 = pts
                .iter()
                .zip(pts.iter().cycle().skip(1))
                .map(|(a, b)| a.0 * b.1 - b.0 * a.1)
                .sum();
            let anticlockwise = if area > 0.0 { 1.0 } else { -1.0 };
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
            let (su, sv) = surface.partials(u, v);
            let (mut du, mut dv) = uv_velocity(su, sv, ed.curve.derivative(t));
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
        let at = |u: f64, v: f64| match surface {
            Surface::Freeform { surface, .. } => {
                let (p, su, sv) = surface.value_and_partials(u, v);
                (p, geom::cross(su, sv))
            }
            _ => {
                let (su, sv) = surface.partials(u, v);
                (surface.value(u, v), geom::cross(su, sv))
            }
        };
        let [area, mx] = self.face_integral(face, |u, v| {
            let (p, n) = at(u, v);
            let w = geom::norm(n);
            [w, p[0] * w]
        })?;
        let [my, mz] = self.face_integral(face, |u, v| {
            let (p, n) = at(u, v);
            let w = geom::norm(n);
            [p[1] * w, p[2] * w]
        })?;
        let [nx, ny] = self.face_integral(face, |u, v| {
            let n = at(u, v).1;
            [n[0], n[1]]
        })?;
        let [nz, _] = self.face_integral(face, |u, v| [at(u, v).1[2], 0.0])?;
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
}
