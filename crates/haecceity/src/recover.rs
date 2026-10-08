//! Analytic surfaces recovered from freeform faces — what the Python implementation asks
//! `ShapeAnalysis_CanonicalRecognition` for (`_effective_surfaces`): a B-spline face that is a
//! plane, cylinder, cone or sphere within `1e-6 × nominal + COORD_FLOOR` of it.
//!
//! Each primitive is fitted to points of the trimmed face itself (a grid over its parameter
//! domain and its edges' samples), using the exact normals there to find axes: a cylinder's
//! normals are all perpendicular to its axis and a cone's all at one angle to it. The fit's
//! certificate is the largest distance of those points from it. As in Python, a face is recovered
//! only when exactly one kind fits.

// Small dense matrix code reads best by index.
#![allow(clippy::needless_range_loop)]

use super::brep::{Part, holds};
use super::geom::{self, COORD_FLOOR, Frame, Surface, V3};

const RECOVERY_REL: f64 = 1e-6;
/// A fit's gap is certified with the rounding of its own evaluation: this many ulps of the
/// largest distance it measures from the fit's centre, apex or axis point. A near-flat face fits
/// a cylinder or sphere of radius ~1e19, whose measured gap is round-off (0.0 in one placement).
const ROUNDING: f64 = 16.0 * f64::EPSILON;
/// Grid lines across the face's parameter range, each way.
const GRID: usize = 16;

impl Part {
    /// The analytic surface a freeform face is, if exactly one kind fits it within the
    /// recovery tolerance (computed once per face).
    pub fn recovered(&self, face: usize) -> Option<&Surface> {
        self.cache[face]
            .recovered
            .get_or_init(|| recover(self, face))
            .as_ref()
    }

    /// The face's outward normal at a point of it: from its own surface and orientation, which a
    /// recovered surface (fitted, so unoriented) does not carry.
    pub fn outward_at(&self, face: usize, p: V3) -> Option<V3> {
        let (u, v) = self.faces[face].surface.parameters(p, None)?;
        self.face_normal(face, u, v)
    }
}

fn recover(part: &Part, face: usize) -> Option<Surface> {
    let Surface::Freeform { surface, .. } = &part.faces[face].surface else {
        return None;
    };
    let tolerance = tolerance(part, face)?;
    let (u0, u1, v0, v1) = part.uv_bounds(face)?;
    let domain = part.domain(face)?;
    let mut points: Vec<V3> = part
        .face_edges(face)
        .into_iter()
        .flat_map(|e| part.edges[e].samples.iter().copied())
        .collect();
    // Interior points with their normals: those the face holds a neighbourhood of, as the grid's
    // outer lines run along the face's boundary, where whether a point is inside is round-off.
    let mut grid: Vec<(V3, V3)> = Vec::new();
    let h = (1e-6 * (u1 - u0), 1e-6 * (v1 - v0));
    for i in 0..=GRID {
        for j in 0..=GRID {
            let u = u0 + (u1 - u0) * i as f64 / GRID as f64;
            let v = v0 + (v1 - v0) * j as f64 / GRID as f64;
            if !holds(domain, u, v, h) {
                continue;
            }
            let (p, du, dv) = surface.value_and_partials(u, v);
            points.push(p);
            if let Some(n) = geom::unit(geom::cross(du, dv)) {
                grid.push((p, n));
            }
        }
    }
    if grid.len() < 6 {
        return None;
    }
    let fits: Vec<Surface> = [
        fit_plane(&points),
        fit_cylinder(&points, &grid),
        fit_cone(&points, &grid),
        fit_sphere(&points, &grid),
    ]
    .into_iter()
    .flatten()
    .filter(|(_, gap)| *gap <= tolerance)
    .map(|(s, _)| s)
    .collect();
    (fits.len() == 1).then(|| fits.into_iter().next().unwrap())
}

/// `recovery_tolerance`: how far a recovered surface may stand from the face.
pub fn tolerance(part: &Part, face: usize) -> Option<f64> {
    Some(RECOVERY_REL * nominal(part, face)? + COORD_FLOOR)
}

