//! Oblique through-step recognition (`quiddity.oblique_through_steps`): an open step whose run
//! is oblique in the principal frame, cut by one axis-aligned principal wall and one rectangular
//! oblique wall perpendicular to it. Their concave seam reaches the solid's envelope at both
//! ends with convex terminals there, and the volume swept off the principal wall is empty.

use serde::Serialize;

use super::Context;
use super::body::BodyKey;
use super::evidence::{self, EvidenceError, Occurrence};
use super::planes::{axis_aligned_axis, coordinates, linear_quad, normalized, plane_normal};
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{
    Bounds, COORD_FLOOR, Curve, SMOOTH_ARC_GAP, Surface, V3, add, dot, length_tol, norm, scale, sub,
};
use crate::kernel::py;

const END_EPS: f64 = COORD_FLOOR;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ObliqueThroughStep {
    pub run: V3,
    pub length: f64,
    pub at: V3,
    pub depth_direction: V3,
    pub depth: f64,
    pub across_direction: V3,
    pub wall_outline: [[f64; 2]; 4],
    pub body_key: Option<BodyKey>,
}

impl ObliqueThroughStep {
    /// The fields in the order Python's `__lt__` compares them.
    fn sort_key(&self) -> Vec<f64> {
        let mut key: Vec<f64> = self.run.to_vec();
        key.push(self.length);
        key.extend(self.at);
        key.extend(self.depth_direction);
        key.push(self.depth);
        key.extend(self.across_direction);
        key.extend(self.wall_outline.iter().flatten());
        key.push(if self.body_key.is_some() { 1.0 } else { 0.0 });
        key.extend(self.body_key.iter().flatten());
        key
    }
}

/// `recognise_oblique_through_steps`.
pub fn recognise_oblique_through_steps(part: &Part) -> Vec<ObliqueThroughStep> {
    super::records(discover(&Context::new(part)))
}

/// The evidence path: each step with its principal and oblique walls.
pub fn discover_verified(
    ctx: &Context<'_>,
) -> Result<Vec<Occurrence<ObliqueThroughStep>>, EvidenceError> {
    evidence::verified(ctx.part, discover(ctx))
}

/// The face's distinct vertex points (`face.vertices()`).
fn face_vertices(part: &Part, face: usize) -> Vec<V3> {
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for e in part.face_edges(face) {
        let edge = &part.edges[e];
        for (v, p) in [(edge.vertices.0, edge.start), (edge.vertices.1, edge.end)] {
            if !seen.contains(&v) {
                seen.push(v);
                out.push(p);
            }
        }
    }
    out
}

/// The outer loop's vertices in boundary order (`outer_wire().vertices()`).
fn outer_vertices(part: &Part, face: usize) -> Vec<V3> {
    let Some(outer) = part.outer_loop(face) else {
        return Vec::new();
    };
    let mut seen = Vec::new();
    let mut out = Vec::new();
    for &(e, forward) in &part.faces[face].loops[outer].edges {
        let edge = &part.edges[e];
        let (v, p) = if forward {
            (edge.vertices.0, edge.start)
        } else {
            (edge.vertices.1, edge.end)
        };
        if !seen.contains(&v) {
            seen.push(v);
            out.push(p);
        }
    }
    out
}

/// `_canonical_outline`: the least rotation of the outline or its reverse, without -0.0.
fn canonical_outline(vertices: &[[f64; 2]]) -> [[f64; 2]; 4] {
    let normalized: Vec<[f64; 2]> = vertices
        .iter()
        .map(|p| p.map(py::without_negative_zero))
        .collect();
    let mut reversed = normalized.clone();
    reversed.reverse();
    let mut best: Option<[[f64; 2]; 4]> = None;
    for path in [&normalized, &reversed] {
        for i in 0..4 {
            let candidate: [[f64; 2]; 4] = std::array::from_fn(|k| path[(i + k) % 4]);
            let flat = |c: &[[f64; 2]; 4]| c.iter().flatten().copied().collect::<Vec<f64>>();
            if best
                .as_ref()
                .is_none_or(|b| py::tuple_order(&flat(&candidate), &flat(b)).is_lt())
            {
                best = Some(candidate);
            }
        }
    }
    best.expect("four rotations")
}

