//! Step 1 of `correspond`: the rigid motion taking the old revision onto the new one, from
//! fingerprints distinctive in both (`docs/correspondence.md`, "Align"). Candidate pairs are
//! anchors (features and faces) whose motion-free fingerprints agree; hypotheses are solved
//! from two or three pairs at a time (Horn's closed form, the quaternion version of Kabsch), and
//! the one most anchors agree with wins (RANSAC, over a deterministic sample).

// Matrix code reads clearest indexed.
#![allow(clippy::needless_range_loop)]

use crate::kernel::geom::{self, V3};

use super::fingerprint::{FaceFingerprint, FeatureFingerprint, Fingerprints};
use super::{Alignment, Thresholds, multiset_distance, same_value};

/// A proper rigid motion: `p ↦ R p + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub r: [[f64; 3]; 3],
    pub t: V3,
}

impl Motion {
    pub const IDENTITY: Motion = Motion {
        r: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        t: [0.0; 3],
    };

    pub fn point(&self, p: V3) -> V3 {
        geom::add(self.dir(p), self.t)
    }

    pub fn dir(&self, d: V3) -> V3 {
        [0, 1, 2].map(|i| geom::dot(self.r[i], d))
    }
}

/// An anchor: a point, and a direction where it has one (signed for a face's normal, a line for
/// a feature's axis).
#[derive(Clone, Copy, Debug)]
struct Anchor {
    point: V3,
    dir: Option<V3>,
    line: bool,
}

/// A candidate pair: indices into the old and new anchor lists.
type Pair = (usize, usize);

fn feature_anchor(f: &FeatureFingerprint) -> Option<Anchor> {
    Some(Anchor {
        point: f.position?,
        dir: f.axis,
        line: true,
    })
}

fn face_anchor(f: &FaceFingerprint) -> Anchor {
    Anchor {
        point: f.centroid,
        dir: f.normal,
        line: false,
    }
}

/// Whether two features could be the same feature unmoved and unchanged: everything but their
/// placement agrees.
fn same_feature(a: &FeatureFingerprint, b: &FeatureFingerprint, rel: f64) -> bool {
    a.family == b.family
        && a.traits == b.traits
        && a.sizes.len() == b.sizes.len()
        && a.sizes
            .iter()
            .all(|(k, x)| b.sizes.get(k).is_some_and(|y| same_value(*x, *y, rel)))
        && a.neighbourhood == b.neighbourhood
}

fn same_face(a: &FaceFingerprint, b: &FaceFingerprint, rel: f64) -> bool {
    a.surface == b.surface
        && a.parameters.len() == b.parameters.len()
        && a.parameters
            .iter()
            .all(|(k, x)| b.parameters.get(k).is_some_and(|y| same_value(*x, *y, rel)))
        && match (a.area, b.area) {
            (Some(x), Some(y)) => same_value(x, y, rel),
            _ => false,
        }
        && multiset_distance(&a.adjacency, &b.adjacency) == 0.0
}

/// Candidate pairs among items: each old item paired with every new one *same* accepts, keeping
/// only items with at most `max` candidates either way (a repeat seen more often than that
/// cannot anchor anything).
fn candidates<T>(
    old: &[T],
    new: &[T],
    same: impl Fn(&T, &T) -> bool,
    max: usize,
) -> Vec<(usize, usize, usize)> {
    let mut pairs = Vec::new();
    let mut per_new = vec![0usize; new.len()];
    for (i, a) in old.iter().enumerate() {
        let mine: Vec<usize> = (0..new.len()).filter(|&j| same(a, &new[j])).collect();
        for &j in &mine {
            per_new[j] += 1;
        }
        if !mine.is_empty() && mine.len() <= max {
            for &j in &mine {
                pairs.push((i, j, mine.len()));
            }
        }
    }
    pairs.retain(|&(_, j, _)| per_new[j] <= max);
    pairs
        .into_iter()
        .map(|(i, j, n)| (i, j, n.max(per_new[j])))
        .collect()
}

