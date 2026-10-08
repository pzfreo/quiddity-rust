//! Record-agnostic pattern geometry (`quiddity._pattern_geometry`): the collinearity, pitch and
//! lattice arithmetic hole, pocket and slot patterns share.
//!
//! Nothing here knows what a pattern record is: each caller passes a `make` that builds its own
//! record from the members and the measured geometry, as Python's callers pass a constructor.
//! Members need only a world centre ([`Located`]).

use crate::kernel::geom::{self, V3, dominant_axis_preferring_z};
use crate::kernel::py;

const PATTERN_REL_TOL: f64 = 0.02;
pub(crate) const PATTERN_ABS_TOL: f64 = 0.1;

/// What a pattern member is to the geometry: its world centre (Python's `.location`).
pub(crate) trait Located {
    fn location(&self) -> V3;
}

/// A pattern candidate: the record and the (sorted) indices of the members it takes, local to
/// the points it was found among.
pub(crate) type Candidate<R> = (R, Vec<usize>);

/// `_pattern_tol`: what two pattern members' spacings may differ by.
pub(crate) fn pattern_tol(nominal: f64) -> f64 {
    PATTERN_REL_TOL * nominal + PATTERN_ABS_TOL
}

/// Two orthonormal vectors spanning the plane perpendicular to *axis*: the dominant axis's
/// canonical in-plane basis, Gram-Schmidt'd against the actual axis (`_plane_uv`). `(0, 0, -1)`
/// and `(0, 0, 1)` give the same pair: a pattern's axis is published as a letter.
pub(crate) fn plane_uv(axis: V3) -> (V3, V3) {
    let a = py::unit(axis).expect("a pattern axis is a unit vector");
    let (u0, v0) = match dominant_axis_preferring_z(a) {
        0 => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        1 => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        _ => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    };
    let project_out = |w: V3, dirs: &[V3]| {
        let mut w = w;
        for d in dirs {
            w = geom::sub(w, geom::scale(*d, py::dot(&w, d)));
        }
        py::unit(w).expect("a seed basis is never parallel to its own axis")
    };
    let u = project_out(u0, &[a]);
    (u, project_out(v0, &[a, u]))
}

fn sub<const D: usize>(a: [f64; D], b: [f64; D]) -> [f64; D] {
    std::array::from_fn(|i| a[i] - b[i])
}

/// Distance along and perpendicular to a directed line (`_line_position`).
fn line_position<const D: usize>(
    point: [f64; D],
    origin: [f64; D],
    direction: [f64; D],
) -> (f64, f64) {
    let rel = sub(point, origin);
    let along = py::dot(&rel, &direction);
    let perpendicular: [f64; D] = std::array::from_fn(|i| rel[i] - along * direction[i]);
    (along, py::hypot(&perpendicular))
}

/// A linear array when the points are collinear at constant pitch (`_as_linear_array`): *make*
/// gets the members ordered along the array, the pitch and the world direction first to last.
fn as_linear_array<M: Located, R, const D: usize>(
    members: &[&M],
    pts: &[[f64; D]],
    make: &impl Fn(Vec<&M>, f64, V3) -> R,
) -> Option<R> {
    let n = pts.len();
    // The farthest-apart pair are the ends, whatever the row's orientation.
    let mut best = (0, 1);
    for i in 0..n {
        for j in i + 1..n {
            if py::dist(&pts[i], &pts[j]) > py::dist(&pts[best.0], &pts[best.1]) {
                best = (i, j);
            }
        }
    }
    let (first, last) = (pts[best.0], pts[best.1]);
    let span = py::dist(&last, &first);
    if span < PATTERN_ABS_TOL {
        return None;
    }
    let direction = sub(last, first).map(|c| c / span);
    // Off-line tolerance scales with the pitch, not the span.
    let line_tol = pattern_tol(span / (n - 1) as f64);
    let mut projections = Vec::with_capacity(n);
    for p in pts {
        let (along, perpendicular) = line_position(*p, first, direction);
        if perpendicular > line_tol {
            return None;
        }
        projections.push(along);
    }
    let mut ts = projections.clone();
    ts.sort_by(|a, b| py::order(*a, *b));
    let pitch = span / (n - 1) as f64;
    if ts
        .windows(2)
        .any(|w| (w[1] - w[0] - pitch).abs() > pattern_tol(pitch))
    {
        return None;
    }
    let mut ordered: Vec<(f64, &M)> = projections
        .into_iter()
        .zip(members.iter().copied())
        .collect();
    ordered.sort_by(|a, b| py::order(a.0, b.0));
    let d = geom::sub(
        ordered[ordered.len() - 1].1.location(),
        ordered[0].1.location(),
    );
    let norm = py::hypot(&d);
    Some(make(
        ordered.into_iter().map(|(_, m)| m).collect(),
        py::round_to(pitch, 2),
        d.map(|c| py::without_negative_zero(c / norm)),
    ))
}

