//! Rays against a solid's faces — the stand-in for `IntCurvesFace_ShapeIntersector`: every
//! point where a ray meets a face within its trimming loops, with the face it meets. A hit on a
//! face's boundary counts as on the face (OpenCascade reports `ON` points too), so a ray through
//! an edge meets both faces there.
//!
//! Faces are found through a bounding-volume hierarchy of their boxes, so a ray costs about
//! the logarithm of the face count plus the faces it actually passes near.

use super::brep::{Edge, Part};
use super::geom::{self, Bounds, COORD_FLOOR, Surface, V3};
use super::sampling::{CHORD_TOLERANCE, edge_interval};

/// One meeting of a ray with a face, `t` along the (unit) direction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub t: f64,
    pub face: usize,
}

/// How a ray's crossing meets its face: inside its trim, within the polyline tolerance of an
/// edge (on the boundary, where a neighbour may claim the same crossing), or only in the band
/// of an edge that strays from the surface.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Contact {
    Interior,
    /// Within the tolerance of an edge; *inside* says whether the trim itself contains it.
    Edge {
        inside: bool,
    },
    Band(usize, f64),
}

/// A crossing with how it meets its face and whether the ray touches the surface there
/// tangentially (and so may not cross it at all).
struct Crossing {
    hit: Hit,
    contact: Contact,
    tangent: bool,
}

enum Node {
    Leaf(Vec<usize>),
    Split(Box<Bounds>, Box<Node>, Box<Bounds>, Box<Node>),
}

pub struct RayCaster<'a> {
    pub(super) part: &'a Part,
    /// The faces rays meet.
    pub(super) faces: Vec<usize>,
    /// Per part face: a box that certainly contains it (freeform faces use their control points).
    pub(super) face_boxes: Vec<Bounds>,
    pub(super) edge_boxes: Vec<Bounds>,
    root: Node,
    pub(super) root_box: Bounds,
    /// A hit this close to a face's boundary is on the boundary. It exceeds the edges' polyline
    /// error, so a hit at a shared edge is never missed by both faces.
    pub(super) edge_tol: f64,
}

impl<'a> RayCaster<'a> {
    /// Rays against the faces of one solid.
    pub fn for_solid(part: &'a Part, solid: usize) -> Self {
        Self::for_faces(part, part.solids[solid].faces.clone())
    }