/// The alignment of *old* onto *new*.
pub fn align(old: &Fingerprints, new: &Fingerprints, th: &Thresholds) -> (Motion, Alignment) {
    let scale = old.scale.max(new.scale);
    let tol = th.position_tol * scale;
    let cos_tol = th.angle_tol.cos();

    // Anchors: features first, then faces.
    let mut old_anchors = Vec::new();
    let mut new_anchors = Vec::new();
    let mut pairs: Vec<(Pair, usize, f64)> = Vec::new();
    let feature_pairs = candidates(
        &old.features,
        &new.features,
        |a, b| same_feature(a, b, th.same_rel),
        th.anchor_candidates,
    );
    let mut index_old = vec![usize::MAX; old.features.len()];
    let mut index_new = vec![usize::MAX; new.features.len()];
    for (i, j, n) in feature_pairs {
        let (Some(a), Some(b)) = (
            feature_anchor(&old.features[i]),
            feature_anchor(&new.features[j]),
        ) else {
            continue;
        };
        if index_old[i] == usize::MAX {
            index_old[i] = old_anchors.len();
            old_anchors.push(a);
        }
        if index_new[j] == usize::MAX {
            index_new[j] = new_anchors.len();
            new_anchors.push(b);
        }
        pairs.push(((index_old[i], index_new[j]), n, f64::INFINITY));
    }
    let face_pairs = candidates(
        &old.faces,
        &new.faces,
        |a, b| same_face(a, b, th.same_rel),
        th.anchor_candidates,
    );
    let mut index_old = vec![usize::MAX; old.faces.len()];
    let mut index_new = vec![usize::MAX; new.faces.len()];
    for (i, j, n) in face_pairs {
        if index_old[i] == usize::MAX {
            index_old[i] = old_anchors.len();
            old_anchors.push(face_anchor(&old.faces[i]));
        }
        if index_new[j] == usize::MAX {
            index_new[j] = new_anchors.len();
            new_anchors.push(face_anchor(&new.faces[j]));
        }
        pairs.push((
            (index_old[i], index_new[j]),
            n,
            old.faces[i].area.unwrap_or(0.0),
        ));
    }
    // The most distinctive first (fewest candidates; then features, then the larger faces), as
    // many as the budget allows.
    pairs.sort_by(|a, b| a.1.cmp(&b.1).then(b.2.total_cmp(&a.2)));
    pairs.truncate(th.anchor_pairs);
    let pairs: Vec<Pair> = pairs.into_iter().map(|p| p.0).collect();
    let anchors = {
        let mut seen: Vec<usize> = pairs.iter().map(|p| p.0).collect();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    };

    let inliers = |m: &Motion| -> (usize, f64, Vec<Pair>) {
        let mut hit_old = vec![false; old_anchors.len()];
        let mut hit_new = vec![false; new_anchors.len()];
        let (mut count, mut sq) = (0, 0.0);
        let mut kept = Vec::new();
        for &(i, j) in &pairs {
            let (a, b) = (&old_anchors[i], &new_anchors[j]);
            let d = geom::dist(m.point(a.point), b.point);
            if d > tol {
                continue;
            }
            if let (Some(da), Some(db)) = (a.dir, b.dir) {
                let c = geom::dot(m.dir(da), db);
                if (if a.line { c.abs() } else { c }) < cos_tol {
                    continue;
                }
            }
            kept.push((i, j));
            if !hit_old[i] && !hit_new[j] {
                hit_old[i] = true;
                hit_new[j] = true;
                count += 1;
                sq += d * d;
            }
        }
        (count, (sq / count.max(1) as f64).sqrt(), kept)
    };

    // Hypotheses: pairs of pairs, then triples, each all of them or a deterministic sample.
    // The best motion fixed by points spanning a plane, and the best fixed only by points on a
    // line (an axisymmetric part: its rotation about the line is not determined).
    let mut best: [Option<(Motion, usize, f64)>; 2] = [None, None];
    let mut rivals: Vec<(Motion, usize)> = Vec::new();
    let n = pairs.len();
    let mut consider = |sample: &[usize]| {
        let mut from = Vec::new();
        let mut to = Vec::new();
        for &k in sample {
            let (a, b) = (&old_anchors[pairs[k].0], &new_anchors[pairs[k].1]);
            from.push(a.point);
            to.push(b.point);
            if let (Some(da), Some(db), false) = (a.dir, b.dir, a.line) {
                from.push(geom::add(a.point, geom::scale(da, scale)));
                to.push(geom::add(b.point, geom::scale(db, scale)));
            }
        }
        let line = !spread(&from, tol);
        if line && !far_apart(&from, tol) {
            return;
        }
        let best = &mut best[usize::from(line)];
        let Some(m) = horn(&from, &to) else {
            return;
        };
        if from
            .iter()
            .zip(&to)
            .any(|(p, q)| geom::dist(m.point(*p), *q) > tol)
        {
            return;
        }
        let (count, rms, _) = inliers(&m);
        rivals.push((m, count));
        let better = match &*best {
            None => true,
            Some((bm, bc, br)) => {
                count > *bc
                    || (count == *bc
                        && (rms < br - 0.1 * tol
                            || ((rms - br).abs() <= 0.1 * tol
                                && distance_from_identity(&m, scale)
                                    < distance_from_identity(bm, scale) - 1e-9)))
            }
        };
        if better {
            *best = Some((m, count, rms));
        }
    };
    let mut rng = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = |bound: usize| {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        (rng % bound as u64) as usize
    };
    if n >= 2 {
        if n * (n - 1) / 2 <= th.hypotheses {
            for a in 0..n {
                for b in a + 1..n {
                    consider(&[a, b]);
                }
            }
        } else {
            for _ in 0..th.hypotheses {
                let (a, b) = (next(n), next(n));
                if a != b {
                    consider(&[a, b]);
                }
            }
        }
    }
    if n >= 3 {
        if n * (n - 1) * (n - 2) / 6 <= th.hypotheses {
            for a in 0..n {
                for b in a + 1..n {
                    for c in b + 1..n {
                        consider(&[a, b, c]);
                    }
                }
            }
        } else {
            for _ in 0..th.hypotheses {
                let (a, b, c) = (next(n), next(n), next(n));
                if a != b && b != c && a != c {
                    consider(&[a, b, c]);
                }
            }
        }
    }

    let unaligned = |inliers: usize, symmetric: bool| {
        (
            Motion::IDENTITY,
            Alignment {
                found: false,
                rotation: Motion::IDENTITY.r,
                translation: [0.0; 3],
                anchors,
                inliers,
                symmetric,
                axisymmetric: false,
                axis: None,
                rms: 0.0,
            },
        )
    };
    let accepted = |b: &Option<(Motion, usize, f64)>| {
        b.filter(|(_, count, _)| {
            *count >= th.min_inliers && *count as f64 >= th.min_inlier_fraction * anchors as f64
        })
    };
    let (m, count, axisymmetric) = match (accepted(&best[0]), accepted(&best[1])) {
        (Some((m, c, _)), _) => (m, c, false),
        (None, Some((m, c, _))) => (m, c, true),
        (None, None) => {
            let most = best.iter().flatten().map(|b| b.1).max().unwrap_or(0);
            return unaligned(most, false);
        }
    };
    // Refine on every inlier, then recount.
    let (_, _, kept) = inliers(&m);
    let mut from = Vec::new();
    let mut to = Vec::new();
    for (i, j) in kept {
        let (a, b) = (&old_anchors[i], &new_anchors[j]);
        from.push(a.point);
        to.push(b.point);
        if let (Some(da), Some(db), false) = (a.dir, b.dir, a.line) {
            from.push(geom::add(a.point, geom::scale(da, scale)));
            to.push(geom::add(b.point, geom::scale(db, scale)));
        }
    }
    let refined = horn(&from, &to).unwrap_or(m);
    let (count2, rms2, _) = inliers(&refined);
    let (m, count, rms) = if count2 >= count {
        (refined, count2, rms2)
    } else {
        (m, count, inliers(&m).1)
    };
    // The line every anchor lies on, in the new revision: through its two furthest points.
    let axis = if axisymmetric {
        let a = to[0];
        let far = to
            .iter()
            .copied()
            .max_by(|p, q| geom::dist(*p, a).total_cmp(&geom::dist(*q, a)))
            .unwrap_or(a);
        let b = to
            .iter()
            .copied()
            .max_by(|p, q| geom::dist(*p, far).total_cmp(&geom::dist(*q, far)))
            .unwrap_or(a);
        geom::unit(geom::sub(b, far)).map(|d| [far, d])
    } else {
        None
    };
    // Symmetric: another motion, distinctly different, that as many anchors agree with.
    let symmetric = axisymmetric
        || rivals
            .iter()
            .any(|(r, c)| *c >= count && !same_motion(r, &m, tol, th.angle_tol, scale));
    (
        m,
        Alignment {
            found: true,
            rotation: m.r,
            translation: m.t,
            anchors,
            inliers: count,
            symmetric,
            axisymmetric,
            axis,
            rms,
        },
    )
}

