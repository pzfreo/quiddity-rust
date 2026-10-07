//! Rays against a solid's faces — the stand-in for `IntCurvesFace_ShapeIntersector`: every
//! point where a ray meets a face within its trimming loops, with the face it meets. A hit on a
//! face's boundary counts as on the face (OpenCascade reports `ON` points too), so a ray through
//! an edge meets both faces there.
//!
//! Faces are found through a bounding-volume hierarchy of their boxes, so a ray costs about
//! the logarithm of the face count plus the faces it actually passes near.

use super::brep::Part;
use super::geom::{self, Bounds, Surface, V3};
use super::sampling::CHORD_TOLERANCE;

/// One meeting of a ray with a face, `t` along the (unit) direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f64,
    pub face: usize,
}

/// Where a surface point lies relative to a face: on it (inside or on its boundary), only in
/// the band of an edge that strays from the surface, or off it.
enum On {
    Face,
    Band(usize, f64),
    Off,
}

enum Node {
    Leaf(Vec<usize>),
    Split(Box<Bounds>, Box<Node>, Box<Bounds>, Box<Node>),
}

pub struct RayCaster<'a> {
    part: &'a Part,
    /// Per part face: a box that certainly contains it (freeform faces use their control points).
    face_boxes: Vec<Bounds>,
    edge_boxes: Vec<Bounds>,
    root: Node,
    root_box: Bounds,
    /// A hit this close to a face's boundary is on the boundary. It exceeds the edges' polyline
    /// error, so a hit at a shared edge is never missed by both faces.
    edge_tol: f64,
}

impl<'a> RayCaster<'a> {
    /// Rays against the faces of one solid.
    pub fn for_solid(part: &'a Part, solid: usize) -> Self {
        Self::for_faces(part, part.solids[solid].faces.clone())
    }

    pub fn for_faces(part: &'a Part, faces: Vec<usize>) -> Self {
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
        let mut root_box = Bounds::empty();
        for &f in &faces {
            root_box.merge(&face_boxes[f]);
        }
        let edge_tol = (4.0 * CHORD_TOLERANCE).max(root_box.diagonal() * 1e-9);
        let root = build(faces, &face_boxes);
        RayCaster {
            part,
            face_boxes,
            edge_boxes,
            root,
            root_box,
            edge_tol,
        }
    }

    /// Every hit with `0 < t <= t_max` along the unit direction *dir*, nearest first (equal
    /// distances in face order). `None` when a face the ray reaches has a surface the kernel
    /// cannot intersect.
    pub fn hits(&self, origin: V3, dir: V3, t_max: f64) -> Option<Vec<Hit>> {
        let mut out = Vec::new();
        let mut stack = vec![(&self.root_box, &self.root)];
        while let Some((b, node)) = stack.pop() {
            if !ray_meets_box(origin, dir, t_max, b, self.edge_tol) {
                continue;
            }
            match node {
                Node::Split(lb, l, rb, r) => {
                    stack.push((lb, l));
                    stack.push((rb, r));
                }
                Node::Leaf(faces) => {
                    for &i in faces {
                        if !ray_meets_box(origin, dir, t_max, &self.face_boxes[i], self.edge_tol) {
                            continue;
                        }
                        let (ts, _) = self.part.faces[i].surface.ray_hits(origin, dir, t_max)?;
                        for t in ts {
                            match self.on_face(i, geom::add(origin, geom::scale(dir, t))) {
                                On::Face => out.push((Hit { t, face: i }, None)),
                                On::Band(e, band) => {
                                    out.push((Hit { t, face: i }, Some((e, band))))
                                }
                                On::Off => {}
                            }
                        }
                    }
                }
            }
        }
        out.sort_by(|a, b| a.0.t.total_cmp(&b.0.t).then(a.0.face.cmp(&b.0.face)));
        // A hit found only in a crack's band yields to the face across that edge: the ray
        // crosses the crack once, whichever side of it the surfaces put the crossing.
        let edge_faces = self.part.edge_faces();
        let mut kept: Vec<Hit> = out.iter().filter(|h| h.1.is_none()).map(|h| h.0).collect();
        for (hit, band) in &out {
            let Some((e, band)) = band else { continue };
            let partnered = kept.iter().any(|k| {
                k.face != hit.face
                    && edge_faces[*e].contains(&k.face)
                    && (k.t - hit.t).abs() <= 10.0 * band
            });
            if !partnered {
                kept.push(*hit);
            }
        }
        kept.sort_by(|a, b| a.t.total_cmp(&b.t).then(a.face.cmp(&b.face)));
        Some(kept)
    }