/// Every maximal constant-pitch collinear run of three or more (`_linear_array_candidates`):
/// each pair seeds a line, the points on it are sorted along it and split where the pitch
/// breaks. Indices are local to *pts*.
pub(crate) fn linear_array_candidates<M: Located, R, const D: usize>(
    members: &[&M],
    pts: &[[f64; D]],
    make: impl Fn(Vec<&M>, f64, V3) -> R,
) -> Vec<Candidate<R>> {
    let n = pts.len();
    let (mut out, mut seen) = (Vec::new(), Vec::<Vec<usize>>::new());
    for i in 0..n {
        for j in i + 1..n {
            let span = py::dist(&pts[j], &pts[i]);
            if span < PATTERN_ABS_TOL {
                continue;
            }
            let direction = sub(pts[j], pts[i]).map(|c| c / span);
            let tol = pattern_tol(span);
            let mut positions: Vec<(usize, f64)> = (0..n)
                .filter_map(|m| {
                    let (along, perpendicular) = line_position(pts[m], pts[i], direction);
                    (perpendicular <= tol).then_some((m, along))
                })
                .collect();
            positions.sort_by(|a, b| py::order(a.1, b.1));
            let order: Vec<usize> = positions.iter().map(|p| p.0).collect();
            let ts: Vec<f64> = positions.iter().map(|p| p.1).collect();
            let mut a = 0;
            while a + 2 < order.len() {
                let pitch = ts[a + 1] - ts[a];
                let mut b = a + 1;
                while b + 1 < order.len()
                    && ((ts[b + 1] - ts[b]) - pitch).abs() <= pattern_tol(pitch)
                {
                    b += 1;
                }
                let run = &order[a..=b];
                let mut run_key = run.to_vec();
                run_key.sort_unstable();
                if run.len() >= 3 && !seen.contains(&run_key) {
                    seen.push(run_key.clone());
                    let sub: Vec<&M> = run.iter().map(|&m| members[m]).collect();
                    let sub_pts: Vec<[f64; D]> = run.iter().map(|&m| pts[m]).collect();
                    if let Some(p) = as_linear_array(&sub, &sub_pts, &make) {
                        out.push((p, run_key));
                    }
                }
                // A broken pitch starts the next run at the break.
                a = b;
            }
        }
    }
    out
}

/// The members' mean world centre.
pub(crate) fn mean_location<M: Located>(members: &[&M]) -> V3 {
    let n = members.len() as f64;
    [0, 1, 2].map(|i| py::sum(members.iter().map(|m| m.location()[i])) / n)
}

/// A fully populated N×M lattice of the whole group (`_rect_grid`); 2×2 is a rectangle, not a
/// grid. The shortest pairwise vector is the first basis, along which the columns run; *make*
/// gets `(members, rows, cols, row_pitch, col_pitch, angle, center)`, the angle the column
/// direction's modulo 180°.
pub(crate) fn rect_grid<M: Located, R>(
    members: &[&M],
    pts: &[(f64, f64)],
    make: impl FnOnce(&[&M], usize, usize, f64, f64, f64, V3) -> R,
) -> Option<R> {
    let n = pts.len();
    if n < 6 {
        return None;
    }
    let mut diffs: Vec<(f64, f64, f64)> = Vec::new();
    for i in 0..n {
        for j in 0..n {
            if i == j {
                continue;
            }
            let (dx, dy) = (pts[j].0 - pts[i].0, pts[j].1 - pts[i].1);
            let length = py::hypot(&[dx, dy]);
            if length > PATTERN_ABS_TOL {
                diffs.push((length, dx, dy));
            }
        }
    }
    if diffs.is_empty() {
        return None;
    }
    diffs.sort_by(|a, b| py::tuple_order(&[a.0, a.1, a.2], &[b.0, b.1, b.2]));
    let (l1, b1x, b1y) = diffs[0];
    let u1 = (b1x / l1, b1y / l1);
    let &(l2, b2x, b2y) = diffs
        .iter()
        .find(|(l, dx, dy)| ((dx * u1.0 + dy * u1.1) / l).abs() < 0.2)?;
    let u2 = (b2x / l2, b2y / l2);
    let a0 = pts
        .iter()
        .map(|p| p.0 * u1.0 + p.1 * u1.1)
        .fold(f64::INFINITY, f64::min);
    let b0 = pts
        .iter()
        .map(|p| p.0 * u2.0 + p.1 * u2.1)
        .fold(f64::INFINITY, f64::min);
    let mut cells: Vec<(i64, i64)> = Vec::with_capacity(n);
    for p in pts {
        let da = p.0 * u1.0 + p.1 * u1.1 - a0;
        let db = p.0 * u2.0 + p.1 * u2.1 - b0;
        // Python's round: half to even.
        let (ci, cj) = ((da / l1).round_ties_even(), (db / l2).round_ties_even());
        if (da - ci * l1).abs() > pattern_tol(l1) || (db - cj * l2).abs() > pattern_tol(l2) {
            return None;
        }
        cells.push((ci as i64, cj as i64));
    }
    let mut distinct = cells.clone();
    distinct.sort_unstable();
    distinct.dedup();
    if distinct.len() != n {
        return None;
    }
    let cols = (cells.iter().map(|c| c.0).max()? + 1) as usize;
    let rows = (cells.iter().map(|c| c.1).max()? + 1) as usize;
    if rows < 2 || cols < 2 || rows.max(cols) < 3 || rows * cols != n {
        return None;
    }
    Some(make(
        members,
        rows,
        cols,
        py::round_to(l2, 2),
        py::round_to(l1, 2),
        py::round_to(py::modulo(u1.1.atan2(u1.0).to_degrees(), 180.0), 2),
        mean_location(members),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plane_uv_is_orthonormal_and_canonical_on_principal_axes() {
        assert_eq!(
            plane_uv([0.0, 0.0, -1.0]),
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
        );
        let (u, v) = plane_uv([0.3, 0.1, 0.9]);
        let a = geom::unit([0.3, 0.1, 0.9]).unwrap();
        assert!(
            geom::dot(u, a).abs() < 1e-15
                && geom::dot(v, a).abs() < 1e-15
                && geom::dot(u, v).abs() < 1e-15
        );
    }
}