fn distance_from_identity(m: &Motion, scale: f64) -> f64 {
    let trace = m.r[0][0] + m.r[1][1] + m.r[2][2];
    (3.0 - trace) + geom::norm(m.t) / scale
}

fn same_motion(a: &Motion, b: &Motion, tol: f64, angle_tol: f64, scale: f64) -> bool {
    let rotation = (0..3).all(|i| geom::dot(a.dir(b_axis(i)), b.dir(b_axis(i))) >= angle_tol.cos());
    // Compare where the two motions put points across the part, not their translations alone.
    rotation && geom::dist(a.t, b.t) <= tol + scale * angle_tol
}

fn b_axis(i: usize) -> V3 {
    let mut v = [0.0; 3];
    v[i] = 1.0;
    v
}

/// Whether two of the points are far enough apart to fix a line.
fn far_apart(points: &[V3], tol: f64) -> bool {
    points
        .iter()
        .any(|p| geom::dist(*p, points[0]) > 10.0 * tol)
}

/// Whether the points span a plane (not all on one line): a rotation is determined by them.
fn spread(points: &[V3], tol: f64) -> bool {
    if points.len() < 3 {
        return false;
    }
    let a = points[0];
    let Some(far) = points
        .iter()
        .max_by(|p, q| geom::dist(**p, a).total_cmp(&geom::dist(**q, a)))
    else {
        return false;
    };
    let Some(u) = geom::unit(geom::sub(*far, a)) else {
        return false;
    };
    if geom::dist(*far, a) <= 10.0 * tol {
        return false;
    }
    points.iter().any(|p| {
        let d = geom::sub(*p, a);
        geom::norm(geom::sub(d, geom::scale(u, geom::dot(d, u)))) > 10.0 * tol
    })
}

