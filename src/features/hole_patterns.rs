//! Hole patterns (`quiddity.holes.recognise_hole_patterns`): bolt circles, linear arrays,
//! rectangular grids and four-corner rectangles among holes of one machining spec.
//!
//! Pure arithmetic over [`HoleRecord`]s. Holes are grouped by spec and drilling axis; circles,
//! grids and rectangles are found per shared opening plane, linear arrays in world space. All
//! candidates are allocated greedily, largest first, so each hole joins at most one pattern.

use std::f64::consts::TAU;

use serde::{Deserialize, Serialize};

use super::holes::{CounterBore, HoleRecord};
use crate::kernel::geom::{self, V3, dominant_axis_preferring_z};

const PATTERN_REL_TOL: f64 = 0.02;
const PATTERN_ABS_TOL: f64 = 0.1;
/// Bolt-circle gaps may differ from even spacing by this fraction of it.
const BC_SPACING_FRAC: f64 = 0.04;
/// Opening points share a plane within this fraction of the group's span (the axis snap's
/// bound), plus the coordinate-noise floor.
const OPENING_PLANE_REL_TOL: f64 = 2e-6;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HolePattern {
    RectGrid {
        holes: Vec<HoleRecord>,
        rows: usize,
        cols: usize,
        row_pitch: f64,
        col_pitch: f64,
        angle: f64,
        center: V3,
    },
    RectangularHoleSet {
        holes: Vec<HoleRecord>,
        center: V3,
        width: f64,
        height: f64,
        angle: f64,
    },
    BoltCircle {
        holes: Vec<HoleRecord>,
        center: V3,
        diameter: f64,
    },
    LinearArray {
        holes: Vec<HoleRecord>,
        pitch: f64,
        direction: V3,
    },
}

fn pattern_tol(nominal: f64) -> f64 {
    PATTERN_REL_TOL * nominal + PATTERN_ABS_TOL
}

fn hypot2(a: (f64, f64), b: (f64, f64)) -> f64 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

/// The machining spec holes of one drilled feature share (`HoleSpec.from_hole`): compared with
/// `==`, as a frozen dataclass is in a dict.
#[derive(Clone, Debug, PartialEq)]
struct HoleSpec {
    axis: V3,
    diameter: f64,
    depth: Option<f64>,
    bottom: String,
    cbore: Option<CounterBore>,
    spotface: Option<CounterBore>,
    csink: Option<(f64, f64)>,
}

impl HoleSpec {
    fn of(h: &HoleRecord) -> Self {
        HoleSpec {
            axis: h.axis.map(|c| {
                if c.abs() < 1e-6 {
                    0.0
                } else {
                    geom::round_to(c, 6)
                }
            }),
            diameter: h.diameter,
            depth: (h.bottom != "through").then_some(h.depth),
            bottom: h.bottom.clone(),
            cbore: h.cbore.clone(),
            spotface: h.spotface.clone(),
            csink: h
                .csink
                .as_ref()
                .map(|c| (c.major_diameter, c.included_angle)),
        }
    }
}

/// Two orthonormal vectors spanning the plane perpendicular to *axis*: the dominant axis's
/// canonical in-plane basis, Gram-Schmidt'd against the actual axis (`_plane_uv`).
fn plane_uv(axis: V3) -> (V3, V3) {
    let a = geom::unit(axis).expect("a hole axis is a unit vector");
    let (u0, v0) = match dominant_axis_preferring_z(a) {
        0 => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        1 => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        _ => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    };
    let project_out = |w: V3, dirs: &[V3]| {
        let mut w = w;
        for d in dirs {
            w = geom::sub(w, geom::scale(*d, geom::dot(w, *d)));
        }
        geom::unit(w).expect("a seed basis is never parallel to its own axis")
    };
    let u = project_out(u0, &[a]);
    (u, project_out(v0, &[a, u]))
}

