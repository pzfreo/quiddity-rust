//! Original-support proof for open, at-most-semicircular cylindrical channels
//! (`quiddity._cylindrical_seats`): a concave native cylinder, possibly split at seams, whose
//! only neighbours are convex planar stock faces meeting it in two circular rims (one at each
//! axial end) and straight lips on one mouth plane. The proof rebuilds the trough as the arc
//! section swept along the axis, requires its cylindrical face and the original walls to cover
//! each other exactly, and probes the trough, a slab beyond each end and a slab outside the
//! mouth for material.
//!
//! An unanswered probe (or a section the kernel cannot sweep) proves nothing, so refuses.

use std::collections::BTreeSet;
use std::f64::consts::PI;

use super::Context;
use super::entry_treatments::material_fraction;
use super::evidence::common_valid_solid;
use super::graph::{face_vertices, is_planar, normal, shared_occurrences};
use super::policy::length_tol;
use super::regions::edge_length;
use super::sections::{LocalFrame, SectionVertex};
use super::support_patches::{covered_patch, ruled_prism};
use crate::kernel::brep::{Arc, Edge, Face, Loop, Part};
use crate::kernel::geom::{self, Curve, Frame, Surface, V3};
use crate::kernel::sampling::{edge_interval, sample_edge};
use crate::kernel::sweep::extrude_face;

/// A proved seat (`CylindricalSeatProof`): its wall faces, the planar faces around them, the
/// solid, the frame along the axis through the section's centroid, the open arc section as two
/// vertices (the first carrying the arc's bulge) and the axial interval.
#[derive(Clone, Debug, PartialEq)]
pub struct CylindricalSeatProof {
    pub walls: Vec<usize>,
    pub context: Vec<usize>,
    pub owner: usize,
    pub frame: LocalFrame,
    pub boundary: [SectionVertex; 2],
    pub run_interval: (f64, f64),
}

/// A native cylinder as the proof reads it (`_Cylinder`): radius, canonical axis direction and
/// the located axis point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeatCylinder {
    pub radius: f64,
    pub axis: V3,
    pub origin: V3,
}

impl SeatCylinder {
    /// Whether *other* is the same surface (`same_surface`): radius and axis line within
    /// `length_tol(radius, 1e-6)`, directions parallel to 1e-8.
    fn same_surface(&self, other: &SeatCylinder) -> bool {
        let tolerance = length_tol(self.radius, 1e-6);
        let delta = geom::sub(other.origin, self.origin);
        let across = geom::sub(delta, geom::scale(self.axis, geom::dot(delta, self.axis)));
        (self.radius - other.radius).abs() <= tolerance
            && geom::norm(geom::cross(self.axis, other.axis)) <= 1e-8
            && geom::norm(across) <= tolerance
    }
}

/// The edge's points at the start, middle and end of its parameter range (`position_at(0)`,
/// `(0.5)`, `(1)`: by arc length, which for lines and circles is by parameter).
fn samples(edge: &Edge) -> [V3; 3] {
    let (a, b) = edge_interval(
        &edge.curve,
        edge.start,
        edge.end,
        edge.same_sense,
        edge.is_closed(),
    );
    [a, 0.5 * (a + b), b].map(|t| edge.curve.value(t))
}

/// The circle through three points, as `(centre, radius, normal)` with the walk start → middle
/// → end counter-clockwise about the normal, and the length of that arc
/// (`Edge.make_three_point_arc`). `None` for collinear points.
fn three_point_arc(start: V3, middle: V3, end: V3) -> Option<(V3, f64, V3, f64)> {
    let (a, b) = (geom::sub(start, middle), geom::sub(end, middle));
    let normal = geom::cross(geom::sub(middle, start), geom::sub(end, middle));
    let n2 = geom::dot(normal, normal);
    if n2 <= 1e-300 {
        return None;
    }
    // Circumcentre relative to the middle point.
    let ab = geom::cross(a, b);
    let offset = geom::scale(
        geom::add(
            geom::scale(geom::cross(ab, a), geom::dot(b, b)),
            geom::scale(geom::cross(b, ab), geom::dot(a, a)),
        ),
        0.5 / n2,
    );
    let centre = geom::add(middle, offset);
    let radius = geom::norm(offset);
    let normal = geom::unit(normal)?;
    let turn = |p: V3, q: V3| {
        let (p, q) = (geom::sub(p, centre), geom::sub(q, centre));
        let angle = geom::dot(geom::cross(p, q), normal).atan2(geom::dot(p, q));
        if angle < 0.0 { angle + 2.0 * PI } else { angle }
    };
    let length = radius * (turn(start, middle) + turn(middle, end));
    Some((centre, radius, normal, length))
}

