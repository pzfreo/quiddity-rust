//! Whether support faces cover a patch (`quiddity._support_patches.covered_patch`), and the
//! straight-edged planar shapes the section proofs build to ask it and to probe volumes with:
//! a polygon face (`Face(Wire.make_polygon(...))`) and a closed polyhedron (`Solid(Shell(...))`,
//! a ruled loft or an extrusion of a polygon).
//!
//! Python cuts the patch by each support in turn and skips a support whose conservative box is
//! apart from a fragment's (`_separated`), a saving that cannot change the answer. Here the
//! question is answered in the patch's parameter space by [`kernel::cover`](crate::kernel::cover),
//! checked against every question Python asks over the corpus (`crates/haecceity/tests/
//! patches.rs`), which reads only the supports on the patch's own surface.

use crate::kernel::brep::{Edge, Face, Loop, Part, Solid};
use crate::kernel::cover::{FaceRef, covered};
use crate::kernel::geom::{self, Curve, Frame, Surface, V3};

/// `covered_patch`: whether the supports leave at most 1e-9 of the patch's area uncovered.
pub fn covered_patch(patch: FaceRef<'_>, supports: &[FaceRef<'_>]) -> bool {
    covered(patch, supports)
}

/// The unit normal of a planar loop by Newell's method, its walk counter-clockwise about it.
fn loop_normal(points: &[V3], corners: &[usize]) -> Option<V3> {
    let mut n = [0.0; 3];
    for (i, &a) in corners.iter().enumerate() {
        let (p, q) = (points[a], points[corners[(i + 1) % corners.len()]]);
        n[0] += (p[1] - q[1]) * (p[2] + q[2]);
        n[1] += (p[2] - q[2]) * (p[0] + q[0]);
        n[2] += (p[0] - q[0]) * (p[1] + q[1]);
    }
    geom::unit(n)
}

/// A part of planar faces bounded by straight edges: *faces* lists each face's corners as
/// indices into *points*, walked counter-clockwise seen from the side the face's normal points
/// to. With `closed`, the faces are one solid's shell and must point outward; otherwise they
/// are loose faces. `None` for a face with fewer than three corners or no area, or a repeated
/// corner.
pub fn polygon_part(points: &[V3], faces: &[Vec<usize>], closed: bool) -> Option<Part> {
    let mut edges: Vec<Edge> = Vec::new();
    let mut keys: Vec<(usize, usize)> = Vec::new();
    let mut built = Vec::new();
    for corners in faces {
        if corners.len() < 3 {
            return None;
        }
        let z = loop_normal(points, corners)?;
        let origin = points[corners[0]];
        let x = geom::unit(geom::sub(points[corners[1]], origin))?;
        let x = geom::unit(geom::sub(x, geom::scale(z, geom::dot(x, z))))?;
        let mut uses = Vec::new();
        for (i, &a) in corners.iter().enumerate() {
            let b = corners[(i + 1) % corners.len()];
            if a == b {
                return None;
            }
            let key = (a.min(b), a.max(b));
            let index = keys.iter().position(|&k| k == key).unwrap_or_else(|| {
                let (start, end) = (points[key.0], points[key.1]);
                keys.push(key);
                edges.push(Edge {
                    curve: Curve::Line {
                        origin: start,
                        dir: geom::sub(end, start),
                    },
                    start,
                    end,
                    vertices: key,
                    same_sense: true,
                    samples: vec![start, end],
                });
                edges.len() - 1
            });
            uses.push((index, a < b));
        }
        built.push(Face {
            surface: Surface::Plane {
                frame: Frame {
                    origin,
                    x,
                    y: geom::cross(z, x),
                    z,
                },
            },
            reversed: false,
            loops: vec![Loop {
                edges: uses,
                vertex: None,
            }],
            solid: closed.then_some(0),
            pcurves: Vec::new(),
        });
    }
    let solids = if closed {
        vec![Solid {
            faces: (0..built.len()).collect(),
        }]
    } else {
        Vec::new()
    };
    Some(Part::new(built, edges, solids))
}

/// One polygon face through *corners* in order (`Face(Wire.make_polygon(corners))`), its normal
/// by the walk.
pub fn polygon_face(corners: &[V3]) -> Option<Part> {
    polygon_part(corners, &[(0..corners.len()).collect()], false)
}

/// The closed solid between two copies of one polygon, *low* and *high* corner for corner, each
/// side a planar quad (a ruled loft of two polygons whose corners are joined by parallel lines,
/// `Solid.make_loft(..., ruled=True)`, or an extrusion). `None` unless the result is a valid
/// solid.
pub fn ruled_prism(low: &[V3], high: &[V3]) -> Option<Part> {
    let n = low.len();
    if n < 3 || high.len() != n {
        return None;
    }
    let mut points = low.to_vec();
    points.extend_from_slice(high);
    // Low corners walked so that the normal points away from the high end.
    let axis = geom::sub(high[0], low[0]);
    let low_ring: Vec<usize> = (0..n).collect();
    let forward = geom::dot(loop_normal(&points, &low_ring)?, axis) < 0.0;
    let ring: Vec<usize> = if forward {
        low_ring
    } else {
        (0..n).rev().collect()
    };
    let mut faces = vec![ring.clone(), ring.iter().rev().map(|&i| i + n).collect()];
    for (k, &a) in ring.iter().enumerate() {
        let b = ring[(k + 1) % n];
        faces.push(vec![b, a, a + n, b + n]);
    }
    let part = polygon_part(&points, &faces, true)?;
    part.solid_is_valid(0).then_some(part)
}
