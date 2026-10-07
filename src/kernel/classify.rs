//! Point-in-solid classification by ray parity — the stand-in for
//! `BRepClass3d_SolidClassifier`.
//!
//! A ray is cast from the point; each crossing of a face (the untrimmed surface hit, kept when
//! the hit lies inside the face's trimming loops) flips inside/outside. A ray that passes too
//! close to an edge or grazes a surface is discarded and another direction tried, so the answer
//! never rests on a knife-edge hit.

use super::brep::Part;
use super::geom::{self, Bounds, Surface, V3};
use super::sampling::CHORD_TOLERANCE;

/// Fixed, deliberately irrational-looking directions: none is parallel to a principal axis or a
/// principal diagonal, which is where modelled geometry concentrates.
const DIRECTIONS: [V3; 7] = [
    [0.5773, 0.6123, 0.5401],
    [-0.4364, 0.8018, 0.4082],
    [0.7071, -0.3015, 0.6396],
    [-0.6247, -0.5217, -0.5812],
    [0.2673, 0.5345, -0.8018],
    [0.9012, 0.1234, -0.4153],
    [-0.1111, -0.9333, 0.3412],
];

/// A point this close to a face is on the boundary.
pub const ON_TOLERANCE: f64 = 1e-6;

/// Precomputed culling boxes for repeated queries against one part.
pub struct Classifier<'a> {
    part: &'a Part,
    /// Per face, a box that certainly contains it (freeform faces use their control points).
    face_boxes: Vec<Bounds>,
    /// Per edge, the box of its samples.
    edge_boxes: Vec<Bounds>,
    reach: f64,
    /// The distance from a hit to a face boundary below which the ray is treated as ambiguous.
    /// It must exceed the edges' polyline error, or a hit near a shared edge could be counted by
    /// both faces or by neither.
    edge_tol: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    In,
    Out,
    /// Within [`ON_TOLERANCE`] of a face (`TopAbs_ON`).
    On,
    /// Every ray was ambiguous, or the part has a surface this classifier cannot intersect.
    Unknown,
}

impl<'a> Classifier<'a> {
    pub fn new(part: &'a Part) -> Self {
        let face_boxes: Vec<Bounds> = (0..part.faces.len())
            .map(|i| match &part.faces[i].surface {
                Surface::Freeform { surface, .. } => {
                    let (min, max) = surface.control_bounds();
                    Bounds { min, max }
                }
                _ => part.face_bounds(i),
            })
            .collect();
        let edge_boxes = part
            .edges
            .iter()
            .map(|e| {
                let mut b = Bounds::empty();
                e.samples.iter().for_each(|p| b.add(*p));
                b
            })
            .collect();
        let mut all = Bounds::empty();
        for b in &face_boxes {
            all.merge(b);
        }
        Classifier {
            part,
            face_boxes,
            edge_boxes,
            reach: all.diagonal() * 2.0 + 1.0,
            edge_tol: (4.0 * CHORD_TOLERANCE).max(all.diagonal() * 1e-9),
        }
    }

    pub fn classify(&self, p: V3) -> State {
        if self.on_boundary(p) {
            return State::On;
        }
        let mut votes = (0usize, 0usize);
        for dir in DIRECTIONS {
            let Some(dir) = geom::unit(dir) else { continue };
            match self.crossings(p, dir) {
                Some(n) if n % 2 == 1 => votes.0 += 1,
                Some(_) => votes.1 += 1,
                None => continue,
            }
            // Two agreeing clean rays settle it.
            if votes.0 >= 2 && votes.1 == 0 {
                return State::In;
            }
            if votes.1 >= 2 && votes.0 == 0 {
                return State::Out;
            }
        }
        match votes.0.cmp(&votes.1) {
            std::cmp::Ordering::Greater => State::In,
            std::cmp::Ordering::Less => State::Out,
            std::cmp::Ordering::Equal => State::Unknown,
        }
    }