    pub fn for_faces(part: &'a Part, faces: Vec<usize>) -> Self {
        let face_boxes: Vec<Bounds> = (0..part.faces.len())
            .map(|i| match &part.faces[i].surface {
                // A freeform face's box is sampled, so padded to be sure of it: its control
                // net's box is certain but can be far larger (a helix's spans its cylinder).
                Surface::Freeform { surface, .. } => {
                    let b = part.face_bounds(i);
                    let pad = 0.02 * b.diagonal() + 1e-3;
                    let (min, max) = surface.control_bounds();
                    Bounds {
                        min: [0, 1, 2].map(|k| (b.min[k] - pad).max(min[k])),
                        max: [0, 1, 2].map(|k| (b.max[k] + pad).min(max[k])),
                    }
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
        let root = build(faces.clone(), &face_boxes);
        RayCaster {
            part,
            faces,
            face_boxes,
            edge_boxes,
            root,
            root_box,
            edge_tol,
        }
    }

    /// The box of the faces rays meet.
    pub fn bounds(&self) -> Bounds {
        self.root_box
    }

    /// Whether any face's box shares volume with *b* (boxes that only touch do not).
    pub fn any_face_box_meets(&self, b: &Bounds) -> bool {
        self.faces.iter().any(|&f| {
            let fb = &self.face_boxes[f];
            (0..3).all(|i| fb.min[i] < b.max[i] && b.min[i] < fb.max[i])
        })
    }

    /// Every hit with `0 < t <= t_max` along the unit direction *dir*, nearest first (equal
    /// distances in face order). `None` when a face the ray reaches has a surface the kernel
    /// cannot intersect.
    pub fn hits(&self, origin: V3, dir: V3, t_max: f64) -> Option<Vec<Hit>> {
        let (crossings, _) = self.crossings(origin, dir, t_max)?;
        Some(crossings.into_iter().map(|c| c.hit).collect())
    }

    /// Whether the ray meets any face beyond `t_min` (and within `t_max`): the question
    /// visibility asks, answered at the first such face. `None` when no face is met but one the
    /// ray reaches has a surface the kernel cannot intersect (as [`Self::hits`]).
    pub fn any_hit(&self, origin: V3, dir: V3, t_min: f64, t_max: f64) -> Option<bool> {
        let mut unanswered = false;
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
                        let Some((ts, _)) = self.part.faces[i].surface.ray_hits(origin, dir, t_max)
                        else {
                            unanswered = true;
                            continue;
                        };
                        let met = ts.into_iter().filter(|&t| t > t_min).any(|t| {
                            self.contact(i, geom::add(origin, geom::scale(dir, t)))
                                .is_some()
                        });
                        if met {
                            return Some(true);
                        }
                    }
                }
            }
        }
        (!unanswered).then_some(false)
    }

    /// The hits that lie on their faces' trimmed regions themselves, not merely within the
    /// tolerance of an edge: the crossings that bound material along a line. A line passing an
    /// edge at a distance sees one face or the other there, never both and never neither.
    pub fn trimmed_hits(&self, origin: V3, dir: V3, t_max: f64) -> Option<Vec<Hit>> {
        let (crossings, _) = self.crossings(origin, dir, t_max)?;
        let on_trim = |c: &Crossing| !matches!(c.contact, Contact::Edge { inside: false });
        Some(
            crossings
                .into_iter()
                .filter(on_trim)
                .map(|c| c.hit)
                .collect(),
        )
    }

    /// How many times the ray crosses the faces, for parity; `None` when that cannot be
    /// trusted: the ray runs along a surface, touches one tangentially, passes through an
    /// edge (where its neighbours may both claim, or both miss, the crossing), or crosses two
    /// faces at one point (faces lying on each other, as where two solids touch).
    pub fn crossing_count(&self, origin: V3, dir: V3, t_max: f64) -> Option<usize> {
        let (crossings, in_surface) = self.crossings(origin, dir, t_max)?;
        let clean = !in_surface
            && crossings
                .iter()
                .all(|c| !c.tangent && !matches!(c.contact, Contact::Edge { .. }))
            && crossings
                .windows(2)
                .all(|w| w[1].hit.t - w[0].hit.t > self.edge_tol);
        clean.then_some(crossings.len())
    }

    /// The crossings along a ray, nearest first, with whether the ray lies in a surface. A
    /// crossing found only in a crack's band yields to the face across that edge: the ray
    /// crosses the crack once, whichever side of it the surfaces put the crossing.
    fn crossings(&self, origin: V3, dir: V3, t_max: f64) -> Option<(Vec<Crossing>, bool)> {
        let mut out = Vec::new();
        let mut in_surface = false;
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
                        let (ts, tangent) =
                            self.part.faces[i].surface.ray_hits(origin, dir, t_max)?;
                        in_surface |= tangent && ts.is_empty();
                        for t in ts {
                            let q = geom::add(origin, geom::scale(dir, t));
                            if let Some(contact) = self.contact(i, q) {
                                let hit = Hit { t, face: i };
                                out.push(Crossing {
                                    hit,
                                    contact,
                                    tangent,
                                });
                            }
                        }
                    }
                }
            }
        }
        let order = |a: &Hit, b: &Hit| a.t.total_cmp(&b.t).then(a.face.cmp(&b.face));
        out.sort_by(|a, b| order(&a.hit, &b.hit));
        let edge_faces = self.part.edge_faces();
        let (mut kept, banded): (Vec<Crossing>, Vec<Crossing>) = out
            .into_iter()
            .partition(|c| !matches!(c.contact, Contact::Band(..)));
        for c in banded {
            let Contact::Band(e, band) = c.contact else {
                continue;
            };
            let partnered = kept.iter().any(|k| {
                k.hit.face != c.hit.face
                    && edge_faces[e].contains(&k.hit.face)
                    && (k.hit.t - c.hit.t).abs() <= 10.0 * band
            });
            if !partnered {
                kept.push(c);
            }
        }
        kept.sort_by(|a, b| order(&a.hit, &b.hit));
        Some((kept, in_surface))
    }

    /// How a point on face *i*'s surface meets the face, if it does. The band is where the file
    /// itself leaves the boundary uncertain: an edge that strays from this face's surface (a
    /// B-spline face approximating its neighbour) leaves a crack as wide as the stray, which
    /// OpenCascade covers with the edge tolerance it sets on import.
    ///
    /// The trim is tested on polylines, which stray from a curved edge by up to the chord
    /// tolerance, so a point that close to an edge could be put on either side of it. On an
    /// analytic surface the side of each edge near is taken from its curve instead, so the two
    /// faces of an edge decide by the same exact curve and a crossing next to it, or through
    /// it, is claimed by exactly the faces the line crosses. A point nearest an edge's end lies
    /// outside that edge's sliver (chord and arc meet there), and an edge off the surface (a
    /// band) has no side on it to read, so neither changes the answer; nor does anything on a
    /// B-spline face, whose surface cannot be followed past its edges.
    fn contact(&self, i: usize, q: V3) -> Option<Contact> {
        let part = self.part;
        let surface = &part.faces[i].surface;
        let exact = !matches!(surface, Surface::Freeform { .. });
        let domain = part.domain(i);
        let uv = surface.parameters(q, None);
        let mut inside = domain.is_some_and(|d| uv.is_some_and(|(u, v)| d.contains(u, v)));
        let (mut on_edge, mut in_band) = (false, None);
        // Each edge whose curve passes within reach of the point between its ends: the edge,
        // its nearest point and the reach.
        let mut near: Vec<(usize, V3, f64)> = Vec::new();
        for &(e, deviation) in part.edge_deviation(i) {
            let reach = self.edge_tol + deviation;
            let edge = &part.edges[e];
            if !self.edge_boxes[e].contains(q, reach) || polyline_distance(q, &edge.samples) > reach
            {
                continue;
            }
            // The polyline only finds the edges near; the distance that decides is the curve's,
            // at the tolerance OpenCascade's intersector is loaded with.
            let (c, between_ends) = nearest_on_edge(edge, q);
            let d = geom::dist(c, q);
            on_edge |= d <= COORD_FLOOR;
            if d <= COORD_FLOOR + deviation && in_band.is_none() {
                in_band = Some(Contact::Band(e, COORD_FLOOR + deviation));
            }
            // Nearer an end than the point is to the curve, the sliver is too thin to hold it.
            let clear_of_ends = between_ends
                && (edge.is_closed() || geom::dist(c, edge.start).min(geom::dist(c, edge.end)) > d);
            if exact && d <= reach && clear_of_ends && deviation <= COORD_FLOOR {
                near.push((e, c, reach));
            }
        }
        if let Some(uv) = uv {
            for &(e, c, reach) in &near {
                if misplaces(surface, q, uv, &part.edges[e].samples, c, reach) {
                    inside = !inside;
                }
            }
        }
        if on_edge {
            return Some(Contact::Edge { inside });
        }
        if inside {
            Some(Contact::Interior)
        } else {
            in_band
        }
    }
}