/// `recovery_nominal`: the face's controlling length, min(√area, 2·area / perimeter), the
/// perimeter excluding seams.
fn nominal(part: &Part, face: usize) -> Option<f64> {
    let area = part.face_mass(face)?[0];
    if !(area.is_finite() && area > 0.0) {
        return None;
    }
    let mut uses = std::collections::BTreeMap::<usize, usize>::new();
    for lp in &part.faces[face].loops {
        for &(e, _) in &lp.edges {
            *uses.entry(e).or_default() += 1;
        }
    }
    let perimeter: f64 = uses
        .iter()
        .filter(|&(_, &n)| n == 1)
        .map(|(&e, _)| {
            let s = &part.edges[e].samples;
            s.windows(2).map(|w| geom::dist(w[0], w[1])).sum::<f64>()
        })
        .sum();
    let scale = area.sqrt();
    Some(if perimeter > 0.0 {
        scale.min(2.0 * area / perimeter)
    } else {
        scale
    })
}

fn fit_plane(points: &[V3]) -> Option<(Surface, f64)> {
    let c = centroid(points);
    let (_, vectors) = eigen(scatter(points.iter().map(|p| geom::sub(*p, c))));
    let n = vectors[0];
    let gap = points
        .iter()
        .map(|p| geom::dot(geom::sub(*p, c), n).abs())
        .fold(0.0, worst);
    Some((Surface::Plane { frame: frame(c, n) }, gap))
}

/// The axis is the direction every normal is perpendicular to; the section a circle.
fn fit_cylinder(points: &[V3], grid: &[(V3, V3)]) -> Option<(Surface, f64)> {
    let (values, vectors) = eigen(scatter(grid.iter().map(|g| g.1)));
    // Normals all alike are a plane's, not a cylinder's.
    if values[1] <= 1e-9 * values[2] {
        return None;
    }
    let axis = vectors[0];
    // Fitted about the points' centroid, so the arithmetic does not follow the placement.
    let c = centroid(points);
    let f = frame(c, axis);
    let flat: Vec<(f64, f64)> = points
        .iter()
        .map(|p| {
            let q = geom::sub(*p, c);
            (geom::dot(q, f.x), geom::dot(q, f.y))
        })
        .collect();
    let (cx, cy, r) = fit_circle(&flat)?;
    let origin = geom::add(c, geom::add(geom::scale(f.x, cx), geom::scale(f.y, cy)));
    let gap = certified(points, origin, |p| (radial(p, origin, axis) - r).abs());
    Some((
        Surface::Cylinder {
            frame: frame(origin, axis),
            radius: r,
        },
        gap,
    ))
}

/// The axis is the direction every normal makes one angle with; the apex the point every
/// normal line's plane passes through.
fn fit_cone(points: &[V3], grid: &[(V3, V3)]) -> Option<(Surface, f64)> {
    let normals: Vec<V3> = grid.iter().map(|g| g.1).collect();
    let mean = centroid(&normals);
    let (values, vectors) = eigen(scatter(normals.iter().map(|n| geom::sub(*n, mean))));
    // Normals that vary by less than ~1e-6 rad are a plane's: its apex is round-off.
    if values[1] <= 1e-9 * values[2].max(f64::MIN_POSITIVE)
        || values[2] <= 1e-12 * normals.len() as f64
    {
        return None;
    }
    let mut axis = vectors[0];
    // The apex: every point's tangent plane passes through it. Solved about the points'
    // centroid, so the arithmetic does not follow the placement.
    let c = centroid(points);
    let m = scatter(normals.iter().copied());
    let rhs = grid.iter().fold([0.0; 3], |acc, (p, n)| {
        geom::add(acc, geom::scale(*n, geom::dot(*n, geom::sub(*p, c))))
    });
    let apex = geom::add(c, solve3(m, rhs)?);
    if geom::dot(geom::sub(c, apex), axis) < 0.0 {
        axis = geom::scale(axis, -1.0);
    }
    let angles: Vec<f64> = points
        .iter()
        .filter_map(|p| {
            let q = geom::sub(*p, apex);
            let len = geom::norm(q);
            (len > 1e-9).then(|| (geom::dot(q, axis) / len).clamp(-1.0, 1.0).acos())
        })
        .collect();
    let semi = angles.iter().sum::<f64>() / angles.len().max(1) as f64;
    if !(1e-4..std::f64::consts::FRAC_PI_2 - 1e-4).contains(&semi) {
        return None;
    }
    let gap = certified(points, apex, |p| {
        let q = geom::sub(p, apex);
        let t = geom::dot(q, axis);
        let rho = geom::norm(geom::sub(q, geom::scale(axis, t)));
        (rho * semi.cos() - t * semi.sin()).abs()
    });
    // Placed at the points' mean height, where its radius is t·tan(α).
    let t = geom::dot(geom::sub(c, apex), axis);
    let origin = geom::add(apex, geom::scale(axis, t));
    let surface = Surface::Cone {
        frame: frame(origin, axis),
        radius: t * semi.tan(),
        semi_angle: semi,
    };
    Some((surface, gap))
}

