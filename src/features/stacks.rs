//! Coaxial cylinder segments read at their ends (`quiddity._cylinder_stacks`): which faces lie
//! beyond an axial end, and whether that end is open, flat, a drill point, or unknown. Shared by
//! holes (internal segments) and bosses (external ones).

use super::cylinders::{STACK_GAP_FRAC, Segment};
use crate::kernel::brep::Part;
use crate::kernel::geom::{self, Surface, V3};
use crate::kernel::py;

/// What lies beyond one axial end of a segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Open,
    Flat,
    DrillPoint,
    Unknown,
}

/// The faces beyond one axial end of a segment: partners across the segment's edges that lie at
/// that end (within a margin for lips dipping off the end plane), nearest first
/// (`_end_partners`).
pub fn end_partners(part: &Part, seg: &Segment, s_end: f64) -> Vec<usize> {
    let d = seg.direction;
    let margin = geom::length_tol(seg.diameter, STACK_GAP_FRAC)
        .max((0.45 * (seg.s_hi - seg.s_lo)).min(0.5 * seg.diameter));
    let edge_faces = part.edge_faces();
    // (distance, first-seen order, face)
    let mut ranked: Vec<(f64, usize, usize)> = Vec::new();
    for &face in &seg.faces {
        for edge in part.face_edges(face) {
            let e = &part.edges[edge];
            let distance = std::iter::once(e.midpoint())
                .chain(e.vertex_points())
                .map(|p| (geom::dot(p, d) - s_end).abs())
                .fold(f64::NEG_INFINITY, f64::max);
            if distance > margin {
                continue;
            }
            for &partner in &edge_faces[edge] {
                if seg.faces.contains(&partner) {
                    continue;
                }
                match ranked.iter_mut().find(|r| r.2 == partner) {
                    None => {
                        let order = ranked.len();
                        ranked.push((distance, order, partner));
                    }
                    Some(r) if distance < r.0 => r.0 = distance,
                    Some(_) => {}
                }
            }
        }
    }
    ranked.sort_by(|a, b| py::order(a.0, b.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().map(|r| r.2).collect()
}

/// Classify one axial end of a segment from the face beyond it (`_classify_end`), with the
/// faces that close it (its `terminal_faces`). Planes, cones and tori decide; a curved wall is a
/// weak signal that counts only when nothing decides.
pub fn classify_end(part: &Part, seg: &Segment, s_end: f64, hi_end: bool) -> (End, Vec<usize>) {
    let d = seg.direction;
    let e_sign = if hi_end { 1.0 } else { -1.0 };
    let mut weak = None;
    for partner in end_partners(part, seg, s_end) {
        let surface = &part.faces[partner].surface;
        match surface {
            Surface::Cone { .. } => {
                let apex = surface.cone_apex().expect("a cone");
                let outward = (geom::dot(apex, d) - s_end) * e_sign > 0.0;
                if seg.external {
                    return (if outward { End::Open } else { End::Flat }, vec![]);
                }
                if !outward {
                    return (End::Open, vec![]); // an entry chamfer or countersink widens the bore
                }
                // Apex outward closes the bore — unless a plane across the cone faces back
                // along the axis, which makes it a chamfered flat floor.
                for e2 in part.face_edges(partner) {
                    for &n in &part.edge_faces()[e2] {
                        if n == partner || seg.faces.contains(&n) {
                            continue;
                        }
                        if plane_normal(part, n)
                            .is_some_and(|normal| geom::dot(normal, d).abs() > 0.9)
                        {
                            return (End::Flat, vec![n]);
                        }
                    }
                }
                return (End::DrillPoint, vec![partner]);
            }
            Surface::Torus { major, .. } => {
                let curls_in = *major < seg.diameter / 2.0;
                let state = if seg.external != curls_in {
                    End::Flat
                } else {
                    End::Open
                };
                return (state, vec![]);
            }
            Surface::Plane { .. } => {
                let normal = plane_normal(part, partner).expect("a plane");
                let alignment = py::dot(&normal, &d) * e_sign;
                if alignment < -0.5 {
                    return (End::Flat, vec![partner]);
                }
                if alignment > 0.5 {
                    return (End::Open, vec![partner]);
                }
            }
            Surface::Sphere { .. } => {
                let convex = part.frame_points_outward(partner).unwrap_or(false);
                let state = if seg.external == convex {
                    End::Flat
                } else {
                    End::Open
                };
                weak = Some((state, vec![partner]));
            }
            Surface::Cylinder { .. } => {
                weak = Some((if seg.external { End::Flat } else { End::Open }, vec![]));
            }
            // A freeform face may be an exact plane in spline form; Python certifies that
            // through its effective-surface recovery, which this port does not yet have.
            _ => {}
        }
    }
    weak.unwrap_or((End::Unknown, vec![]))
}

/// A planar face's outward normal.
fn plane_normal(part: &Part, face: usize) -> Option<V3> {
    match part.faces[face].surface {
        Surface::Plane { .. } => part.face_normal(face, 0.0, 0.0),
        _ => None,
    }
}