/// The point of an edge nearest *q*, and whether it lies between the edge's ends (else it is
/// the nearer end).
fn nearest_on_edge(edge: &Edge, q: V3) -> (V3, bool) {
    let t = edge.curve.parameter(q);
    if edge.is_closed() {
        return (edge.curve.value(t), true);
    }
    let (a, b) = edge_interval(&edge.curve, edge.start, edge.end, edge.same_sense, false);
    let (lo, hi) = (a.min(b), a.max(b));
    let t = edge
        .curve
        .period()
        .map_or(t, |period| lo + (t - lo).rem_euclid(period));
    if (lo..=hi).contains(&t) {
        (edge.curve.value(t), true)
    } else if geom::dist(edge.start, q) <= geom::dist(edge.end, q) {
        (edge.start, false)
    } else {
        (edge.end, false)
    }
}

/// Whether an edge's polyline, in *surface*'s parameters as the trim test sees it, puts the
/// point *q* (at parameters *uv*) on the other side of the edge from where its curve does: the
/// way from *q* through *c*, the curve's nearest point, to *beyond* past it crosses the curve
/// once, and must cross the polyline an odd number of times too. Only segments within the
/// length of that way of *q* can cross it.
fn misplaces(surface: &Surface, q: V3, uv: (f64, f64), samples: &[V3], c: V3, beyond: f64) -> bool {
    let (pu, pv) = surface.periodic();
    let at = |p: V3| {
        surface.parameters(p, Some(uv)).map(|(u, v)| {
            (
                if pu { geom::nearest_turn(u, uv.0) } else { u },
                if pv { geom::nearest_turn(v, uv.1) } else { v },
            )
        })
    };
    let d = geom::dist(q, c);
    // On the curve to within rounding there is no side to read.
    if d <= 1e-10 {
        return false;
    }
    let end = geom::add(c, geom::scale(geom::sub(c, q), beyond / d));
    let Some(e2) = at(end) else {
        return false;
    };
    let way = (e2.0 - uv.0, e2.1 - uv.1);
    let side = |p: (f64, f64)| way.0 * (p.1 - uv.1) - way.1 * (p.0 - uv.0);
    let along = |p: (f64, f64)| way.0 * (p.0 - uv.0) + way.1 * (p.1 - uv.1);
    let mut crossings = 0;
    for w in samples.windows(2) {
        if point_segment_distance(q, w[0], w[1]) > d + beyond {
            continue;
        }
        let (Some(a), Some(b)) = (at(w[0]), at(w[1])) else {
            continue;
        };
        let (sa, sb) = (side(a), side(b));
        if (sa > 0.0) == (sb > 0.0) {
            continue;
        }
        // Where the segment meets the line of the way, as a fraction of it.
        let k = sa / (sa - sb);
        let x = (a.0 + k * (b.0 - a.0), a.1 + k * (b.1 - a.1));
        let s = along(x) / (way.0 * way.0 + way.1 * way.1);
        if s > 0.0 && s < 1.0 {
            crossings += 1;
        }
    }
    crossings % 2 == 0
}