fn endpoint_on_envelope(point: V3, bounds: &Bounds, principal_axis: usize) -> bool {
    (0..3).filter(|&a| a != principal_axis).any(|a| {
        [bounds.min[a], bounds.max[a]]
            .iter()
            .any(|b| (point[a] - b).abs() <= END_EPS)
    })
}

/// Whether two faces share an edge with a vertex at *endpoint*.
fn meet_at(part: &Part, a: usize, b: usize, endpoint: V3) -> bool {
    part.shared_edges(a, b).iter().any(|&e| {
        part.edges[e]
            .vertex_points()
            .iter()
            .any(|p| norm(sub(*p, endpoint)) <= END_EPS)
    })
}

/// `_terminal`: convex faces closing both walls at a seam end, one face or two convex-joined.
fn terminal(part: &Part, principal: usize, oblique: usize, endpoint: V3, solid: &[usize]) -> bool {
    let convex_terminals = |source: usize| -> Vec<usize> {
        part.neighbours(source)
            .into_iter()
            .filter(|&n| {
                solid.contains(&n)
                    && n != principal
                    && n != oblique
                    && part.arc(source, n) == Some(Arc::Convex)
                    && meet_at(part, source, n, endpoint)
            })
            .collect()
    };
    let rights = convex_terminals(oblique);
    convex_terminals(principal).into_iter().any(|left| {
        rights.iter().any(|&right| {
            left == right
                || (part.arc(left, right) == Some(Arc::Convex)
                    && meet_at(part, left, right, endpoint))
        })
    })
}

