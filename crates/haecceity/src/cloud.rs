//! Faces as point clouds, and how far apart two clouds are — for comparing surface groups
//! whose STEP face decomposition differs (Python compares OpenCascade meshes).
//!
//! The points lie on the exact geometry, as densely as a mesh to a linear deflection and 0.1 rad
//! angular deflection (build123d's `tessellate`) needs: each edge's samples thinned to that sag
//! and turn, and on curved faces a parameter grid refined until it meets them too. Sampling
//! depends only on a face's own geometry and parameters, so congruent copies sample congruently.

use super::brep::Part;
use super::geom::{self, Surface, V3};
use super::sampling::edge_interval;

/// The angle between neighbouring samples' directions, as `tessellate`'s angular tolerance.
const ANGULAR_DEFLECTION: f64 = 0.1;
const MAX_GRID: usize = 256;

impl Part {
    /// Points on the face: its edges, and a grid over its interior unless it is a plane (which
    /// its boundary fixes, and which OpenCascade meshes from its boundary alone).
    pub fn face_cloud(&self, face: usize, deflection: f64) -> Vec<V3> {
        let mut out = Vec::new();
        for e in self.face_edges(face) {
            thin(&self.edges[e].samples, deflection, &mut out);
        }
        let surface = &self.faces[face].surface;
        if matches!(surface, Surface::Plane { .. }) {
            return out;
        }
        let (Some((u0, u1, v0, v1)), Some(domain)) = (self.uv_bounds(face), self.domain(face))
        else {
            return out;
        };
        let at = |i: usize, nu: usize, j: usize, nv: usize| {
            (
                u0 + (u1 - u0) * i as f64 / nu as f64,
                v0 + (v1 - v0) * j as f64 / nv as f64,
            )
        };
        // Doubled until each grid step, along three lines each way, meets the deflections.
        let fine = |nu: usize, nv: usize, along_u: bool| {
            let (n, lines) = if along_u { (nu, nv) } else { (nv, nu) };
            [0, lines / 2, lines].into_iter().all(|line| {
                let point = |k: usize| {
                    let (u, v) = if along_u {
                        at(k, 2 * nu, 2 * line, 2 * nv)
                    } else {
                        at(2 * line, 2 * nu, k, 2 * nv)
                    };
                    surface.value(u, v)
                };
                (0..n).all(|k| within(point(2 * k), point(2 * k + 1), point(2 * k + 2), deflection))
            })
        };
        let (mut nu, mut nv) = (1, 1);
        while nu < MAX_GRID && !fine(nu, nv, true) {
            nu *= 2;
        }
        while nv < MAX_GRID && !fine(nu, nv, false) {
            nv *= 2;
        }
        for i in 0..=nu {
            for j in 0..=nv {
                let (u, v) = at(i, nu, j, nv);
                if domain.contains(u, v) {
                    out.push(surface.value(u, v));
                }
            }
        }
        out
    }
}

/// Exact distances to one face: from the nearest of its edge samples (each with its edge and
/// curve parameter) and grid samples (each with its surface parameters), refined onto the exact
/// curve and surface by Newton steps from there.
pub struct FaceProbe<'a> {
    part: &'a Part,
    face: usize,
    edge_tree: KdTree,
    edge_at: Vec<(usize, f64)>,
    grid_tree: KdTree,
    grid_at: Vec<(f64, f64)>,
}

impl<'a> FaceProbe<'a> {
    pub fn new(part: &'a Part, face: usize) -> Self {
        const PER_EDGE: usize = 64;
        let (mut points, mut edge_at) = (Vec::new(), Vec::new());
        for e in part.face_edges(face) {
            let ed = &part.edges[e];
            let (t0, t1) =
                edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
            for k in 0..=PER_EDGE {
                let t = t0 + (t1 - t0) * k as f64 / PER_EDGE as f64;
                points.push(ed.curve.value(t));
                edge_at.push((e, t));
            }
        }
        let edge_tree = KdTree::new(&points);
        let (mut points, mut grid_at) = (Vec::new(), Vec::new());
        let f = &part.faces[face];
        if let (Some((u0, u1, v0, v1)), Some(domain)) = (part.uv_bounds(face), part.domain(face)) {
            let n = 48;
            for i in 0..=n {
                for j in 0..=n {
                    let u = u0 + (u1 - u0) * i as f64 / n as f64;
                    let v = v0 + (v1 - v0) * j as f64 / n as f64;
                    if domain.contains(u, v) {
                        points.push(f.surface.value(u, v));
                        grid_at.push((u, v));
                    }
                }
            }
        }
        FaceProbe {
            part,
            face,
            edge_tree,
            edge_at,
            grid_tree: KdTree::new(&points),
            grid_at,
        }
    }