/// The planar region between the arc start → end about *frame* (its z the arc's normal) and the
/// chord back (`Face(Wire([arc, Edge.make_line(end, start)]))`), as a one-face part.
fn segment_face(frame: Frame, radius: f64, start: V3, end: V3) -> Part {
    let arc = Curve::Circle { frame, radius };
    let chord = Curve::Line {
        origin: end,
        dir: geom::sub(start, end),
    };
    let edges = vec![
        Edge {
            samples: sample_edge(&arc, start, end, true, false),
            curve: arc,
            start,
            end,
            vertices: (0, 1),
            same_sense: true,
        },
        Edge {
            curve: chord,
            start: end,
            end: start,
            vertices: (1, 0),
            same_sense: true,
            samples: vec![end, start],
        },
    ];
    let face = Face {
        surface: Surface::Plane { frame },
        reversed: false,
        loops: vec![Loop {
            edges: vec![(0, true), (1, true)],
            vertex: None,
        }],
        solid: None,
        pcurves: Vec::new(),
    };
    Part::new(vec![face], edges, Vec::new())
}

/// Whether *probe* is built and holds no more than 1e-9 of its volume in material; an unbuilt
/// or unanswered probe proves nothing.
fn empty(ctx: &Context<'_>, owner: usize, probe: Option<&Part>) -> bool {
    probe
        .and_then(|p| material_fraction(ctx, owner, p))
        .is_some_and(|f| f <= 1e-9)
}