    /// Whether a point on face *i*'s surface lies within its trimming loops or on them. "On"
    /// includes the band in which the file itself leaves the boundary uncertain: an edge that
    /// strays from this face's surface (a B-spline face approximating its neighbour) leaves a
    /// crack as wide as the stray, which OpenCascade covers with the edge tolerance it sets on
    /// import.
    fn on_face(&self, i: usize, q: V3) -> On {
        let part = self.part;
        let mut in_band = None;
        for &(e, deviation) in part.edge_deviation(i) {
            let band = self.edge_tol + deviation;
            if !self.edge_boxes[e].contains(q, band) {
                continue;
            }
            let s = &part.edges[e].samples;
            let d = s
                .windows(2)
                .map(|w| point_segment_distance(q, w[0], w[1]))
                .fold(f64::INFINITY, f64::min);
            if d <= self.edge_tol {
                return On::Face;
            }
            if d <= band && in_band.is_none() {
                in_band = Some((e, band));
            }
        }
        let inside = part.domain(i).is_some_and(|domain| {
            part.faces[i]
                .surface
                .parameters(q, None)
                .is_some_and(|(u, v)| domain.contains(u, v))
        });
        match (inside, in_band) {
            (true, _) => On::Face,
            (false, Some((e, band))) => On::Band(e, band),
            (false, None) => On::Off,
        }
    }
}

impl Part {
    /// Each of the face's edges with the furthest its samples lie from the face's surface.
    pub fn edge_deviation(&self, face: usize) -> &[(usize, f64)] {
        self.cache[face].edge_deviation.get_or_init(|| {
            let surface = &self.faces[face].surface;
            self.face_edges(face)
                .into_iter()
                .map(|e| {
                    let mut hint = None;
                    let worst = self.edges[e].samples.iter().fold(0.0f64, |worst, &p| {
                        let Some(uv) = surface.parameters(p, hint) else {
                            return worst;
                        };
                        hint = Some(uv);
                        worst.max(geom::dist(surface.value(uv.0, uv.1), p))
                    });
                    (e, worst)
                })
                .collect()
        })
    }

    /// `Face.position_at(u, v)`: the surface point at fractions (u, v) of the face's
    /// `BRepTools::UVBounds`, with its parameters.
    pub fn position_at(&self, face: usize, u: f64, v: f64) -> Option<(V3, (f64, f64))> {
        let (u0, u1, v0, v1) = self.uv_bounds(face)?;
        let (pu, pv) = (u0 + u * (u1 - u0), v0 + v * (v1 - v0));
        Some((self.faces[face].surface.value(pu, pv), (pu, pv)))
    }

    /// `Face.normal_at(point)`: the face's outward normal at the surface point nearest *p*;
    /// `None` where the surface has no normal (a pole or an apex).
    pub fn normal_at_point(&self, face: usize, p: V3) -> Option<V3> {
        let (u, v) = self.faces[face].surface.parameters(p, None)?;
        self.face_normal(face, u, v)
    }
}

const LEAF: usize = 4;

fn build(mut faces: Vec<usize>, boxes: &[Bounds]) -> Node {
    if faces.len() <= LEAF {
        return Node::Leaf(faces);
    }
    let mut all = Bounds::empty();
    for &f in &faces {
        all.merge(&boxes[f]);
    }
    let extent = geom::sub(all.max, all.min);
    let axis = (0..3)
        .max_by(|&a, &b| extent[a].total_cmp(&extent[b]))
        .unwrap();
    let centre = |f: usize| boxes[f].min[axis] + boxes[f].max[axis];
    faces.sort_by(|&a, &b| centre(a).total_cmp(&centre(b)).then(a.cmp(&b)));
    let right = faces.split_off(faces.len() / 2);
    let span = |fs: &[usize]| {
        let mut b = Bounds::empty();
        fs.iter().for_each(|&f| b.merge(&boxes[f]));
        b
    };
    let (lb, rb) = (span(&faces), span(&right));
    Node::Split(
        Box::new(lb),
        Box::new(build(faces, boxes)),
        Box::new(rb),
        Box::new(build(right, boxes)),
    )
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

fn ray_meets_box(p: V3, dir: V3, t_max: f64, b: &Bounds, pad: f64) -> bool {
    let (mut t0, mut t1) = (0.0f64, t_max + pad);
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
