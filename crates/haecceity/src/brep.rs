//! The boundary-representation model the recognisers read: faces, edges and solids, in the
//! traversal order OpenCascade gives a STEP import, so face indices mean the same thing on both
//! sides of the port.

use std::f64::consts::TAU;
use std::sync::OnceLock;

use super::geom::{self, Bounds, Curve, Surface, V3};
use super::sampling::{edge_interval, extremes_along};
use super::uv::{FaceDomain, UvLoop, touches_singular_point};

const AXES: [V3; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

#[derive(Clone, Debug)]
pub struct Edge {
    pub curve: Curve,
    pub start: V3,
    pub end: V3,
    /// Vertex identities (indices into the part's vertices): an edge is closed when its two
    /// ends are the same vertex, not merely coincident points.
    pub vertices: (usize, usize),
    /// Whether start → end follows the curve's parameter direction (`EDGE_CURVE.same_sense`).
    pub same_sense: bool,
    /// Points along the edge, start → end, dense enough to stand in for the exact curve.
    pub samples: Vec<V3>,
}

#[derive(Clone, Debug)]
pub struct Loop {
    /// Edge indices with their use direction in this loop (`true` = start → end).
    pub edges: Vec<(usize, bool)>,
    /// A loop made of a single vertex (`VERTEX_LOOP`): the apex of a cone, the pole of a sphere.
    pub vertex: Option<V3>,
}

#[derive(Clone, Debug)]
pub struct Face {
    pub surface: Surface,
    /// `TopAbs_REVERSED`: the face's material side is against the surface normal.
    pub reversed: bool,
    pub loops: Vec<Loop>,
    pub solid: Option<usize>,
    /// The file's parameter-space curves (pcurves) for this face's edges, by edge. OpenCascade
    /// sizes a face's parameter range from its pcurves, not from the 3D edges (which may sit
    /// off the surface within tolerance), so [`Part::uv_bounds`] uses them where given.
    pub pcurves: Vec<(usize, Pcurve)>,
}

/// How a solid turns where two faces meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arc {
    /// The material forms a wedge at the edge.
    Convex,
    /// The material wraps round the edge.
    Concave,
    /// The outward normals agree there (a split face, or a tangent blend).
    Smooth,
    Unknown,
}

/// A pcurve, reduced to what sizing a parameter range needs.
#[derive(Clone, Debug)]
pub enum Pcurve {
    /// A B-spline spanning exactly its edge: its control polygon (OpenCascade boxes poles).
    Poles(Vec<(f64, f64)>),
    /// A straight line in parameter space: a point on it and its direction.
    Line { point: (f64, f64), dir: (f64, f64) },
}

#[derive(Clone, Debug)]
pub struct Solid {
    pub faces: Vec<usize>,
}

/// Per-face values derived from the geometry, computed on first use.
#[derive(Debug, Default)]
pub(super) struct FaceCache {
    pub(super) uv_loops: OnceLock<Option<Vec<UvLoop>>>,
    pub(super) domain: OnceLock<Option<FaceDomain>>,
    pub(super) bounds: OnceLock<Bounds>,
    pub(super) mass: OnceLock<Option<[f64; 5]>>,
    pub(super) edge_deviation: OnceLock<Vec<(usize, f64)>>,
    pub(super) uv_bounds: OnceLock<Option<(f64, f64, f64, f64)>>,
    pub(super) recovered: OnceLock<Option<Surface>>,
}

impl Edge {
    /// Whether the edge starts and ends at the same vertex (a full circle, a closed spline).
    pub fn is_closed(&self) -> bool {
        self.vertices.0 == self.vertices.1
    }