/// The seat proof for one group of same-surface walls, sorted by face (`_prove`); `None` for
/// anything outside the contract.
pub fn prove(
    ctx: &Context<'_>,
    walls: &[usize],
    cylinder: &SeatCylinder,
) -> Option<CylindricalSeatProof> {
    let part = ctx.part;
    let own: BTreeSet<usize> = walls.iter().copied().collect();
    let context: Vec<usize> = walls
        .iter()
        .flat_map(|&w| part.neighbours(w))
        .filter(|n| !own.contains(n))
        .collect::<BTreeSet<usize>>()
        .into_iter()
        .collect();
    let members: Vec<usize> = walls.iter().chain(&context).copied().collect();
    let owner = common_valid_solid(part, &members)?;
    if context.is_empty() || context.iter().any(|&n| !is_planar(part, n)) {
        return None;
    }
    let (radius, axis) = (cylinder.radius, cylinder.axis);
    let tolerance = length_tol(radius, 1e-6);
    let along: Vec<f64> = walls
        .iter()
        .flat_map(|&w| face_vertices(part, w))
        .map(|p| geom::dot(p, axis))
        .collect();
    if along.is_empty() {
        return None;
    }
    let low = along.iter().copied().fold(f64::INFINITY, f64::min);
    let high = along.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if high - low <= 2.0 * tolerance {
        return None;
    }
    let mut rims: [BTreeSet<usize>; 2] = [BTreeSet::new(), BTreeSet::new()];
    let mut lips: BTreeSet<usize> = BTreeSet::new();
    let mut opening_normal: Option<V3> = None;
    for &wall in walls {
        for node in part.neighbours(wall) {
            if own.contains(&node) {
                continue;
            }
            // Tangent fillets and interior corner blends are not open seats.
            if part.arc(wall, node) != Some(Arc::Convex) {
                return None;
            }
            let normal = normal(part, node)?;
            for e in shared_occurrences(part, wall, node) {
                let edge = &part.edges[e];
                let points = samples(edge);
                match edge.curve {
                    Curve::Circle { .. } => {
                        let middle = geom::dot(points[1], axis);
                        let end_index = usize::from((middle - low).abs() >= (middle - high).abs());
                        let (at, sign) = if end_index == 0 {
                            (low, -1.0)
                        } else {
                            (high, 1.0)
                        };
                        if geom::dot(normal, axis) * sign < 1.0 - 1e-8
                            || points
                                .iter()
                                .any(|&p| (geom::dot(p, axis) - at).abs() > tolerance)
                        {
                            return None;
                        }
                        rims[end_index].insert(e);
                    }
                    Curve::Line { dir, .. } => {
                        // Native seams may leave arbitrarily short lip fragments. Prove
                        // direction from the line, independent of fragment length; the complete
                        // axial span has its own minimum above.
                        let tangent = geom::unit(dir)?;
                        if geom::norm(geom::sub(points[2], points[0])) == 0.0
                            || geom::norm(geom::cross(tangent, axis)) > 1e-8
                            || geom::dot(normal, axis).abs() > 1e-8
                        {
                            return None;
                        }
                        if opening_normal.is_some_and(|o| geom::dot(normal, o) < 1.0 - 1e-8) {
                            return None;
                        }
                        opening_normal = Some(normal);
                        lips.insert(e);
                    }
                    _ => return None,
                }
            }
        }
    }
    let opening_normal = opening_normal?;
    if rims.iter().any(BTreeSet::is_empty) || lips.is_empty() {
        return None;
    }
    // A rim may be subdivided at a native seam. Its two degree-one vertices determine the
    // physical arc; all subdivisions retain their source faces.
    let mut counts: Vec<(usize, V3, usize)> = Vec::new();
    for &e in &rims[0] {
        let edge = &part.edges[e];
        let mut ends = vec![(edge.vertices.0, edge.start)];
        if !edge.is_closed() {
            ends.push((edge.vertices.1, edge.end));
        }
        for (vertex, point) in ends {
            match counts.iter_mut().find(|c| c.0 == vertex) {
                Some(c) => c.2 += 1,
                None => counts.push((vertex, point, 1)),
            }
        }
    }
    let ends: Vec<V3> = counts.iter().filter(|c| c.2 == 1).map(|c| c.1).collect();
    if ends.len() != 2 || counts.iter().any(|c| !matches!(c.2, 1 | 2)) {
        return None;
    }
    let sweep = rims[0].iter().map(|&e| edge_length(part, e)).sum::<f64>() / radius;
    if !(1e-6 < sweep && sweep <= PI + 1e-8) {
        return None;
    }
    let (start, end) = (ends[0], ends[1]);
    if geom::dot(geom::sub(end, start), opening_normal).abs() > tolerance {
        return None;
    }
    let mouth_at = geom::dot(start, opening_normal);
    if lips.iter().any(|&e| {
        (geom::dot(samples(&part.edges[e])[1], opening_normal) - mouth_at).abs() > tolerance
    }) {
        return None;
    }
    // The circle's low point opposite the exterior normal fixes which of the two arcs is the
    // concave trough, including exactly semicircular seats.
    let origin = cylinder.origin;
    let centre = geom::add(origin, geom::scale(axis, low - geom::dot(origin, axis)));
    let middle = geom::sub(centre, geom::scale(opening_normal, radius));
    let (arc_centre, arc_radius, arc_normal, arc_length) = three_point_arc(start, middle, end)?;
    if (arc_length - radius * sweep).abs() > tolerance {
        return None;
    }
    let x = geom::unit(geom::sub(start, arc_centre))?;
    let frame = Frame {
        origin: arc_centre,
        x,
        y: geom::cross(arc_normal, x),
        z: arc_normal,
    };
    let section = segment_face(frame, arc_radius, start, end);
    let length = high - low;
    let prism = extrude_face(&section, 0, [0.0; 3], geom::scale(axis, length))?;
    let support: Vec<usize> = (0..prism.faces.len())
        .filter(|&f| matches!(prism.faces[f].surface, Surface::Cylinder { .. }))
        .collect();
    let originals: Vec<(&Part, usize)> = walls.iter().map(|&w| (part, w)).collect();
    if support.len() != 1
        || !covered_patch((&prism, support[0]), &originals)
        || originals
            .iter()
            .any(|&f| !covered_patch(f, &[(&prism, support[0])]))
    {
        return None;
    }
    // `length_tol(radius, rel=1e-4, floor=2e-5)`.
    let thickness = 1e-4 * radius + 2e-5;
    let mouth = [
        start,
        end,
        geom::add(end, geom::scale(axis, length)),
        geom::add(start, geom::scale(axis, length)),
    ];
    let lifted = |by: f64| mouth.map(|p| geom::add(p, geom::scale(opening_normal, by)));
    let slabs = [
        extrude_face(
            &section,
            0,
            geom::scale(axis, -thickness),
            geom::scale(axis, thickness - 1e-6),
        ),
        extrude_face(
            &section,
            0,
            geom::scale(axis, length + 1e-6),
            geom::scale(axis, thickness - 1e-6),
        ),
        ruled_prism(&lifted(1e-6), &lifted(thickness)),
    ];
    if !empty(ctx, owner, Some(&prism)) || slabs.iter().any(|p| !empty(ctx, owner, p.as_ref())) {
        return None;
    }
    // The section face's centroid: on the bisector of the arc, at the circular segment's
    // centroid distance from the arc's centre.
    let angle = arc_length / arc_radius;
    let bisector = geom::add(
        geom::scale(frame.x, (0.5 * angle).cos()),
        geom::scale(frame.y, (0.5 * angle).sin()),
    );
    let distance = 4.0 * arc_radius * (0.5 * angle).sin().powi(3) / (3.0 * (angle - angle.sin()));
    let centroid = geom::add(arc_centre, geom::scale(bisector, distance));
    let frame = LocalFrame::canonical(axis, centroid).ok()?;
    let project = |point: V3| {
        let delta = geom::sub(point, frame.origin);
        [geom::dot(delta, frame.u), geom::dot(delta, frame.v)]
    };
    let (first, last) = (project(start), project(end));
    let (radial_start, radial_end) = (geom::sub(start, centre), geom::sub(end, centre));
    let turn = geom::dot(geom::cross(radial_start, radial_end), axis);
    let mut sign = if turn > 0.0 { 1.0 } else { -1.0 };
    // At a semicircle the cross product vanishes. The midpoint fixes orientation.
    if turn.abs() < radius * radius * 1e-8 {
        let towards = geom::dot(geom::cross(radial_start, geom::sub(middle, centre)), axis);
        sign = if towards > 0.0 { 1.0 } else { -1.0 };
    }
    let boundary = [
        SectionVertex::new(first, sign * (sweep / 4.0).tan()).ok()?,
        SectionVertex::new(last, 0.0).ok()?,
    ];
    Some(CylindricalSeatProof {
        walls: walls.to_vec(),
        context,
        owner,
        frame,
        boundary,
        run_interval: (low, high),
    })
}