/// The proper rotation and translation that best take *from* onto *to* in least squares
/// (Horn 1987: the largest eigenvector of a 4×4 symmetric matrix is the rotation's quaternion).
pub fn horn(from: &[V3], to: &[V3]) -> Option<Motion> {
    if from.len() != to.len() || from.is_empty() {
        return None;
    }
    let n = from.len() as f64;
    let mean = |ps: &[V3]| geom::scale(ps.iter().fold([0.0; 3], |s, p| geom::add(s, *p)), 1.0 / n);
    let (ca, cb) = (mean(from), mean(to));
    let mut s = [[0.0; 3]; 3];
    for (p, q) in from.iter().zip(to) {
        let (a, b) = (geom::sub(*p, ca), geom::sub(*q, cb));
        for i in 0..3 {
            for j in 0..3 {
                s[i][j] += a[i] * b[j];
            }
        }
    }
    let [[sxx, sxy, sxz], [syx, syy, syz], [szx, szy, szz]] = s;
    let mut nm = [
        [sxx + syy + szz, syz - szy, szx - sxz, sxy - syx],
        [syz - szy, sxx - syy - szz, sxy + syx, szx + sxz],
        [szx - sxz, sxy + syx, -sxx + syy - szz, syz + szy],
        [sxy - syx, szx + sxz, syz + szy, -sxx - syy + szz],
    ];
    let (values, vectors) = jacobi4(&mut nm);
    let k = (0..4).max_by(|&a, &b| values[a].total_cmp(&values[b]))?;
    let q = [0, 1, 2, 3].map(|i| vectors[i][k]);
    let norm = q.iter().map(|x| x * x).sum::<f64>().sqrt();
    if norm == 0.0 || norm.is_nan() {
        return None;
    }
    let [w, x, y, z] = q.map(|c| c / norm);
    let r = [
        [
            w * w + x * x - y * y - z * z,
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            w * w - x * x + y * y - z * z,
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            w * w - x * x - y * y + z * z,
        ],
    ];
    let m = Motion { r, t: [0.0; 3] };
    Some(Motion {
        r,
        t: geom::sub(cb, m.dir(ca)),
    })
}