/// Bounded clusters of coordinates, as index lists in ascending order (`cluster_coordinates`).
fn cluster_coordinates(coordinates: &[f64], tol: f64) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..coordinates.len()).collect();
    order.sort_by(|&a, &b| {
        coordinates[a]
            .partial_cmp(&coordinates[b])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for index in order {
        match clusters.last_mut() {
            Some(c) if coordinates[index] - coordinates[c[0]] <= tol => c.push(index),
            _ => clusters.push(vec![index]),
        }
    }
    clusters
}

/// Holes whose openings share one plane perpendicular to *axis* (`_opening_plane_clusters`).
fn opening_plane_clusters(members: &[&HoleRecord], axis: V3) -> Vec<Vec<usize>> {
    let direction = geom::unit(axis).expect("unit axis");
    let origin = members[0].location;
    let offsets: Vec<f64> = members
        .iter()
        .map(|m| geom::dot(geom::sub(m.location, origin), direction))
        .collect();
    let scale = members
        .iter()
        .map(|m| geom::dist(origin, m.location))
        .fold(members[0].diameter, f64::max);
    cluster_coordinates(&offsets, geom::length_tol(scale, OPENING_PLANE_REL_TOL))
}

fn mean_location(holes: &[&HoleRecord]) -> V3 {
    let n = holes.len() as f64;
    [0, 1, 2].map(|i| holes.iter().map(|h| h.location[i]).sum::<f64>() / n)
}

fn owned(holes: &[&HoleRecord]) -> Vec<HoleRecord> {
    holes.iter().map(|h| (*h).clone()).collect()
}

/// A bolt circle when the 2D points are equally spaced on one circle (`_as_bolt_circle`).
fn as_bolt_circle(holes: &[&HoleRecord], pts: &[(f64, f64)]) -> Option<HolePattern> {
    let n = pts.len() as f64;
    let c = (
        pts.iter().map(|p| p.0).sum::<f64>() / n,
        pts.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let radii: Vec<f64> = pts.iter().map(|p| hypot2(c, *p)).collect();
    let r = radii.iter().sum::<f64>() / n;
    if r < PATTERN_ABS_TOL || radii.iter().any(|ri| (ri - r).abs() > pattern_tol(r)) {
        return None;
    }
    let mut angles: Vec<f64> = pts.iter().map(|p| (p.1 - c.1).atan2(p.0 - c.0)).collect();
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut gaps: Vec<f64> = angles.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.push(TAU - (angles[angles.len() - 1] - angles[0]));
    let even = TAU / n;
    if gaps
        .iter()
        .any(|g| (g - even).abs() > BC_SPACING_FRAC * even)
    {
        return None;
    }
    Some(HolePattern::BoltCircle {
        holes: owned(holes),
        center: mean_location(holes),
        diameter: geom::round_to(2.0 * r, 2),
    })
}

/// The circle through three 2D points, `None` when they are collinear (`_circumcircle`).
fn circumcircle(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64)) -> Option<(f64, f64, f64)> {
    let ((ax, ay), (bx, by), (cx, cy)) = (p0, p1, p2);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-9 {
        return None;
    }
    let (a2, b2, c2) = (ax * ax + ay * ay, bx * bx + by * by, cx * cx + cy * cy);
    let ux = (a2 * (by - cy) + b2 * (cy - ay) + c2 * (ay - by)) / d;
    let uy = (a2 * (cx - bx) + b2 * (ax - cx) + c2 * (bx - ax)) / d;
    Some((ux, uy, (ax - ux).hypot(ay - uy)))
}

type Candidate = (HolePattern, Vec<usize>);

/// Every bolt circle in a group: each triple seeds a circle, the points on it are gathered and
/// kept if evenly populated (`_bolt_circle_candidates`). Indices are local to *pts*.
fn bolt_circle_candidates(members: &[&HoleRecord], pts: &[(f64, f64)]) -> Vec<Candidate> {
    let n = pts.len();
    let (mut out, mut seen) = (Vec::new(), Vec::<[f64; 3]>::new());
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let Some((cx, cy, r)) = circumcircle(pts[i], pts[j], pts[k]) else {
                    continue;
                };
                if r < PATTERN_ABS_TOL {
                    continue;
                }
                let key = [
                    geom::round_to(cx, 2),
                    geom::round_to(cy, 2),
                    geom::round_to(r, 2),
                ];
                if seen.contains(&key) {
                    continue;
                }
                seen.push(key);
                let tol = pattern_tol(r);
                let idx: Vec<usize> = (0..n)
                    .filter(|&m| (hypot2((cx, cy), pts[m]) - r).abs() <= tol)
                    .collect();
                if idx.len() < 3 {
                    continue;
                }
                let sub: Vec<&HoleRecord> = idx.iter().map(|&m| members[m]).collect();
                let sub_pts: Vec<(f64, f64)> = idx.iter().map(|&m| pts[m]).collect();
                if let Some(p) = as_bolt_circle(&sub, &sub_pts) {
                    out.push((p, idx));
                }
            }
        }
    }
    out
}

/// Distance along and perpendicular to a directed line.
fn line_position(point: V3, origin: V3, direction: V3) -> (f64, f64) {
    let rel = geom::sub(point, origin);
    let along = geom::dot(rel, direction);
    (
        along,
        geom::norm(geom::sub(rel, geom::scale(direction, along))),
    )
}