/// The record for one principal wall and its oblique neighbour, or `None` (`_one_pair`).
fn one_pair(
    ctx: &Context<'_>,
    s: usize,
    principal: usize,
    oblique: usize,
    bounds: &Bounds,
    solid: &[usize],
) -> Option<ObliqueThroughStep> {
    let part = ctx.part;
    if part.arc(principal, oblique) != Some(Arc::Concave) {
        return None;
    }
    let (axis, coordinate) = axis_aligned_axis(part, principal)?;
    if !matches!(part.faces[oblique].surface, Surface::Plane { .. })
        || axis_aligned_axis(part, oblique).is_some()
    {
        return None;
    }
    if !linear_quad(part, principal) || !linear_quad(part, oblique) {
        return None;
    }
    let principal_normal = plane_normal(part, principal)?;
    let oblique_normal = plane_normal(part, oblique)?;
    if dot(principal_normal, oblique_normal).abs() > SMOOTH_ARC_GAP {
        return None;
    }
    let shared = part.shared_edges(principal, oblique);
    let [seam] = shared[..] else { return None };
    if !matches!(part.edges[seam].curve, Curve::Line { .. }) {
        return None;
    }
    let mut ends = part.edges[seam].vertex_points();
    if ends.len() != 2 {
        return None;
    }
    ends.sort_by(|a, b| py::tuple_order(a, b));
    let (start, end) = (ends[0], ends[1]);
    let run_length = norm(sub(end, start));
    if run_length <= COORD_FLOOR {
        return None;
    }
    let run = normalized(sub(end, start));
    if dot(run, principal_normal).abs() > SMOOTH_ARC_GAP
        || dot(run, oblique_normal).abs() > SMOOTH_ARC_GAP
    {
        return None;
    }
    if !endpoint_on_envelope(start, bounds, axis) || !endpoint_on_envelope(end, bounds, axis) {
        return None;
    }
    if !terminal(part, principal, oblique, start, solid)
        || !terminal(part, principal, oblique, end, solid)
    {
        return None;
    }
    let all_convex = |node: usize, other: usize| {
        part.neighbours(node)
            .into_iter()
            .filter(|&n| n != other)
            .all(|n| part.arc(node, n) == Some(Arc::Convex))
    };
    if !all_convex(principal, oblique) || !all_convex(oblique, principal) {
        return None;
    }
    // The oblique wall is the far side of the sweep: each seam end on the principal wall and
    // again at one common positive depth.
    let oblique_vertices = face_vertices(part, oblique);
    let mut depths: Vec<f64> = oblique_vertices
        .iter()
        .map(|v| py::round_to(dot(sub(*v, start), principal_normal), 6))
        .collect();
    depths.sort_by(|a, b| py::order(*a, *b));
    depths.dedup();
    let [near, depth] = depths[..] else {
        return None;
    };
    if near.abs() > END_EPS || depth <= COORD_FLOOR {
        return None;
    }
    let far = coordinate + depth * principal_normal[axis];
    if (far - bounds.min[axis]).abs() > END_EPS && (far - bounds.max[axis]).abs() > END_EPS {
        return None;
    }
    for v in &oblique_vertices {
        let offset = sub(*v, start);
        if dot(offset, oblique_normal).abs() > END_EPS {
            return None;
        }
        let along = dot(offset, run);
        if along.abs().min((along - run_length).abs()) > END_EPS {
            return None;
        }
        let deep = dot(offset, principal_normal);
        if deep.abs().min((deep - depth).abs()) > END_EPS {
            return None;
        }
    }
    let principal_vertices = outer_vertices(part, principal);
    if principal_vertices.len() != 4 {
        return None;
    }
    let outer: Vec<V3> = principal_vertices
        .iter()
        .copied()
        .filter(|v| norm(sub(*v, start)).min(norm(sub(*v, end))) > END_EPS)
        .collect();
    let [o0, o1] = outer[..] else { return None };
    let outer_edge = sub(o1, o0);
    // A skew connecting edge closes a triangular blind-step wedge; a through step's opposite
    // wall boundary is a principal stock edge.
    if norm(outer_edge) <= COORD_FLOOR
        || !(0..3)
            .filter(|&c| c != axis)
            .any(|c| 1.0 - normalized(outer_edge)[c].abs() <= SMOOTH_ARC_GAP)
    {
        return None;
    }
    let across_values: Vec<f64> = principal_vertices
        .iter()
        .map(|v| dot(sub(*v, start), oblique_normal))
        .collect();
    let lo = across_values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = across_values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    if lo < -END_EPS && hi > END_EPS {
        return None;
    }
    let across = if hi > END_EPS {
        oblique_normal
    } else {
        scale(oblique_normal, -1.0)
    };
    let outline: Vec<[f64; 2]> = principal_vertices
        .iter()
        .map(|v| {
            let o = sub(*v, start);
            [py::round_to3(dot(o, run)), py::round_to3(dot(o, across))]
        })
        .collect();
    if outline.iter().filter(|p| p[1] > END_EPS).count() != 2 {
        return None;
    }
    // The wall swept through the depth, inset only along it so that boundary contact is not
    // counted as material.
    let inset = length_tol(depth, 1e-6).min(depth / 4.0);
    let volume = ctx.swept_face_volume(
        s,
        principal,
        scale(principal_normal, inset),
        scale(principal_normal, depth - 2.0 * inset),
    )?;
    if volume != 0.0 {
        return None;
    }
    Some(ObliqueThroughStep {
        run: coordinates(run, 6),
        length: py::round_to3(run_length),
        at: coordinates(scale(add(start, end), 0.5), 3),
        depth_direction: coordinates(principal_normal, 6),
        depth: py::round_to3(depth),
        across_direction: coordinates(across, 6),
        wall_outline: canonical_outline(&outline),
        body_key: None, // filled in by `discover`
    })
}

pub fn discover(ctx: &Context<'_>) -> Vec<Occurrence<ObliqueThroughStep>> {
    let part = ctx.part;
    let mut found = Vec::new();
    for (s, solid) in part.solids.iter().enumerate() {
        let bounds = part.solid_bounds(s);
        let mut faces = solid.faces.clone();
        faces.sort_unstable();
        for principal in faces {
            if axis_aligned_axis(part, principal).is_none() {
                continue;
            }
            for oblique in part.neighbours(principal) {
                if !solid.faces.contains(&oblique) {
                    continue;
                }
                if let Some(record) = one_pair(ctx, s, principal, oblique, &bounds, &solid.faces) {
                    found.push((s, record, principal, oblique));
                }
            }
        }
    }
    // Body keys need every solid's exact mass, so they are only computed for parts with records.
    let keys = if found.is_empty() {
        vec![]
    } else {
        ctx.body_keys(true)
    };
    let mut out: Vec<_> = found
        .into_iter()
        .map(|(s, record, principal, oblique)| Occurrence {
            record: ObliqueThroughStep {
                body_key: keys[s].clone(),
                ..record
            },
            defining: vec![principal, oblique],
            context: vec![],
        })
        .collect();
    out.sort_by(|a, b| py::tuple_order(&a.record.sort_key(), &b.record.sort_key()));
    out
}