    /// The exact distance from *q* to the face: to the foot of *q* on its surface when that
    /// lies on the face, else to the nearest point of its edges.
    pub fn distance(&self, q: V3) -> f64 {
        let surface = &self.part.faces[self.face].surface;
        let hint = (!self.grid_at.is_empty()).then(|| self.grid_at[self.grid_tree.nearest(q).0]);
        let on_surface = surface.parameters(q, hint).and_then(|(u, v)| {
            self.part
                .domain(self.face)?
                .contains(u, v)
                .then(|| geom::dist(q, surface.value(u, v)))
        });
        let to_edge = if self.edge_at.is_empty() {
            f64::INFINITY
        } else {
            let (e, t) = self.edge_at[self.edge_tree.nearest(q).0];
            self.part.edge_distance(e, q, t)
        };
        on_surface.map_or(to_edge, |d| d.min(to_edge))
    }
}

impl Part {
    /// The exact distance from *q* to the edge (its curve between its ends), by Newton steps
    /// from parameter *t*.
    pub fn edge_distance(&self, edge: usize, q: V3, mut t: f64) -> f64 {
        let ed = &self.edges[edge];
        let (t0, t1) = edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
        let (lo, hi) = (t0.min(t1), t0.max(t1));
        for _ in 0..20 {
            let d = ed.curve.derivative(t);
            let dd = geom::dot(d, d);
            if dd < 1e-300 {
                break;
            }
            let next = (t + geom::dot(geom::sub(q, ed.curve.value(t)), d) / dd).clamp(lo, hi);
            let settled = (next - t).abs() <= 1e-14 * (1.0 + t.abs());
            t = next;
            if settled {
                break;
            }
        }
        geom::dist(q, ed.curve.value(t))
    }
}

/// The angle between the directions a→m and m→b (zero if either is degenerate).
fn turn(a: V3, m: V3, b: V3) -> f64 {
    match (geom::unit(geom::sub(m, a)), geom::unit(geom::sub(b, m))) {
        (Some(x), Some(y)) => geom::dot(x, y).clamp(-1.0, 1.0).acos(),
        _ => 0.0,
    }
}

/// Whether the chord a→b stands in for the arc through m: its midpoint within the deflection,
/// and the arc turning no more than the angular deflection.
fn within(a: V3, m: V3, b: V3, deflection: f64) -> bool {
    let sag = geom::dist(m, geom::scale(geom::add(a, b), 0.5));
    sag <= deflection && turn(a, m, b) <= ANGULAR_DEFLECTION
}

/// The edge's samples thinned (Douglas–Peucker) to those the deflections need, appended.
pub fn thin(samples: &[V3], deflection: f64, out: &mut Vec<V3>) {
    fn split(s: &[V3], deflection: f64, out: &mut Vec<V3>) {
        if s.len() <= 2 {
            return;
        }
        let (a, b) = (s[0], s[s.len() - 1]);
        let chord = geom::sub(b, a);
        let off = |p: V3| {
            let t = geom::dot(geom::sub(p, a), chord) / geom::dot(chord, chord).max(1e-300);
            geom::dist(p, geom::add(a, geom::scale(chord, t.clamp(0.0, 1.0))))
        };
        let (k, far) = (1..s.len() - 1)
            .map(|k| (k, off(s[k])))
            .fold((0, -1.0), |best, x| if x.1 > best.1 { x } else { best });
        // The turn from the run's first direction to its last.
        let turned = match (
            geom::unit(geom::sub(s[1], a)),
            geom::unit(geom::sub(b, s[s.len() - 2])),
        ) {
            (Some(x), Some(y)) => geom::dot(x, y).clamp(-1.0, 1.0).acos(),
            _ => 0.0,
        };
        if far > deflection || turned > ANGULAR_DEFLECTION {
            split(&s[..=k], deflection, out);
            out.push(s[k]);
            split(&s[k..], deflection, out);
        }
    }
    let Some(&first) = samples.first() else {
        return;
    };
    out.push(first);
    split(samples, deflection, out);
    if samples.len() > 1 {
        out.push(samples[samples.len() - 1]);
    }
}