/// Eigenvalues and eigenvectors (columns) of a symmetric 4×4 matrix, by cyclic Jacobi rotations.
fn jacobi4(a: &mut [[f64; 4]; 4]) -> ([f64; 4], [[f64; 4]; 4]) {
    let mut v = [[0.0; 4]; 4];
    for (i, row) in v.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    for _ in 0..100 {
        let off: f64 = (0..4)
            .flat_map(|i| (0..4).filter(move |&j| j != i).map(move |j| (i, j)))
            .map(|(i, j)| a[i][j] * a[i][j])
            .sum();
        let diag: f64 = (0..4).map(|i| a[i][i] * a[i][i]).sum();
        if off <= 1e-30 * diag.max(1e-300) {
            break;
        }
        for p in 0..4 {
            for q in p + 1..4 {
                if a[p][q].abs() < 1e-300 {
                    continue;
                }
                let theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
                let t = theta.signum() / (theta.abs() + (theta * theta + 1.0).sqrt());
                let t = if theta == 0.0 { 1.0 } else { t };
                let c = 1.0 / (t * t + 1.0).sqrt();
                let s = t * c;
                for k in 0..4 {
                    let (akp, akq) = (a[k][p], a[k][q]);
                    a[k][p] = c * akp - s * akq;
                    a[k][q] = s * akp + c * akq;
                }
                for k in 0..4 {
                    let (apk, aqk) = (a[p][k], a[q][k]);
                    a[p][k] = c * apk - s * aqk;
                    a[q][k] = s * apk + c * aqk;
                }
                for row in v.iter_mut() {
                    let (vp, vq) = (row[p], row[q]);
                    row[p] = c * vp - s * vq;
                    row[q] = s * vp + c * vq;
                }
            }
        }
    }
    ([a[0][0], a[1][1], a[2][2], a[3][3]], v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horn_recovers_a_rotation_and_translation() {
        let r = [[0.0, -1.0, 0.0], [0.0, 0.0, -1.0], [1.0, 0.0, 0.0]];
        let m = Motion {
            r,
            t: [5.0, -2.0, 7.5],
        };
        let from = [
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [0.0, 4.0, 0.0],
            [1.0, 2.0, 3.0],
        ];
        let to: Vec<V3> = from.iter().map(|p| m.point(*p)).collect();
        let got = horn(&from, &to).unwrap();
        for i in 0..3 {
            for j in 0..3 {
                assert!((got.r[i][j] - r[i][j]).abs() < 1e-12, "{got:?}");
            }
            assert!((got.t[i] - m.t[i]).abs() < 1e-12);
        }
    }

    #[test]
    fn collinear_points_do_not_fix_a_rotation() {
        assert!(!spread(&[[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]], 1e-6));
        assert!(spread(&[[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], 1e-6));
    }
}