/// The distance from *p* to a polyline.
pub(super) fn polyline_distance(p: V3, samples: &[V3]) -> f64 {
    samples
        .windows(2)
        .map(|w| point_segment_distance(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_hit_is_unknown_where_only_an_unintersectable_face_is_reached() {
        // A 30 × 30 × 20 block about the origin, bored through along z.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rejected_bored_box.step");
        let mut part = crate::read_step_file(&path).unwrap();
        let (origin, dir) = ([20.0, 10.0, 0.0], [-1.0, 0.0, 0.0]);
        let all: Vec<usize> = (0..part.faces.len()).collect();
        let side = RayCaster::for_faces(&part, all.clone())
            .hits(origin, dir, 100.0)
            .unwrap()[0];
        assert!((side.t - 5.0).abs() < 1e-9, "the x = 15 side first");
        part.faces[side.face].surface = Surface::Other { kind: "TEST" };
        let rays = RayCaster::for_faces(&part, all);
        // Only the unanswerable side lies within reach; beyond it, the far side is met.
        assert_eq!(rays.any_hit(origin, dir, 0.0, 10.0), None);
        assert_eq!(rays.any_hit(origin, dir, 0.0, 100.0), Some(true));
        assert_eq!(rays.hits(origin, dir, 10.0), None);
        // A ray that reaches no face at all is a clean miss.
        assert_eq!(
            rays.any_hit(origin, [1.0, 0.0, 0.0], 0.0, 100.0),
            Some(false)
        );
    }

    /// The bored box (a 30 × 30 × 20 block about the origin, a radius-5 bore along z) and its
    /// rim at z = 10: the bore's centre there, a unit direction across the bore, and the widest
    /// sliver the rim's polyline leaves (chord to arc), in that direction.
    fn bore_rim() -> (Part, V3, V3, f64) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/rejected_bored_box.step");
        let part = crate::read_step_file(&path).unwrap();
        let edge = part
            .edges
            .iter()
            .find(|e| {
                matches!(e.curve, geom::Curve::Circle { .. }) && (e.start[2] - 10.0).abs() < 1e-9
            })
            .unwrap();
        let geom::Curve::Circle { frame, radius } = edge.curve else {
            unreachable!()
        };
        let chord = edge
            .samples
            .windows(2)
            .map(|w| geom::scale(geom::add(w[0], w[1]), 0.5))
            .min_by(|a, b| geom::dist(*a, frame.origin).total_cmp(&geom::dist(*b, frame.origin)))
            .unwrap();
        let across = geom::unit(geom::sub(chord, frame.origin)).unwrap();
        (
            part,
            frame.origin,
            across,
            radius - geom::dist(chord, frame.origin),
        )
    }

    #[test]
    fn a_crossing_beside_a_curved_edge_is_claimed_by_the_face_its_curve_puts_it_on() {
        let (part, centre, across, sliver) = bore_rim();
        assert!(
            sliver > 10.0 * COORD_FLOOR,
            "the polyline leaves a sliver: {sliver}"
        );
        let rays = RayCaster::for_solid(&part, 0);
        let at = |r: f64| geom::add(centre, geom::scale(across, 5.0 + r));
        let through = |q: V3, d: V3| (geom::sub(q, geom::scale(d, 40.0)), d);
        // Down and outwards through the sliver, inside the arc: the ray enters the bore's
        // opening (air) and crosses into the wall, so the top face must not claim it, though
        // its polyline puts the point on it. Then out through the bottom: two crossings.
        let down_out = geom::unit(geom::add(geom::scale(across, 0.3), [0.0, 0.0, -1.0])).unwrap();
        let (o, d) = through(at(-0.5 * sliver), down_out);
        assert_eq!(rays.crossing_count(o, d, 80.0), Some(2));
        let hits = rays.trimmed_hits(o, d, 80.0).unwrap();
        assert_eq!(hits.len(), 2);
        assert!(matches!(
            part.faces[hits[0].face].surface,
            Surface::Cylinder { .. }
        ));
        // Just outside the arc the top face claims it, and the wall takes the ray back into the
        // bore just below: two crossings again.
        let down_in = geom::unit(geom::add(geom::scale(across, -0.02), [0.0, 0.0, -1.0])).unwrap();
        let (o, d) = through(at(0.5 * sliver), down_in);
        assert_eq!(rays.crossing_count(o, d, 80.0), Some(2));
        let first = rays.hits(o, d, 80.0).unwrap()[0].face;
        assert!(matches!(part.faces[first].surface, Surface::Plane { .. }));
        // Through the edge itself (within the coordinate floor): an even count or none, never
        // an odd one, whichever side of the arc the ray passes.
        for r in [0.0, 0.5 * COORD_FLOOR, -0.5 * COORD_FLOOR] {
            for d in [down_out, down_in] {
                let (o, d) = through(at(r), d);
                if let Some(n) = rays.crossing_count(o, d, 80.0) {
                    assert_eq!(n % 2, 0, "{n} crossings through the edge at {r}");
                }
            }
        }
    }

    #[test]
    fn edge_and_tangent_contacts_on_analytic_faces() {
        let (part, centre, across, sliver) = bore_rim();
        let rays = RayCaster::for_solid(&part, 0);
        let at = |r: f64| geom::add(centre, geom::scale(across, 5.0 + r));
        // Up the bore and out through its opening inside the arc, in the sliver: nothing is
        // met, though the top face's polyline takes in the point.
        let up = geom::unit(geom::add(geom::scale(across, 0.01), [0.0, 0.0, 1.0])).unwrap();
        let o = geom::sub(at(-0.5 * sliver), geom::scale(up, 5.0));
        assert_eq!(rays.any_hit(o, up, 0.0, 20.0), Some(false));
        assert!(rays.hits(o, up, 20.0).unwrap().is_empty());
        // Through the rim itself the boundary is met (OpenCascade reports `ON` points), and the
        // count is not trusted.
        let o = geom::sub(at(0.0), geom::scale(up, 5.0));
        assert_eq!(rays.any_hit(o, up, 0.0, 20.0), Some(true));
        assert_eq!(rays.crossing_count(o, up, 20.0), None);
        // Along the bore's wall, touching it at mid-height: met, and not counted.
        let along = geom::cross(across, [0.0, 0.0, 1.0]);
        let touch = geom::sub(at(0.0), [0.0, 0.0, 10.0]);
        let o = geom::sub(touch, geom::scale(along, 3.0));
        assert_eq!(rays.any_hit(o, along, 0.0, 6.0), Some(true));
        assert_eq!(rays.crossing_count(o, along, 6.0), None);
    }
}