    /// The point halfway along the edge by arc length (build123d `Edge.center()`): exact for
    /// lines and circular arcs, measured along the samples otherwise.
    pub fn midpoint(&self) -> V3 {
        match &self.curve {
            Curve::Line { .. } => geom::scale(geom::add(self.start, self.end), 0.5),
            Curve::Circle { .. } => {
                let (a, b) = edge_interval(
                    &self.curve,
                    self.start,
                    self.end,
                    self.same_sense,
                    self.is_closed(),
                );
                self.curve.value(0.5 * (a + b))
            }
            _ => {
                let lengths: Vec<f64> = self
                    .samples
                    .windows(2)
                    .map(|w| geom::dist(w[0], w[1]))
                    .collect();
                let half = 0.5 * lengths.iter().sum::<f64>();
                let mut run = 0.0;
                for (w, len) in self.samples.windows(2).zip(&lengths) {
                    if run + len >= half && *len > 0.0 {
                        let t = (half - run) / len;
                        return geom::add(w[0], geom::scale(geom::sub(w[1], w[0]), t));
                    }
                    run += len;
                }
                self.end
            }
        }
    }

    /// The vertex points (one for a closed edge), as build123d's `edge.vertices()` gives them.
    pub fn vertex_points(&self) -> Vec<V3> {
        if self.is_closed() {
            vec![self.start]
        } else {
            vec![self.start, self.end]
        }
    }
}

/// Whether the face's domain holds a neighbourhood of (u, v): the points a step of *h* away
/// in u and in v, each way, are all inside it (the point itself is never read). A point on the
/// domain's boundary, a seam included, is inside or not by round-off, so the answer there would
/// follow the part's placement; this one is "no" there, and placement-independent elsewhere.
pub(super) fn holds(domain: &FaceDomain, u: f64, v: f64, (hu, hv): (f64, f64)) -> bool {
    [(-hu, 0.0), (hu, 0.0), (0.0, -hv), (0.0, hv)]
        .iter()
        .all(|&(du, dv)| domain.contains(u + du, v + dv))
}

/// A part's boundary representation: faces, edges and solids in OpenCascade's traversal order.
///
/// Immutable after construction. `faces`, `edges` and `solids` are public for reading only:
/// the per-face caches (parameter-space loops and bounds, boxes, mass, recovered surfaces),
/// the edge-to-face map and the solid validity are computed lazily from them on first use and
/// never recomputed, so a change to the topology after any query leaves those answers stale.
/// Build a new `Part` instead.
#[derive(Debug)]
pub struct Part {
    pub faces: Vec<Face>,
    pub edges: Vec<Edge>,
    pub solids: Vec<Solid>,
    pub(super) cache: Vec<FaceCache>,
    edge_faces: OnceLock<Vec<Vec<usize>>>,
    valid_solids: OnceLock<Vec<bool>>,
    unresolved_faces: Vec<usize>,
    unresolved_edges: Vec<usize>,
}

impl Part {
    pub fn new(faces: Vec<Face>, edges: Vec<Edge>, solids: Vec<Solid>) -> Self {
        let cache = faces.iter().map(|_| FaceCache::default()).collect();
        Part {
            faces,
            edges,
            solids,
            cache,
            edge_faces: OnceLock::new(),
            valid_solids: OnceLock::new(),
            unresolved_faces: Vec::new(),
            unresolved_edges: Vec::new(),
        }
    }

    /// Records the faces whose surface (`Surface::Other`, unevaluable) and the edges whose curve
    /// (its chord stands in) the reader could not resolve.
    pub(super) fn with_unresolved(mut self, faces: Vec<usize>, edges: Vec<usize>) -> Self {
        self.unresolved_faces = faces;
        self.unresolved_edges = edges;
        self
    }

    /// The faces whose surface did not resolve: they carry `Surface::Other`, which evaluates
    /// to NaN and meets no ray.
    pub fn unresolved_faces(&self) -> &[usize] {
        &self.unresolved_faces
    }

    /// The edges whose curve did not resolve: each carries its chord in its place.
    pub fn unresolved_edges(&self) -> &[usize] {
        &self.unresolved_edges
    }

