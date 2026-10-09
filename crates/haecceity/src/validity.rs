//! Per-face geometric validity: the face half of `BRepCheck_Analyzer` that the topological check
//! ([`Part::solid_is_valid`]) leaves out (`BRepCheck_Face`).
//!
//! Two faults of a face's loops are looked for, both read from the loops as the file writes them
//! (with their bounds' orientation flags):
//!
//! - loops that cross (`BRepCheck_IntersectingWires`): one loop's boundary runs through
//!   another's, so the face's region is not defined by them;
//! - a loop wound the wrong way (`BRepCheck_BadOrientationOfSubshape`): a face's material lies
//!   on the left of its boundary, seen against the outward normal, so the outer loop runs
//!   counter-clockwise round the normal and every inner loop (a hole) clockwise.
//!
//! Only planar faces are checked: their parameters are the plane's own coordinates, so a loop is
//! a polygon there with its winding as the sign of its area. Over the corpus OpenCascade faults
//! two faces after its import, both on parts it also heals; the kernel faults one of them
//! (`tests/validity.rs` in this crate has the evidence):
//!
//! - `inventory_refusal/14052.step.gz`, planar face 1: as written its circular hole crosses the
//!   outer loop (the circle reaches y = 4.455, the boundary runs along y = 5.126), which
//!   `BRepCheck_Face` calls `BRepCheck_IntersectingWires` on the face rebuilt from the file's
//!   loops. OpenCascade's import splits the circle where it crosses and turns the triangular
//!   hole's loop round, so after import it reports `BRepCheck_BadOrientationOfSubshape` on the
//!   same face. Either way the face is faulted, and only it.
//! - `cadgenbench_inputs/cgb202.step.gz`, face 1654 (`BRepCheck_UnorientableShape` with a
//!   self-intersecting wire): made by the import's healing re-orienting wires; the file's own
//!   loops are closed and consistently oriented, and the kernel faults nothing there.

use super::brep::Part;
use super::geom::Surface;

/// A planar loop: its polygon and the polygon's box.
struct Polygon<'a> {
    points: &'a [(f64, f64)],
    lo: (f64, f64),
    hi: (f64, f64),
}

impl<'a> Polygon<'a> {
    fn new(points: &'a [(f64, f64)]) -> Self {
        let (mut lo, mut hi) = (
            (f64::INFINITY, f64::INFINITY),
            (f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for &(u, v) in points {
            lo = (lo.0.min(u), lo.1.min(v));
            hi = (hi.0.max(u), hi.1.max(v));
        }
        Polygon { points, lo, hi }
    }

    /// The diagonal of the box.
    fn extent(&self) -> f64 {
        (self.hi.0 - self.lo.0).hypot(self.hi.1 - self.lo.1)
    }

    /// The edges, closing back to the first point.
    fn segments(&self) -> impl Iterator<Item = ((f64, f64), (f64, f64))> + '_ {
        let n = self.points.len();
        (0..n).map(move |i| (self.points[i], self.points[(i + 1) % n]))
    }

    /// The signed area, positive counter-clockwise (the shoelace formula, about the first point
    /// so the part's placement does not enter the round-off).
    fn signed_area(&self) -> f64 {
        let Some(&(u0, v0)) = self.points.first() else {
            return 0.0;
        };
        0.5 * self
            .segments()
            .map(|(a, b)| (a.0 - u0) * (b.1 - v0) - (b.0 - u0) * (a.1 - v0))
            .sum::<f64>()
    }
}

impl Part {
    /// Whether planar face *face*'s loops are faulted: two of them cross, or one is wound
    /// against the outward normal (its outer loop clockwise, or an inner loop counter-clockwise).
    /// `false` for a face this does not check (not planar).
    pub(super) fn face_loops_faulted(&self, face: usize) -> bool {
        let f = &self.faces[face];
        if !matches!(f.surface, Surface::Plane { .. }) {
            return false;
        }
        let (Some(loops), Some(outer)) = (self.uv_loops(face), self.outer_loop(face)) else {
            return false;
        };
        let polygons: Vec<Polygon<'_>> = loops.iter().map(|l| Polygon::new(&l.points)).collect();
        let scale = polygons.iter().map(Polygon::extent).fold(0.0, f64::max);
        let crossing = (0..polygons.len()).any(|i| {
            (i + 1..polygons.len()).any(|j| loops_cross(&polygons[i], &polygons[j], scale))
        });
        let sense = if f.reversed { -1.0 } else { 1.0 };
        crossing
            || polygons.iter().enumerate().any(|(i, p)| {
                let area = sense * p.signed_area();
                let extent = p.extent();
                // A loop with no width has no winding to read.
                area.abs() > 1e-12 * extent * extent && (i == outer) == (area < 0.0)
            })
    }
}

/// Whether an edge of polygon *a* crosses one of *b*'s: each passes strictly from one side of
/// the other to the other side. Loops touching (at a shared vertex, or along a shared stretch)
/// do not cross.
fn loops_cross(a: &Polygon<'_>, b: &Polygon<'_>, scale: f64) -> bool {
    if a.hi.0 < b.lo.0 || b.hi.0 < a.lo.0 || a.hi.1 < b.lo.1 || b.hi.1 < a.lo.1 {
        return false;
    }
    // Twice the area of a triangle whose apex is off the base line by more than round-off.
    let floor = 1e-12 * scale * scale;
    let side = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        let s = (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0);
        if s > floor {
            1
        } else if s < -floor {
            -1
        } else {
            0
        }
    };
    let overlaps = |(p, q): ((f64, f64), (f64, f64)), (r, s): ((f64, f64), (f64, f64))| {
        p.0.min(q.0) <= r.0.max(s.0)
            && r.0.min(s.0) <= p.0.max(q.0)
            && p.1.min(q.1) <= r.1.max(s.1)
            && r.1.min(s.1) <= p.1.max(q.1)
    };
    a.segments().any(|(p, q)| {
        b.segments().any(|(r, s)| {
            overlaps((p, q), (r, s))
                && side(p, q, r) * side(p, q, s) < 0
                && side(r, s, p) * side(r, s, q) < 0
        })
    })
}

impl Part {
    /// How many shells solid *solid* has: the groups of its faces joined through shared edges
    /// (`solid.shells()`, for a solid whose topology [`Part::solid_is_valid`] accepts).
    pub fn shell_count(&self, solid: usize) -> usize {
        let faces = &self.solids[solid].faces;
        let mut seen = vec![false; self.faces.len()];
        let mut shells = 0;
        for &start in faces {
            if seen[start] {
                continue;
            }
            shells += 1;
            seen[start] = true;
            let mut stack = vec![start];
            while let Some(f) = stack.pop() {
                for n in self.neighbours(f) {
                    if !seen[n] && self.faces[n].solid == Some(solid) {
                        seen[n] = true;
                        stack.push(n);
                    }
                }
            }
        }
        shells
    }
}