    /// Whether *p* lies on a face, within [`ON_TOLERANCE`] (the tolerance the Python
    /// implementation hands `BRepClass3d_SolidClassifier::Perform`).
    fn on_boundary(&self, p: V3) -> bool {
        // On an edge, a face's own containment test is at its boundary and may say no.
        let on_edge = self.part.edges.iter().enumerate().any(|(e, edge)| {
            self.edge_boxes[e].contains(p, self.edge_tol)
                && edge
                    .samples
                    .windows(2)
                    .any(|w| point_segment_distance(p, w[0], w[1]) <= self.edge_tol)
                && geom::dist(edge.curve.value(edge.curve.parameter(p)), p) <= ON_TOLERANCE
        });
        on_edge
            || (0..self.part.faces.len()).any(|i| {
                if !self.face_boxes[i].contains(p, ON_TOLERANCE) {
                    return false;
                }
                let face = &self.part.faces[i];
                let Some((u, v)) = face.surface.parameters(p, None) else {
                    return false;
                };
                geom::dist(face.surface.value(u, v), p) <= ON_TOLERANCE
                    && self.part.domain(i).is_some_and(|d| d.contains(u, v))
            })
    }

    /// Each probe ray's crossing count (`None` when ambiguous), for diagnosing a classification.
    pub fn explain(&self, p: V3) -> Vec<Option<usize>> {
        DIRECTIONS
            .iter()
            .filter_map(|d| geom::unit(*d))
            .map(|d| self.crossings(p, d))
            .collect()
    }

    /// Number of face crossings along the ray, or `None` when the ray is ambiguous.
    fn crossings(&self, p: V3, dir: V3) -> Option<usize> {
        let mut count = 0;
        for (i, face) in self.part.faces.iter().enumerate() {
            if !ray_meets_box(p, dir, &self.face_boxes[i], self.edge_tol) {
                continue;
            }
            let (hits, grazing) = face.surface.ray_hits(p, dir, self.reach)?;
            if grazing && hits.is_empty() {
                return None; // the ray lies in a plane's surface
            }
            for t in hits {
                let q = geom::add(p, geom::scale(dir, t));
                match self.inside_face(i, q) {
                    Some(true) => {
                        if grazing {
                            return None;
                        }
                        count += 1;
                    }
                    Some(false) => {}
                    None => return None,
                }
            }
        }
        Some(count)
    }

    /// Whether a point on face *i*'s surface lies within its trimming loops; `None` when it is
    /// too close to a boundary to say.
    fn inside_face(&self, i: usize, q: V3) -> Option<bool> {
        let part = self.part;
        for lp in &part.faces[i].loops {
            for &(e, _) in &lp.edges {
                if !self.edge_boxes[e].contains(q, self.edge_tol) {
                    continue;
                }
                let s = &part.edges[e].samples;
                for w in s.windows(2) {
                    if point_segment_distance(q, w[0], w[1]) <= self.edge_tol {
                        return None;
                    }
                }
            }
        }
        let domain = part.domain(i)?;
        let (u, v) = part.faces[i].surface.parameters(q, None)?;
        Some(domain.contains(u, v))
    }
}

fn point_segment_distance(p: V3, a: V3, b: V3) -> f64 {
    let ab = geom::sub(b, a);
    let len2 = geom::dot(ab, ab);
    let t = if len2 <= 0.0 {
        0.0
    } else {
        (geom::dot(geom::sub(p, a), ab) / len2).clamp(0.0, 1.0)
    };
    geom::dist(p, geom::add(a, geom::scale(ab, t)))
}

fn ray_meets_box(p: V3, dir: V3, b: &Bounds, pad: f64) -> bool {
    let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
    for i in 0..3 {
        let (lo, hi) = (b.min[i] - pad, b.max[i] + pad);
        if dir[i].abs() < 1e-300 {
            if p[i] < lo || p[i] > hi {
                return false;
            }
            continue;
        }
        let (mut a, mut c) = ((lo - p[i]) / dir[i], (hi - p[i]) / dir[i]);
        if a > c {
            std::mem::swap(&mut a, &mut c);
        }
        t0 = t0.max(a);
        t1 = t1.min(c);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// Is *p* inside the material of *part*? (`_bevel._material_at`: only a clean `IN` counts.)
pub fn material_at(classifier: &Classifier<'_>, p: V3) -> bool {
    classifier.classify(p) == State::In
}