/// A linear array when the points are collinear at constant pitch (`_as_linear_array`).
fn as_linear_array(members: &[&HoleRecord], pts: &[V3]) -> Option<HolePattern> {
    let n = pts.len();
    let mut best = (0, 1);
    for i in 0..n {
        for j in i + 1..n {
            if geom::dist(pts[i], pts[j]) > geom::dist(pts[best.0], pts[best.1]) {
                best = (i, j);
            }
        }
    }
    let (first, last) = (pts[best.0], pts[best.1]);
    let span = geom::dist(first, last);
    if span < PATTERN_ABS_TOL {
        return None;
    }
    let direction = geom::scale(geom::sub(last, first), 1.0 / span);
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
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pitch = span / (n - 1) as f64;
    if ts
        .windows(2)
        .any(|w| (w[1] - w[0] - pitch).abs() > pattern_tol(pitch))
    {
        return None;
    }
    let mut ordered: Vec<(f64, &HoleRecord)> = projections
        .into_iter()
        .zip(members.iter().copied())
        .collect();
    ordered.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let d = geom::sub(ordered[ordered.len() - 1].1.location, ordered[0].1.location);
    let norm = geom::norm(d);
    Some(HolePattern::LinearArray {
        holes: ordered.iter().map(|(_, h)| (*h).clone()).collect(),
        pitch: geom::round_to(pitch, 2),
        direction: d.map(|c| geom::without_negative_zero(c / norm)),
    })
}

/// Every maximal constant-pitch collinear run of three or more (`_linear_array_candidates`).
fn linear_array_candidates(members: &[&HoleRecord], pts: &[V3]) -> Vec<Candidate> {
    let n = pts.len();
    let (mut out, mut seen) = (Vec::new(), Vec::<Vec<usize>>::new());
    for i in 0..n {
        for j in i + 1..n {
            let span = geom::dist(pts[i], pts[j]);
            if span < PATTERN_ABS_TOL {
                continue;
            }
            let direction = geom::scale(geom::sub(pts[j], pts[i]), 1.0 / span);
            let tol = pattern_tol(span);
            let mut positions: Vec<(usize, f64)> = (0..n)
                .filter_map(|m| {
                    let (along, perpendicular) = line_position(pts[m], pts[i], direction);
                    (perpendicular <= tol).then_some((m, along))
                })
                .collect();
            positions.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
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
                    let sub: Vec<&HoleRecord> = run.iter().map(|&m| members[m]).collect();
                    let sub_pts: Vec<V3> = run.iter().map(|&m| pts[m]).collect();
                    if let Some(p) = as_linear_array(&sub, &sub_pts) {
                        out.push((p, run_key));
                    }
                }
                a = b;
            }
        }
    }
    out
}

/// A fully populated N×M lattice of the whole group (`_rect_grid`); 2×2 is a rectangle.
fn rect_grid(members: &[&HoleRecord], pts: &[(f64, f64)]) -> Option<HolePattern> {
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
            let length = dx.hypot(dy);
            if length > PATTERN_ABS_TOL {
                diffs.push((length, dx, dy));
            }
        }
    }
    if diffs.is_empty() {
        return None;
    }
    diffs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
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
    Some(HolePattern::RectGrid {
        holes: owned(members),
        rows,
        cols,
        row_pitch: geom::round_to(l2, 2),
        col_pitch: geom::round_to(l1, 2),
        angle: geom::round_to(u1.1.atan2(u1.0).to_degrees().rem_euclid(180.0), 2),
        center: mean_location(members),
    })
}