/// A k-d tree over points, for nearest-neighbour distances: each subrange is split at its
/// median along its widest axis, the median stored in the middle with that axis.
pub struct KdTree {
    points: Vec<(V3, usize)>,
    axes: Vec<u8>,
}

impl KdTree {
    pub fn new(points: &[V3]) -> Self {
        fn build(p: &mut [(V3, usize)], axes: &mut [u8]) {
            if p.len() <= 1 {
                return;
            }
            let axis = widest(p);
            let mid = p.len() / 2;
            p.select_nth_unstable_by(mid, |a, b| a.0[axis].total_cmp(&b.0[axis]));
            axes[mid] = axis as u8;
            let (left, right) = p.split_at_mut(mid);
            let (left_axes, right_axes) = axes.split_at_mut(mid);
            build(left, left_axes);
            build(&mut right[1..], &mut right_axes[1..]);
        }
        let mut points: Vec<(V3, usize)> = points.iter().copied().zip(0..).collect();
        let mut axes = vec![0; points.len()];
        build(&mut points, &mut axes);
        KdTree { points, axes }
    }

    /// The index of the point nearest *q*, and its distance (infinite when there are none).
    pub fn nearest(&self, q: V3) -> (usize, f64) {
        fn search(p: &[(V3, usize)], axes: &[u8], q: V3, best: &mut (usize, f64)) {
            if p.is_empty() {
                return;
            }
            let mid = p.len() / 2;
            let d = geom::dist(p[mid].0, q);
            if d < best.1 {
                *best = (p[mid].1, d);
            }
            if p.len() == 1 {
                return;
            }
            let axis = axes[mid] as usize;
            let off = q[axis] - p[mid].0[axis];
            let (left, right) = ((&p[..mid], &axes[..mid]), (&p[mid + 1..], &axes[mid + 1..]));
            let (near, far) = if off < 0.0 {
                (left, right)
            } else {
                (right, left)
            };
            search(near.0, near.1, q, best);
            if off.abs() < best.1 {
                search(far.0, far.1, q, best);
            }
        }
        let mut best = (0, f64::INFINITY);
        search(&self.points, &self.axes, q, &mut best);
        best
    }
}

/// The axis along which the points spread furthest.
fn widest(p: &[(V3, usize)]) -> usize {
    let mut b = geom::Bounds::empty();
    for &(x, _) in p {
        b.add(x);
    }
    (0..3)
        .max_by(|&i, &j| (b.max[i] - b.min[i]).total_cmp(&(b.max[j] - b.min[j])))
        .unwrap()
}

/// `_distance`: the larger of the two directed 99th-percentile nearest-neighbour distances
/// (numpy's linear quantile); `None` if either cloud is empty.
pub fn fit_distance(a: &[V3], b: &[V3]) -> Option<f64> {
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let directed = |from: &[V3], to: &[V3]| {
        let tree = KdTree::new(to);
        quantile99(from.iter().map(|&q| tree.nearest(q).1).collect())
    };
    Some(directed(a, b).max(directed(b, a)))
}

/// numpy's linear 99th percentile of a non-empty sample.
pub fn quantile99(mut d: Vec<f64>) -> f64 {
    d.sort_by(f64::total_cmp);
    let h = (d.len() - 1) as f64 * 0.99;
    let (k, f) = (h.floor() as usize, h - h.floor());
    match d.get(k + 1) {
        Some(next) => d[k] + f * (next - d[k]),
        None => d[k],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_matches_brute_force() {
        // A deterministic scatter.
        let mut x = 0.5f64;
        let mut next = || {
            x = (x * 3.9).fract() * 0.999 + 0.0005;
            x
        };
        let points: Vec<V3> = (0..500).map(|_| [next(), next(), next()]).collect();
        let tree = KdTree::new(&points);
        for _ in 0..200 {
            let q = [next(), next(), next()];
            let brute = (0..points.len())
                .map(|i| (i, geom::dist(points[i], q)))
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap();
            assert_eq!(tree.nearest(q), brute);
        }
        assert_eq!(fit_distance(&points, &points), Some(0.0));
    }
}