/// |p|² = 2c·p + k, solved in the least-squares sense.
fn fit_sphere(points: &[V3], grid: &[(V3, V3)]) -> Option<(Surface, f64)> {
    // Normals alike in any direction are a plane's or a cylinder's: points on a plane fit a
    // sphere of near-infinite radius to rounding.
    let (values, _) = eigen(scatter(grid.iter().map(|g| g.1)));
    if values[0] <= 1e-9 * values[2] {
        return None;
    }
    // Fitted about the points' centroid, so the arithmetic does not follow the placement.
    let o = centroid(points);
    let mut a = [[0.0; 4]; 4];
    let mut b = [0.0; 4];
    for p in points {
        let p = geom::sub(*p, o);
        let row = [2.0 * p[0], 2.0 * p[1], 2.0 * p[2], 1.0];
        let y = geom::dot(p, p);
        for i in 0..4 {
            for j in 0..4 {
                a[i][j] += row[i] * row[j];
            }
            b[i] += row[i] * y;
        }
    }
    let x = solve4(a, b)?;
    let c = [x[0], x[1], x[2]];
    let r2 = x[3] + geom::dot(c, c);
    let c = geom::add(o, c);
    if !(r2.is_finite() && r2 > 0.0) {
        return None;
    }
    let r = r2.sqrt();
    let gap = certified(points, c, |p| (geom::dist(p, c) - r).abs());
    Some((
        Surface::Sphere {
            frame: frame(c, [0.0, 0.0, 1.0]),
            radius: r,
        },
        gap,
    ))
}

/// A circle through 2D points: algebraic fit, then geometric refinement.
fn fit_circle(points: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    let mut a = [[0.0; 3]; 3];
    let mut b = [0.0; 3];
    for &(x, y) in points {
        let row = [x, y, 1.0];
        let z = -(x * x + y * y);
        for i in 0..3 {
            for j in 0..3 {
                a[i][j] += row[i] * row[j];
            }
            b[i] += row[i] * z;
        }
    }
    let [d, e, f] = solve3(a, b)?;
    let (mut cx, mut cy) = (-d / 2.0, -e / 2.0);
    let mut r = (cx * cx + cy * cy - f).max(0.0).sqrt();
    for _ in 0..20 {
        // Gauss–Newton on the distances to the circle.
        let mut jtj = [[0.0; 3]; 3];
        let mut jtr = [0.0; 3];
        for &(x, y) in points {
            let (dx, dy) = (x - cx, y - cy);
            let rho = dx.hypot(dy).max(f64::MIN_POSITIVE);
            let res = rho - r;
            let jac = [-dx / rho, -dy / rho, -1.0];
            for i in 0..3 {
                for j in 0..3 {
                    jtj[i][j] += jac[i] * jac[j];
                }
                jtr[i] += jac[i] * res;
            }
        }
        let [sx, sy, sr] = solve3(jtj, jtr)?;
        (cx, cy, r) = (cx - sx, cy - sy, r - sr);
        if sx.abs().max(sy.abs()).max(sr.abs()) <= 1e-15 * r.abs().max(1.0) {
            break;
        }
    }
    (r.is_finite() && r > 0.0).then_some((cx, cy, r))
}

