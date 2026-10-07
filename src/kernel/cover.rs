//! Whether faces cover a face — what the Python implementation asks of repeated face booleans
//! (`_support_patches.covered_patch`): cut the patch by each support and see whether the area
//! left is at most 1e-9 of it.
//!
//! A support cuts area from the patch only where it lies on the patch's surface, so only
//! supports on the same plane or cylinder count. In the patch's parameter space the question is
//! two-dimensional: along each line `v = c` the patch's intervals, less every support's, leave
//! a length uncovered, and the uncovered area is that length integrated over `v`, between the
//! heights of the loops' vertices. Two faces sharing a curved edge sample it differently, so a
//! support's intervals are widened by the samples' chordal error either side; a gap narrower
//! than that is no gap.

use super::brep::Part;
use super::geom::{self, Surface, V3};
use super::sampling::CHORD_TOLERANCE;

/// A face of a part.
pub type FaceRef<'a> = (&'a Part, usize);

/// `covered_patch`: whether the supports leave at most 1e-9 of the patch's area uncovered.
pub fn covered(patch: FaceRef<'_>, supports: &[FaceRef<'_>]) -> bool {
    let (part, face) = patch;
    let surface = &part.faces[face].surface;
    let (scale, widen) = match *surface {
        Surface::Plane { .. } => (1.0, 2.0 * CHORD_TOLERANCE),
        Surface::Cylinder { radius, .. } => (radius, 2.0 * CHORD_TOLERANCE / radius),
        _ => return false,
    };
    let Some(area) = part.face_mass(face).map(|m| m[0]) else {
        return false;
    };
    let mine = loops_in(part, face, surface);
    // A support's loops on a cylinder are put on the patch's turn.
    let mean = |lp: &[(f64, f64)]| lp.iter().map(|q| q.0).sum::<f64>() / lp.len().max(1) as f64;
    let centre = mean(&mine.concat());
    let periodic = matches!(surface, Surface::Cylinder { .. });
    let theirs: Vec<Vec<Vec<(f64, f64)>>> = supports
        .iter()
        .filter(|(p, f)| same_surface(surface, &p.faces[*f].surface))
        .map(|&(p, f)| {
            let mut loops = loops_in(p, f, surface);
            if periodic {
                for lp in &mut loops {
                    let shift = geom::nearest_turn(mean(lp), centre) - mean(lp);
                    lp.iter_mut().for_each(|q| q.0 += shift);
                }
            }
            loops
        })
        .collect();
    let turns: &[f64] = if periodic {
        &[-std::f64::consts::TAU, 0.0, std::f64::consts::TAU]
    } else {
        &[0.0]
    };
    let mut vs: Vec<f64> = mine.iter().flatten().map(|q| q.1).collect();
    vs.extend(theirs.iter().flatten().flatten().map(|q| q.1));
    vs.sort_by(f64::total_cmp);
    vs.dedup_by(|a, b| *a - *b <= 1e-12);
    let uncovered = |v: f64| {
        let mut left = intervals(&mine, v);
        for support in &theirs {
            for (a, b) in intervals(support, v) {
                // Round a cylinder, the support covers the same stretch a turn either way.
                for turn in turns {
                    left = subtract(&left, (a + turn - widen, b + turn + widen));
                }
            }
        }
        left.iter()
            .map(|(a, b)| b - a)
            .filter(|&l| l > 1e-9)
            .sum::<f64>()
    };
    let gap: f64 = vs
        .windows(2)
        .map(|w| {
            let (h, c) = (0.5 * (w[1] - w[0]), 0.5 * (w[0] + w[1]));
            let t = h / 3f64.sqrt();
            h * (uncovered(c - t) + uncovered(c + t))
        })
        .sum();
    gap * scale <= 1e-9 * area
}

/// Whether two surfaces are one: the same plane, or the same cylinder.
fn same_surface(a: &Surface, b: &Surface) -> bool {
    const TOL: f64 = 1e-6;
    match (a, b) {
        (Surface::Plane { frame: f }, Surface::Plane { frame: g }) => {
            geom::dot(f.z, g.z).abs() > 1.0 - 1e-9
                && geom::dot(geom::sub(g.origin, f.origin), f.z).abs() <= TOL
        }
        (
            Surface::Cylinder {
                frame: f,
                radius: r,
            },
            Surface::Cylinder {
                frame: g,
                radius: s,
            },
        ) => {
            let off = geom::sub(g.origin, f.origin);
            let across = geom::sub(off, geom::scale(f.z, geom::dot(off, f.z)));
            geom::dot(f.z, g.z).abs() > 1.0 - 1e-9
                && geom::norm(across) <= TOL
                && (r - s).abs() <= TOL
        }
        _ => false,
    }
}

/// A face's loops as closed polygons in the parameters of *surface* (the patch's), each
/// periodic parameter unwrapped along the loop.
fn loops_in(part: &Part, face: usize, surface: &Surface) -> Vec<Vec<(f64, f64)>> {
    let periodic_u = matches!(surface, Surface::Cylinder { .. });
    let mut out = Vec::new();
    for lp in &part.faces[face].loops {
        let mut points: Vec<V3> = Vec::new();
        for &(e, forward) in &lp.edges {
            let samples = &part.edges[e].samples;
            let ordered: Vec<V3> = if forward {
                samples.clone()
            } else {
                samples.iter().rev().copied().collect()
            };
            for p in ordered {
                if points.last().is_none_or(|q| geom::dist(*q, p) > 1e-12) {
                    points.push(p);
                }
            }
        }
        let mut uv: Vec<(f64, f64)> = Vec::new();
        for p in points {
            let Some((mut u, v)) = surface.parameters(p, None) else {
                continue;
            };
            if periodic_u && let Some(&(last, _)) = uv.last() {
                u = geom::nearest_turn(u, last);
            }
            uv.push((u, v));
        }
        if let (Some(&first), Some(&last)) = (uv.first(), uv.last())
            && first != last
        {
            uv.push(first);
        }
        out.push(uv);
    }
    out
}

/// The intervals of `u` the polygons enclose along `v`, by the parity of their crossings.
fn intervals(loops: &[Vec<(f64, f64)>], v: f64) -> Vec<(f64, f64)> {
    let mut us: Vec<f64> = Vec::new();
    for lp in loops {
        for w in lp.windows(2) {
            let ((u0, v0), (u1, v1)) = (w[0], w[1]);
            if (v0 <= v) != (v1 <= v) {
                us.push(u0 + (u1 - u0) * (v - v0) / (v1 - v0));
            }
        }
    }
    us.sort_by(f64::total_cmp);
    us.as_chunks::<2>().0.iter().map(|&[a, b]| (a, b)).collect()
}

/// The intervals left once (a, b) is taken away.
fn subtract(from: &[(f64, f64)], (a, b): (f64, f64)) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for &(x, y) in from {
        if b <= x || y <= a {
            out.push((x, y));
            continue;
        }
        if x < a {
            out.push((x, a));
        }
        if b < y {
            out.push((b, y));
        }
    }
    out
}