    /// The face's axis-aligned box: its boundary plus any interior axis extremes.
    pub fn face_bounds(&self, face: usize) -> Bounds {
        *self.cache[face]
            .bounds
            .get_or_init(|| self.compute_face_bounds(face))
    }

    fn compute_face_bounds(&self, face: usize) -> Bounds {
        let mut b = Bounds::empty();
        for p in self.face_extreme_points(face, &AXES) {
            b.add(p);
        }
        b
    }

    /// The face's boundary samples plus every point where it may be extreme along one of the
    /// unit directions *dirs* (both senses): circle arcs' exact extremes and the interior
    /// extremes of doubly-curved faces (exact for spheres and tori, sampled for freeform
    /// surfaces) that lie on the face. Their extent along each direction is the face's.
    fn face_extreme_points(&self, face: usize, dirs: &[V3]) -> Vec<V3> {
        let f = &self.faces[face];
        let mut out = Vec::new();
        for lp in &f.loops {
            if let Some(p) = lp.vertex {
                out.push(p);
            }
            for &(e, _) in &lp.edges {
                let edge = &self.edges[e];
                out.extend(&edge.samples);
                // Conics' extremes in closed form: their samples are taken in the part's
                // placement, so would put the box's sampling error there too.
                if let Curve::Circle { .. } | Curve::Ellipse { .. } = edge.curve {
                    let interval = edge_interval(
                        &edge.curve,
                        edge.start,
                        edge.end,
                        edge.same_sense,
                        edge.is_closed(),
                    );
                    out.extend(extremes_along(&edge.curve, interval, dirs));
                }
            }
        }
        // Doubly-curved faces can bulge past their boundary: add their interior extremes that
        // lie on the face.
        let Some(domain) = self.domain(face) else {
            return out;
        };
        // A candidate counts where the face holds a neighbourhood of it (`holds`): one on the
        // face's boundary adds nothing the edges do not.
        match &f.surface {
            Surface::Sphere { .. } | Surface::Torus { .. } => {
                for (u, v) in f.surface.extreme_parameters_along(dirs) {
                    if holds(domain, u, v, (1e-6 * TAU, 1e-6 * TAU))
                        || touches_singular_point(&f.surface, domain, v)
                    {
                        out.push(f.surface.value(u, v));
                    }
                }
            }
            // A cone face can run to its apex without a vertex there to bound it. The apex is a
            // whole parameter line (u is undefined there), so whether the face reaches it is read
            // round that line, on both sides, between the u steps that a seam at u = 0 would
            // put on the face's boundary.
            Surface::Cone {
                radius, semi_angle, ..
            } => {
                let apex = -radius / semi_angle.sin();
                let reaches = [apex - 1e-4, apex + 1e-4]
                    .into_iter()
                    .any(|v| (0..64).any(|k| domain.contains(TAU * (k as f64 + 0.5) / 64.0, v)));
                if reaches {
                    out.push(f.surface.value(0.0, apex));
                }
            }
            // Sampled: a grid over the surface's parameter domain.
            Surface::Freeform { surface, .. } => {
                let (u0, u1, v0, v1) = surface.domain();
                let n = 12;
                let h = (1e-6 * (u1 - u0), 1e-6 * (v1 - v0));
                for i in 0..=n {
                    for j in 0..=n {
                        let u = u0 + (u1 - u0) * i as f64 / n as f64;
                        let v = v0 + (v1 - v0) * j as f64 / n as f64;
                        if holds(domain, u, v, h) {
                            out.push(f.surface.value(u, v));
                        }
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// The least and greatest coordinate of *faces* along the unit direction *d*: the extent
    /// along *d* of the box OpenCascade gives the shape turned so that *d* is a coordinate axis.
    pub fn extent_along(&self, faces: &[usize], d: V3) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &face in faces {
            for p in self.face_extreme_points(face, &[d]) {
                let s = geom::dot(p, d);
                lo = lo.min(s);
                hi = hi.max(s);
            }
        }
        (lo, hi)
    }

    pub fn bounds(&self) -> Bounds {
        let mut b = Bounds::empty();
        for i in 0..self.faces.len() {
            b.merge(&self.face_bounds(i));
        }
        b
    }

    /// Faces of each edge, in face order (`edge_face_map`). A seam edge lists its face once.
    pub fn edge_faces(&self) -> &[Vec<usize>] {
        self.edge_faces.get_or_init(|| {
            let mut out = vec![Vec::new(); self.edges.len()];
            for face in 0..self.faces.len() {
                for e in self.face_edges(face) {
                    out[e].push(face);
                }
            }
            out
        })
    }

    /// The face's distinct edges in loop order (build123d `face.edges()`): a seam once.
    pub fn face_edges(&self, face: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for lp in &self.faces[face].loops {
            for &(e, _) in &lp.edges {
                if !out.contains(&e) {
                    out.push(e);
                }
            }
        }
        out
    }

    /// The edges two faces share, in *a*'s edge order.
    pub fn shared_edges(&self, a: usize, b: usize) -> Vec<usize> {
        let theirs = self.face_edges(b);
        self.face_edges(a)
            .into_iter()
            .filter(|e| theirs.contains(e))
            .collect()
    }

    /// The box of every face of a solid.
    pub fn solid_bounds(&self, solid: usize) -> Bounds {
        let mut b = Bounds::empty();
        for &f in &self.solids[solid].faces {
            b.merge(&self.face_bounds(f));
        }
        b
    }

    /// How the solid turns where two faces meet (`FaceGraph.arc`): `None` when they share no
    /// edge, `Unknown` when their shared edges disagree or a normal cannot be read.
    pub fn arc(&self, a: usize, b: usize) -> Option<Arc> {
        let shared = self.shared_edges(a, b);
        let first = self.arc_at(a, b, *shared.first()?);
        Some(if shared.iter().all(|&e| self.arc_at(a, b, e) == first) {
            first
        } else {
            Arc::Unknown
        })
    }

    /// The turn at one shared edge: walking the edge in *a*'s boundary direction, left (in *a*'s
    /// surface) points into *a*; which side of *b* that is decides convex or concave.
    fn arc_at(&self, a: usize, b: usize, edge: usize) -> Arc {
        let Some(forward) = self.faces[a]
            .loops
            .iter()
            .flat_map(|l| &l.edges)
            .find(|e| e.0 == edge)
            .map(|e| e.1)
        else {
            return Arc::Unknown;
        };
        let ed = &self.edges[edge];
        let (t0, t1) = edge_interval(&ed.curve, ed.start, ed.end, ed.same_sense, ed.is_closed());
        let middle = 0.5 * (t0 + t1);
        let h = 1e-6 * (t1 - t0).abs().max(1e-12);
        let tangent = geom::sub(ed.curve.value(middle + h), ed.curve.value(middle - h));
        // The interval already runs start → end; the loop may use the edge the other way.
        let walk = if (t1 >= t0) == forward {
            tangent
        } else {
            geom::scale(tangent, -1.0)
        };
        let (Some(direction), point) = (geom::unit(walk), ed.curve.value(middle)) else {
            return Arc::Unknown;
        };
        let normal = |face: usize| {
            let (u, v) = self.faces[face].surface.parameters(point, None)?;
            self.face_normal(face, u, v)
        };
        let (Some(na), Some(nb)) = (normal(a), normal(b)) else {
            return Arc::Unknown;
        };
        if 1.0 - geom::dot(na, nb) <= geom::SMOOTH_ARC_GAP {
            return Arc::Smooth;
        }
        // A closed edge's recorded direction is not evidence (see `solid_is_valid`): step to
        // its left in *a* and walk the other way if that leaves the face.
        let mut direction = direction;
        if ed.is_closed() {
            let step = 1e-4 * self.face_bounds(a).diagonal().max(1e-9);
            let surface = &self.faces[a].surface;
            let hint = surface.parameters(point, None);
            let on_face = |side: f64| {
                let left = geom::scale(geom::cross(na, direction), side * step);
                let (u, v) = surface.parameters(geom::add(point, left), hint)?;
                Some(self.domain(a)?.contains(u, v))
            };
            // Only clear evidence (the face on the right, not the left) overrides the record.
            if on_face(1.0) == Some(false) && on_face(-1.0) == Some(true) {
                direction = geom::scale(direction, -1.0);
            }
        }
        if geom::dot(geom::cross(na, direction), nb) < 0.0 {
            Arc::Convex
        } else {
            Arc::Concave
        }
    }

    /// The face's outward normal at (u, v): the surface normal, flipped for a reversed face.
    pub fn face_normal(&self, face: usize, u: f64, v: f64) -> Option<V3> {
        let f = &self.faces[face];
        let n = f.surface.normal(u, v)?;
        Some(if f.reversed { geom::scale(n, -1.0) } else { n })
    }

    /// The distinct faces sharing an edge with *face*, in the face's own edge order
    /// (`_adjacency.neighbours`).
    pub fn neighbours(&self, face: usize) -> Vec<usize> {
        let edge_faces = self.edge_faces();
        let mut out = Vec::new();
        for lp in &self.faces[face].loops {
            for &(e, _) in &lp.edges {
                for &other in &edge_faces[e] {
                    if other != face && !out.contains(&other) {
                        out.push(other);
                    }
                }
            }
        }
        out
    }

    /// Does this face's surface frame already point out of the solid? `None` for surfaces with
    /// no single frame direction to read (`_adjacency.frame_points_outward`).
    pub fn frame_points_outward(&self, face: usize) -> Option<bool> {
        let f = &self.faces[face];
        let frame = match &f.surface {
            Surface::Plane { frame }
            | Surface::Cylinder { frame, .. }
            | Surface::Sphere { frame, .. } => frame,
            _ => return None,
        };
        Some(!f.reversed == frame.direct())
    }

    /// The topological half of `BRepCheck` validity that the evidence path depends on: every
    /// edge of the solid is used by exactly two faces (or twice by one, as a seam), and two
    /// different faces sharing an open edge run it in opposite directions, so the shell is closed
    /// and orientable.
    ///
    /// A closed edge (a full circle) is exempt from the direction test: it joins itself
    /// whichever way it is run, so its recorded direction is not evidence. OpenCascade writes
    /// some toroidal faces' circles the wrong way round and re-derives the direction from the
    /// parameter-space curves on reading.
    ///
    /// A solid with a face or edge whose geometry did not resolve is not valid: its shape is
    /// not known.
    pub fn solid_is_valid(&self, solid: usize) -> bool {
        self.valid_solids.get_or_init(|| {
            (0..self.solids.len())
                .map(|s| self.check_solid(s))
                .collect()
        })[solid]
    }

    fn check_solid(&self, solid: usize) -> bool {
        let faces = &self.solids[solid].faces;
        if faces.iter().any(|f| self.unresolved_faces.contains(f)) {
            return false;
        }
        let mut uses: std::collections::BTreeMap<usize, Vec<(usize, bool)>> = Default::default();
        for &f in faces {
            for lp in &self.faces[f].loops {
                for &(e, forward) in &lp.edges {
                    uses.entry(e).or_default().push((f, forward));
                }
            }
        }
        !uses.is_empty()
            && !uses.keys().any(|e| self.unresolved_edges.contains(e))
            && uses.iter().all(|(&e, u)| match u.as_slice() {
                [(fa, da), (fb, db)] => fa == fb || da != db || self.edges[e].is_closed(),
                _ => false,
            })
    }
}