/// The largest of *gap* over the points (NaN if any is), plus the rounding of evaluating it
/// at distances from *centre* (`ROUNDING`).
fn certified(points: &[V3], centre: V3, gap: impl Fn(V3) -> f64) -> f64 {
    let measured = points.iter().map(|p| gap(*p)).fold(0.0, worst);
    let reach = points
        .iter()
        .map(|p| geom::dist(*p, centre))
        .fold(0.0, worst);
    measured + ROUNDING * reach
}

/// `f64::max` that keeps a NaN: a gap that could not be measured is no fit.
fn worst(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn radial(p: V3, origin: V3, axis: V3) -> f64 {
    let q = geom::sub(p, origin);
    geom::norm(geom::sub(q, geom::scale(axis, geom::dot(q, axis))))
}

fn centroid(points: &[V3]) -> V3 {
    let n = points.len().max(1) as f64;
    points
        .iter()
        .fold([0.0; 3], |acc, p| geom::add(acc, *p))
        .map(|c| c / n)
}

fn frame(origin: V3, z: V3) -> Frame {
    let x = geom::default_x_axis(z);
    Frame {
        origin,
        x,
        y: geom::cross(z, x),
        z,
    }
}

fn scatter(vectors: impl Iterator<Item = V3>) -> [[f64; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    for v in vectors {
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += v[i] * v[j];
            }
        }
    }
    m
}

/// Eigenvalues (ascending) and unit eigenvectors of a symmetric 3×3 matrix, by Jacobi rotations.
fn eigen(mut a: [[f64; 3]; 3]) -> ([f64; 3], [V3; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    for _ in 0..50 {
        let (mut p, mut q, mut big) = (0, 1, 0.0);
        for i in 0..3 {
            for j in i + 1..3 {
                if a[i][j].abs() > big {
                    (p, q, big) = (i, j, a[i][j].abs());
                }
            }
        }
        if big <= 1e-300 {
            break;
        }
        let theta = 0.5 * (a[q][q] - a[p][p]) / a[p][q];
        let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
        let t = if theta == 0.0 { 1.0 } else { t };
        let (c, s) = (1.0 / (t * t + 1.0).sqrt(), t / (t * t + 1.0).sqrt());
        for k in 0..3 {
            let (akp, akq) = (a[k][p], a[k][q]);
            a[k][p] = c * akp - s * akq;
            a[k][q] = s * akp + c * akq;
        }
        for k in 0..3 {
            let (apk, aqk) = (a[p][k], a[q][k]);
            a[p][k] = c * apk - s * aqk;
            a[q][k] = s * apk + c * aqk;
        }
        for row in &mut v {
            let (vp, vq) = (row[p], row[q]);
            row[p] = c * vp - s * vq;
            row[q] = s * vp + c * vq;
        }
    }
    let mut order = [0, 1, 2];
    order.sort_by(|&i, &j| a[i][i].total_cmp(&a[j][j]));
    let values = order.map(|i| a[i][i]);
    let vectors = order.map(|i| geom::unit([v[0][i], v[1][i], v[2][i]]).unwrap_or([0.0, 0.0, 1.0]));
    (values, vectors)
}

fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| geom::dot(m[0], geom::cross(m[1], m[2]));
    let d = det(a);
    if d.abs() <= 1e-300 {
        return None;
    }
    Some([0, 1, 2].map(|k| {
        let mut m = a;
        for i in 0..3 {
            m[i][k] = b[i];
        }
        det(m) / d
    }))
}

fn solve4(mut a: [[f64; 4]; 4], mut b: [f64; 4]) -> Option<[f64; 4]> {
    for col in 0..4 {
        let pivot = (col..4).max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))?;
        if a[pivot][col].abs() <= 1e-300 {
            return None;
        }
        a.swap(col, pivot);
        b.swap(col, pivot);
        for row in col + 1..4 {
            let k = a[row][col] / a[col][col];
            for c in col..4 {
                a[row][c] -= k * a[col][c];
            }
            b[row] -= k * b[col];
        }
    }
    let mut x = [0.0; 4];
    for row in (0..4).rev() {
        let s: f64 = (row + 1..4).map(|c| a[row][c] * x[c]).sum();
        x[row] = (b[row] - s) / a[row][row];
    }
    Some(x)
}
