//! Point-in-solid classification by ray parity — the stand-in for
//! `BRepClass3d_SolidClassifier`.
//!
//! A ray is cast from the point; each crossing of a face (the untrimmed surface hit, kept when
//! the hit lies inside the face's trimming loops) flips inside/outside. A ray that passes too
//! close to an edge or grazes a surface is discarded and another direction tried, so the answer
//! never rests on a knife-edge hit.

use crate::brep::{Bounds, Part};
use crate::geom::{self, V3};

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

/// Precomputed per-face trimming data for repeated queries against one part.
pub struct Classifier<'a> {
    part: &'a Part,
    bounds: Vec<Bounds>,
    reach: f64,
    /// The distance from a hit to a face boundary below which the ray is treated as ambiguous.
    edge_tol: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    In,
    Out,
    /// Every ray was ambiguous, or the part has a surface this classifier cannot intersect.
    Unknown,
}

impl<'a> Classifier<'a> {
    pub fn new(part: &'a Part) -> Self {
        let bounds: Vec<Bounds> = (0..part.faces.len()).map(|i| part.face_bounds(i)).collect();
        let mut all = Bounds::empty();
        for b in &bounds {
            all.merge(b);
        }
        let reach = all.diagonal() * 2.0 + 1.0;
        Classifier {
            part,
            bounds,
            reach,
            edge_tol: (all.diagonal() * 1e-9).max(1e-7),
        }
    }

    pub fn classify(&self, p: V3) -> State {
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

    /// Number of face crossings along the ray, or `None` when the ray is ambiguous.
    fn crossings(&self, p: V3, dir: V3) -> Option<usize> {
        let mut count = 0;
        for (i, face) in self.part.faces.iter().enumerate() {
            if !ray_meets_box(p, dir, &self.bounds[i], self.edge_tol * 10.0) {
                continue;
            }
            let (hits, grazing) = face.surface.ray_hits(p, dir, self.reach)?;
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