/// Four holes at the corners of one proved rectangle (`_rectangular_hole_set`).
fn rectangular_hole_set(holes: &[&HoleRecord], pts: &[(f64, f64)]) -> Option<HolePattern> {
    if holes.len() != 4 || pts.len() != 4 {
        return None;
    }
    let span = pts
        .iter()
        .flat_map(|a| pts.iter().map(move |b| hypot2(*a, *b)))
        .fold(f64::NEG_INFINITY, f64::max);
    if span <= PATTERN_ABS_TOL {
        return None;
    }
    let tol = pattern_tol(span);
    let mid = |a: usize, b: usize| ((pts[a].0 + pts[b].0) / 2.0, (pts[a].1 + pts[b].1) / 2.0);
    [((0, 1), (2, 3)), ((0, 2), (1, 3)), ((0, 3), (1, 2))]
        .into_iter()
        .find(|&(f, s)| {
            hypot2(mid(f.0, f.1), mid(s.0, s.1)) <= tol
                && (hypot2(pts[f.0], pts[f.1]) - hypot2(pts[s.0], pts[s.1])).abs() <= tol
        })?;
    let c = (
        pts.iter().map(|p| p.0).sum::<f64>() / 4.0,
        pts.iter().map(|p| p.1).sum::<f64>() / 4.0,
    );
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by(|&a, &b| {
        let angle = |i: usize| (pts[i].1 - c.1).atan2(pts[i].0 - c.0);
        angle(a)
            .partial_cmp(&angle(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let vectors: Vec<(f64, f64)> = (0..4)
        .map(|i| {
            (
                pts[order[(i + 1) % 4]].0 - pts[order[i]].0,
                pts[order[(i + 1) % 4]].1 - pts[order[i]].1,
            )
        })
        .collect();
    let lengths: Vec<f64> = vectors.iter().map(|v| v.0.hypot(v.1)).collect();
    let shortest = lengths.iter().copied().fold(f64::INFINITY, f64::min);
    if shortest <= tol
        || (lengths[0] - lengths[2]).abs() > tol
        || (lengths[1] - lengths[3]).abs() > tol
    {
        return None;
    }
    if (vectors[0].0 * vectors[1].0 + vectors[0].1 * vectors[1].1).abs() > tol * span {
        return None;
    }
    let long = if lengths[0] >= lengths[1] { 0 } else { 1 };
    let (width, height) = (lengths[long], lengths[1 - long]);
    let mut angle = vectors[long]
        .1
        .atan2(vectors[long].0)
        .to_degrees()
        .rem_euclid(180.0);
    if (width - height).abs() <= tol {
        angle = angle.rem_euclid(90.0);
    }
    let ordered: Vec<&HoleRecord> = order.iter().map(|&i| holes[i]).collect();
    Some(HolePattern::RectangularHoleSet {
        holes: owned(&ordered),
        center: mean_location(holes),
        width: geom::round_to(width, 2),
        height: geom::round_to(height, 2),
        angle: geom::round_to(angle, 2),
    })
}

/// `recognise_hole_patterns`.
pub fn recognise_hole_patterns(holes: &[HoleRecord]) -> Vec<HolePattern> {
    let mut groups: Vec<(HoleSpec, Vec<&HoleRecord>)> = Vec::new();
    for h in holes {
        let spec = HoleSpec::of(h);
        match groups.iter_mut().find(|(s, _)| *s == spec) {
            Some((_, members)) => members.push(h),
            None => groups.push((spec, vec![h])),
        }
    }
    let mut patterns = Vec::new();
    for (spec, members) in groups {
        if members.len() < 3 {
            continue;
        }
        let (u, v) = plane_uv(spec.axis);
        let pts: Vec<(f64, f64)> = members
            .iter()
            .map(|h| (geom::dot(h.location, u), geom::dot(h.location, v)))
            .collect();
        let planes = opening_plane_clusters(&members, spec.axis);
        if planes.len() == 1 {
            if let Some(grid) = rect_grid(&members, &pts) {
                patterns.push(grid);
                continue;
            }
        }
        let mut candidates: Vec<Candidate> = Vec::new();
        for indices in &planes {
            if indices.len() < 3 {
                continue;
            }
            let plane_members: Vec<&HoleRecord> = indices.iter().map(|&i| members[i]).collect();
            let plane_points: Vec<(f64, f64)> = indices.iter().map(|&i| pts[i]).collect();
            if let Some(rectangle) = rectangular_hole_set(&plane_members, &plane_points) {
                candidates.push((rectangle, indices.clone()));
            }
            if let Some(grid) = rect_grid(&plane_members, &plane_points) {
                candidates.push((grid, indices.clone()));
            }
            for (pattern, local) in bolt_circle_candidates(&plane_members, &plane_points) {
                candidates.push((pattern, local.iter().map(|&l| indices[l]).collect()));
            }
        }
        let locations: Vec<V3> = members.iter().map(|m| m.location).collect();
        candidates.extend(linear_array_candidates(&members, &locations));
        candidates.sort_by_key(|c| std::cmp::Reverse(c.1.len()));
        let mut used: Vec<usize> = Vec::new();
        for (pattern, idx) in candidates {
            if idx.iter().any(|i| used.contains(i)) {
                continue;
            }
            used.extend(idx);
            patterns.push(pattern);
        }
    }
    patterns
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

    #[test]
    fn clusters_are_bounded_not_chained() {
        let c = cluster_coordinates(&[0.0, 0.4, 0.8, 1.2], 0.5);
        assert_eq!(c, vec![vec![0, 1], vec![2, 3]]);
    }
}