/// Every proved seat (`cylindrical_seat_proofs`): the concave native cylinders grouped, from
/// each face in order, with the neighbours on the same surface, and each group proved.
pub fn cylindrical_seat_proofs(ctx: &Context<'_>) -> Vec<CylindricalSeatProof> {
    let part = ctx.part;
    let mut cylinders: Vec<(usize, SeatCylinder)> = Vec::new();
    for node in 0..part.faces.len() {
        let Surface::Cylinder { frame, radius } = part.faces[node].surface else {
            continue;
        };
        if part.frame_points_outward(node) != Some(false) {
            continue;
        }
        let Ok(canonical) = LocalFrame::canonical(frame.z, [0.0; 3]) else {
            continue;
        };
        cylinders.push((
            node,
            SeatCylinder {
                radius,
                axis: canonical.run,
                origin: frame.origin,
            },
        ));
    }
    let of = |node: usize| cylinders.iter().find(|c| c.0 == node).map(|c| c.1);
    let mut remaining: BTreeSet<usize> = cylinders.iter().map(|c| c.0).collect();
    let mut proofs = Vec::new();
    for &(seed, cylinder) in &cylinders {
        if !remaining.remove(&seed) {
            continue;
        }
        let mut walls = BTreeSet::from([seed]);
        let mut pending = vec![seed];
        while let Some(node) = pending.pop() {
            for neighbour in part.neighbours(node) {
                if remaining.contains(&neighbour)
                    && of(neighbour).is_some_and(|other| cylinder.same_surface(&other))
                {
                    remaining.remove(&neighbour);
                    walls.insert(neighbour);
                    pending.push(neighbour);
                }
            }
        }
        let walls: Vec<usize> = walls.into_iter().collect();
        if let Some(proof) = prove(ctx, &walls, &cylinder) {
            proofs.push(proof);
        }
    }
    proofs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_point_arc_reads_a_semicircle() {
        let (centre, radius, normal, length) =
            three_point_arc([1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [-1.0, 0.0, 0.0]).unwrap();
        assert!(geom::norm(centre) < 1e-15);
        assert!((radius - 1.0).abs() < 1e-15);
        assert!((normal[2] + 1.0).abs() < 1e-15);
        assert!((length - PI).abs() < 1e-14);
        assert!(three_point_arc([0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]).is_none());
    }
}
