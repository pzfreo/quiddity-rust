//! Native cylindrical blend chains (`quiddity._blend_view.BlendCollapseIndex`), with the face
//! graph readings they rest on that no ported family needed before: exact paired edge
//! occurrences and their solid ownership, and the proved material side of a smooth join
//! (`quiddity._adjacency.FaceGraph`: `edge_occurrences`, `shared_occurrences`, `ownership`,
//! `smooth_side`).
//!
//! A chain is a component of native cylinder faces joined by native continuations, sprung
//! smoothly (convex or concave throughout) from exactly two support regions along one
//! nonbranching edge path each, and closed by exactly two terminal edge paths at its ends.
//! Anything else is refused. Of the collapsed graph view (`CollapsedGraphView`) only the
//! support bridges of selected chains are ported, in [`super::experimental_geometry`], which
//! is all the recognisers read of it.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use super::analytic_surfaces::{
    AnalyticFact, SurfaceKind, effective_fact, equivalent_parameters, native_fact,
};
use super::regions::edge_length;
use crate::kernel::brep::{Arc, Part};
use crate::kernel::geom::{self, Curve, Surface, V3};
use crate::kernel::py;
use crate::kernel::sampling::edge_interval;

/// `_SMOOTH_CURVATURE_GAP`: a scaled normal curvature at or below this is none.
const SMOOTH_CURVATURE_GAP: f64 = 1e-6;
/// `_SIDE_SAMPLES`: where along a shared edge (by arc length) the side is read.
const SIDE_SAMPLES: [f64; 3] = [0.25, 0.5, 0.75];

/// One use of an edge in a face's boundary (`EdgeOccurrenceRef`): the face, the loop (wire)
/// and position in it, and the use's direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Half {
    pub face: usize,
    pub wire: usize,
    pub ordinal: usize,
    pub reversed: bool,
    pub edge: usize,
}

/// Two faces' uses of one edge, paired (`SharedEdgeOccurrenceRef`): `endpoints` lower face
/// first, `halves` in the same order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shared {
    pub endpoints: (usize, usize),
    pub halves: (Half, Half),
    pub edge: usize,
}

impl Shared {
    /// `_arc_key`: a stable order independent of traversal.
    pub(crate) fn key(&self) -> (usize, usize, usize, usize, usize, usize) {
        let (l, r) = self.halves;
        (
            self.endpoints.0,
            self.endpoints.1,
            l.wire,
            l.ordinal,
            r.wire,
            r.ordinal,
        )
    }
}

/// `SmoothSide`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SmoothSide {
    Neutral,
    Convex,
    Concave,
    Unproven,
}

/// `FaceGraph.edge_occurrences`: every edge use of the face, loop by loop.
pub fn edge_occurrences(part: &Part, face: usize) -> Vec<Half> {
    let mut out = Vec::new();
    for (wire, lp) in part.faces[face].loops.iter().enumerate() {
        for (ordinal, &(edge, forward)) in lp.edges.iter().enumerate() {
            out.push(Half {
                face,
                wire,
                ordinal,
                reversed: !forward,
                edge,
            });
        }
    }
    out
}

/// `FaceGraph.shared_occurrences`: each edge the two faces use equally often, its uses paired
/// one to one with an opposite use of the other face; an edge without such a unique pairing
/// gives none.
pub fn shared_occurrences(part: &Part, a: usize, b: usize) -> Vec<Shared> {
    if a == b {
        return Vec::new();
    }
    let (left, right) = (a.min(b), a.max(b));
    let right_halves = edge_occurrences(part, right);
    // `_halves_by_edge`: groups in order of first use, members in traversal order.
    let mut groups: Vec<(usize, Vec<Half>)> = Vec::new();
    for half in edge_occurrences(part, left) {
        match groups.iter_mut().find(|(e, _)| *e == half.edge) {
            Some((_, g)) => g.push(half),
            None => groups.push((half.edge, vec![half])),
        }
    }
    let mut out = Vec::new();
    for (edge, left_group) in groups {
        let right_group: Vec<Half> = right_halves
            .iter()
            .filter(|h| h.edge == edge)
            .copied()
            .collect();
        if left_group.len() != right_group.len() {
            continue;
        }
        // A closed edge's recorded direction is not evidence (`Part::solid_is_valid`):
        // OpenCascade rewrites the uses of a full circle it reads the wrong way round, so one
        // use by each face is the opposed pair it reads.
        if part.edges[edge].is_closed() && left_group.len() == 1 {
            out.push(Shared {
                endpoints: (left, right),
                halves: (left_group[0], right_group[0]),
                edge,
            });
            continue;
        }
        let candidates: Vec<Vec<Half>> = left_group
            .iter()
            .map(|l| {
                right_group
                    .iter()
                    .filter(|r| r.reversed != l.reversed)
                    .copied()
                    .collect()
            })
            .collect();
        let unique = candidates.iter().all(|c| c.len() == 1)
            && right_group
                .iter()
                .all(|r| candidates.iter().filter(|c| c.contains(r)).count() == 1);
        if !unique {
            continue;
        }
        for (l, c) in left_group.iter().zip(&candidates) {
            out.push(Shared {
                endpoints: (left, right),
                halves: (*l, c[0]),
                edge,
            });
        }
    }
    out
}

/// The one valid solid both faces belong to, when the edge has exactly those two faces
/// (`FaceGraph.ownership` and `_eligible_side_edge`).
fn edge_owner(part: &Part, a: usize, b: usize, edge: usize) -> Option<usize> {
    let solid = part.faces[a].solid?;
    if part.faces[b].solid != Some(solid) || !part.solid_is_valid(solid) {
        return None;
    }
    let incident = &part.edge_faces()[edge];
    (incident.len() == 2 && incident.contains(&a) && incident.contains(&b)).then_some(solid)
}

/// `FaceGraph.ownership`: the solid of a shared occurrence, when proved.
pub fn ownership(part: &Part, shared: &Shared) -> Option<usize> {
    edge_owner(part, shared.endpoints.0, shared.endpoints.1, shared.edge)
}

/// `BRep_Tool.IsClosed(edge, face)`: the edge is a seam of the face, used twice by it.
/// (Degenerate edges, the other kind Python lets pass, are vertex loops here.)
pub fn closed_in_face(part: &Part, edge: usize, face: usize) -> bool {
    part.faces[face]
        .loops
        .iter()
        .flat_map(|l| &l.edges)
        .filter(|(e, _)| *e == edge)
        .count()
        > 1
}

/// The face's area (`face.area`), where the quadrature integrates it.
pub fn face_area(part: &Part, face: usize) -> f64 {
    part.face_mass(face).map_or(f64::NAN, |m| m[0])
}

/// Python's `min` over floats (first of ties; a NaN compares false and is kept only first).
fn py_min(values: &[f64]) -> f64 {
    values[py::first_min(values, |v| *v)]
}

/// The second partial derivatives (Suu, Svv, Suv) at (u, v): exact for the analytic kinds,
/// central differences of the exact first partials for a freeform surface.
fn second_partials(surface: &Surface, u: f64, v: f64) -> Option<(V3, V3, V3)> {
    let (cu, su) = (u.cos(), u.sin());
    Some(match *surface {
        Surface::Plane { .. } => ([0.0; 3], [0.0; 3], [0.0; 3]),
        Surface::Cylinder { frame, radius } => (
            frame.dir_to_world([-radius * cu, -radius * su, 0.0]),
            [0.0; 3],
            [0.0; 3],
        ),
        Surface::Cone {
            frame,
            radius,
            semi_angle,
        } => {
            let rho = radius + v * semi_angle.sin();
            (
                frame.dir_to_world([-rho * cu, -rho * su, 0.0]),
                [0.0; 3],
                frame.dir_to_world([-semi_angle.sin() * su, semi_angle.sin() * cu, 0.0]),
            )
        }
        Surface::Sphere { frame, radius } => {
            let (cv, sv) = (v.cos(), v.sin());
            (
                frame.dir_to_world([-radius * cv * cu, -radius * cv * su, 0.0]),
                frame.dir_to_world([-radius * cv * cu, -radius * cv * su, -radius * sv]),
                frame.dir_to_world([radius * sv * su, -radius * sv * cu, 0.0]),
            )
        }
        Surface::Torus {
            frame,
            major,
            minor,
        } => {
            let (cv, sv) = (v.cos(), v.sin());
            let rho = major + minor * cv;
            (
                frame.dir_to_world([-rho * cu, -rho * su, 0.0]),
                frame.dir_to_world([-minor * cv * cu, -minor * cv * su, -minor * sv]),
                frame.dir_to_world([minor * sv * su, -minor * sv * cu, 0.0]),
            )
        }
        Surface::Freeform { ref surface, .. } => {
            let (u0, u1, v0, v1) = surface.domain();
            let difference = |lo: f64, hi: f64, at: f64, along_u: bool| {
                let h = 1e-5 * (hi - lo);
                let (a, b) = ((at - h).max(lo), (at + h).min(hi));
                let partials = |t: f64| {
                    let (_, du, dv) = if along_u {
                        surface.value_and_partials(t, v)
                    } else {
                        surface.value_and_partials(u, t)
                    };
                    (du, dv)
                };
                let ((da, ea), (db, eb)) = (partials(a), partials(b));
                let k = 1.0 / (b - a);
                (
                    geom::scale(geom::sub(db, da), k),
                    geom::scale(geom::sub(eb, ea), k),
                )
            };
            let (duu, dvu) = difference(u0, u1, u, true);
            let (_, dvv) = difference(v0, v1, v, false);
            (duu, dvv, dvu)
        }
        Surface::Other { .. } => return None,
    })
}

/// The point at *fraction* of the edge's length from its first curve parameter, and the curve's
/// derivative there (`GCPnts_AbscissaPoint` on `BRepAdaptor_Curve`).
fn edge_point(part: &Part, edge: usize, fraction: f64) -> (V3, V3) {
    let e = &part.edges[edge];
    let (t0, t1) = edge_interval(&e.curve, e.start, e.end, e.same_sense, e.is_closed());
    match e.curve {
        Curve::Line { .. } | Curve::Circle { .. } => {
            let t = t0.min(t1) + fraction * (t1 - t0).abs();
            (e.curve.value(t), e.curve.derivative(t))
        }
        _ => {
            // Along the samples (start → end), from whichever end has the lower parameter.
            let fraction = if t0 <= t1 { fraction } else { 1.0 - fraction };
            let lengths: Vec<f64> = e
                .samples
                .windows(2)
                .map(|w| geom::dist(w[0], w[1]))
                .collect();
            let target = fraction * lengths.iter().sum::<f64>();
            let (mut run, mut q) = (0.0, e.end);
            for (w, len) in e.samples.windows(2).zip(&lengths) {
                if run + len >= target && *len > 0.0 {
                    q = geom::add(
                        w[0],
                        geom::scale(geom::sub(w[1], w[0]), (target - run) / len),
                    );
                    break;
                }
                run += len;
            }
            let t = e.curve.parameter(q);
            (e.curve.value(t), e.curve.derivative(t))
        }
    }
}

/// `FaceGraph._normal_curvature`: the face's normal curvature (against its outward normal) at
/// *point* across the edge running along *tangent*, and whether the face is a native plane.
fn normal_curvature(part: &Part, face: usize, point: V3, tangent: V3) -> Option<(f64, bool)> {
    if geom::norm(tangent) < 1e-12 {
        return None;
    }
    let tangent = geom::scale(tangent, 1.0 / geom::norm(tangent));
    let f = &part.faces[face];
    let (u, v) = f.surface.parameters(point, None)?;
    let (du, dv) = f.surface.partials(u, v);
    let (duu, dvv, duv) = second_partials(&f.surface, u, v)?;
    let cross = geom::cross(du, dv);
    if geom::norm(cross) < 1e-12 {
        return None;
    }
    let mut normal = geom::scale(cross, 1.0 / geom::norm(cross));
    if f.reversed {
        normal = geom::scale(normal, -1.0);
    }
    let inward = geom::cross(normal, tangent);
    if geom::norm(inward) < 1e-12 {
        return None;
    }
    let inward = geom::scale(inward, 1.0 / geom::norm(inward));
    let (e, ff, g) = (geom::dot(du, du), geom::dot(du, dv), geom::dot(dv, dv));
    let det = e * g - ff * ff;
    if !det.is_finite() || det.abs() < 1e-18 {
        return None;
    }
    let (rhs_u, rhs_v) = (geom::dot(inward, du), geom::dot(inward, dv));
    let along_u = (rhs_u * g - rhs_v * ff) / det;
    let along_v = (rhs_v * e - rhs_u * ff) / det;
    let first = geom::add(geom::scale(du, along_u), geom::scale(dv, along_v));
    let denominator = geom::dot(first, first);
    if !denominator.is_finite() || denominator < 1e-18 {
        return None;
    }
    let second = geom::add(
        geom::add(
            geom::scale(duu, along_u * along_u),
            geom::scale(duv, 2.0 * along_u * along_v),
        ),
        geom::scale(dvv, along_v * along_v),
    );
    let curvature = geom::dot(normal, second) / denominator;
    curvature
        .is_finite()
        .then_some((curvature, matches!(f.surface, Surface::Plane { .. })))
}

/// One recognition run's face graph readings for blends, cached per face pair
/// (`FaceGraph` and `EffectiveSurfaceIndex` together).
pub struct BlendGraph<'a> {
    pub part: &'a Part,
    native: Vec<Option<AnalyticFact>>,
    effective: Vec<std::sync::OnceLock<Option<AnalyticFact>>>,
    arcs: RefCell<BTreeMap<(usize, usize), Option<Arc>>>,
    sides: RefCell<BTreeMap<(usize, usize), Option<SmoothSide>>>,
}

impl<'a> BlendGraph<'a> {
    pub fn new(part: &'a Part) -> Self {
        BlendGraph {
            part,
            native: (0..part.faces.len())
                .map(|f| native_fact(part, f))
                .collect(),
            effective: (0..part.faces.len())
                .map(|_| std::sync::OnceLock::new())
                .collect(),
            arcs: RefCell::new(BTreeMap::new()),
            sides: RefCell::new(BTreeMap::new()),
        }
    }

    /// The native analytic fact (`_fact` in the blend view: native and oriented).
    pub fn native(&self, face: usize) -> Option<&AnalyticFact> {
        self.native[face].as_ref()
    }

    /// `EffectiveSurfaceIndex.fact`, when analytic.
    pub fn effective(&self, face: usize) -> Option<&AnalyticFact> {
        self.effective[face]
            .get_or_init(|| effective_fact(self.part, face))
            .as_ref()
    }

    /// The face's neighbours in ascending order.
    pub fn neighbours(&self, face: usize) -> Vec<usize> {
        let mut out = self.part.neighbours(face);
        out.sort_unstable();
        out
    }

    /// `FaceGraph.arc`, cached per unordered pair.
    pub fn arc(&self, a: usize, b: usize) -> Option<Arc> {
        let key = (a.min(b), a.max(b));
        if let Some(found) = self.arcs.borrow().get(&key) {
            return *found;
        }
        let found = self.part.arc(a, b);
        self.arcs.borrow_mut().insert(key, found);
        found
    }

    pub fn smooth(&self, a: usize, b: usize) -> bool {
        self.arc(a, b) == Some(Arc::Smooth)
    }

    /// `FaceGraph.smooth_side`: `None` unless the pair is a smooth join; otherwise the one
    /// side every shared edge proves, or `Unproven`.
    pub fn smooth_side(&self, a: usize, b: usize) -> Option<SmoothSide> {
        let key = (a.min(b), a.max(b));
        if let Some(found) = self.sides.borrow().get(&key) {
            return *found;
        }
        let found = self.smooth(a, b).then(|| {
            let answers: BTreeSet<SmoothSide> = self
                .part
                .shared_edges(a, b)
                .into_iter()
                .map(|edge| self.smooth_side_edge(a, b, edge))
                .collect();
            if answers.len() == 1 {
                *answers.first().unwrap()
            } else {
                SmoothSide::Unproven
            }
        });
        self.sides.borrow_mut().insert(key, found);
        found
    }

    /// `_derive_smooth_side_edge`: neutral where the two faces are one native analytic
    /// surface; otherwise the side all three samples agree on.
    fn smooth_side_edge(&self, a: usize, b: usize, edge: usize) -> SmoothSide {
        if edge_owner(self.part, a, b, edge).is_none() {
            return SmoothSide::Unproven;
        }
        let local = py_min(&[
            edge_length(self.part, edge),
            face_area(self.part, a).sqrt(),
            face_area(self.part, b).sqrt(),
        ]);
        if !(local.is_finite() && local > 0.0) {
            return SmoothSide::Unproven;
        }
        if self.native_continuation(a, b, local) {
            return SmoothSide::Neutral;
        }
        let samples: BTreeSet<SmoothSide> = SIDE_SAMPLES
            .iter()
            .map(|&f| self.smooth_side_sample(a, b, edge, f, local))
            .collect();
        if samples.len() == 1 {
            *samples.first().unwrap()
        } else {
            SmoothSide::Unproven
        }
    }

    /// `_native_continuation`: one native kind with equivalent parameters at *local*.
    fn native_continuation(&self, a: usize, b: usize, local: f64) -> bool {
        match (self.native(a), self.native(b)) {
            (Some(l), Some(r)) if l.kind == r.kind => {
                equivalent_parameters(l.kind, &l.parameters, &r.parameters, local) == Some(true)
            }
            _ => false,
        }
    }

    /// `_smooth_side_sample`: both faces' normal curvatures across the edge, scaled by *local*;
    /// a plane's zero is no evidence, any other zero or equal curvatures prove nothing.
    fn smooth_side_sample(
        &self,
        a: usize,
        b: usize,
        edge: usize,
        fraction: f64,
        local: f64,
    ) -> SmoothSide {
        let (point, tangent) = edge_point(self.part, edge, fraction);
        let (Some(left), Some(right)) = (
            normal_curvature(self.part, a, point, tangent),
            normal_curvature(self.part, b, point, tangent),
        ) else {
            return SmoothSide::Unproven;
        };
        let mut values = Vec::new();
        for (curvature, planar) in [left, right] {
            let scaled = curvature * local;
            if scaled.abs() <= SMOOTH_CURVATURE_GAP {
                if planar {
                    continue;
                }
                return SmoothSide::Unproven;
            }
            values.push(scaled);
        }
        if values.is_empty() || ((left.0 - right.0) * local).abs() <= SMOOTH_CURVATURE_GAP {
            return SmoothSide::Unproven;
        }
        if values.iter().all(|v| *v < 0.0) {
            SmoothSide::Convex
        } else if values.iter().all(|v| *v > 0.0) {
            SmoothSide::Concave
        } else {
            SmoothSide::Unproven
        }
    }

    /// `_native_neutral`: a smooth join proved neutral between native faces of one kind.
    fn native_neutral(&self, a: usize, b: usize) -> bool {
        self.smooth(a, b)
            && self.smooth_side(a, b) == Some(SmoothSide::Neutral)
            && matches!((self.native(a), self.native(b)), (Some(l), Some(r)) if l.kind == r.kind)
    }

    fn cylinder(&self, face: usize) -> Option<&AnalyticFact> {
        self.native(face)
            .filter(|f| f.kind == SurfaceKind::Cylinder)
    }

    /// `_arc_refs`: the pair's shared occurrences in `_arc_key` order.
    fn arc_refs(&self, a: usize, b: usize) -> Vec<Shared> {
        let mut found = shared_occurrences(self.part, a, b);
        found.sort_by_key(Shared::key);
        found
    }
}

/// A complete native cylindrical blend chain (`BlendChain`), reduced to what `blends` reads.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendChain {
    /// Ascending.
    pub blend_nodes: Vec<usize>,
    /// Each ascending, the two ordered by their least face.
    pub supports: [Vec<usize>; 2],
    pub spring_arcs: Vec<Shared>,
    /// The occurrences joining the chain's own faces, in `_arc_key` order.
    pub internal_arcs: Vec<Shared>,
    /// The occurrences closing the chain's two ends, in `_arc_key` order.
    pub terminal_arcs: Vec<Shared>,
    /// Convex or concave.
    pub side: SmoothSide,
    pub radius: f64,
    pub solid: usize,
}

/// `_edge_groups`: the occurrences' edges in vertex-connected groups, each in `_arc_key` order,
/// ordered by their first.
fn edge_groups(part: &Part, occurrences: &[Shared]) -> Vec<Vec<Shared>> {
    let mut ordered = occurrences.to_vec();
    ordered.sort_by_key(Shared::key);
    let vertices = |s: &Shared| {
        let (a, b) = part.edges[s.edge].vertices;
        [a, b]
    };
    let mut remaining: BTreeSet<usize> = (0..ordered.len()).collect();
    let mut groups = Vec::new();
    while let Some(first) = remaining.pop_first() {
        let mut pending = BTreeSet::from([first]);
        let mut reached = BTreeSet::new();
        while let Some(at) = pending.pop_first() {
            if !reached.insert(at) {
                continue;
            }
            let here = vertices(&ordered[at]);
            let touching: Vec<usize> = remaining
                .iter()
                .copied()
                .filter(|&o| vertices(&ordered[o]).iter().any(|v| here.contains(v)))
                .collect();
            for o in touching {
                remaining.remove(&o);
                pending.insert(o);
            }
        }
        groups.push(reached.into_iter().map(|i| ordered[i]).collect::<Vec<_>>());
    }
    groups.sort_by_key(|g: &Vec<Shared>| g[0].key());
    groups
}

/// `_one_nonbranching_edge_group`: the edges form one open, connected, nonbranching path.
fn one_nonbranching_edge_group(part: &Part, occurrences: &[Shared]) -> bool {
    let groups = edge_groups(part, occurrences);
    if groups.len() != 1 {
        return false;
    }
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    for s in &groups[0] {
        let (a, b) = part.edges[s.edge].vertices;
        *counts.entry(a).or_default() += 1;
        if b != a {
            *counts.entry(b).or_default() += 1;
        }
    }
    counts.values().filter(|&&n| n == 1).count() == 2 && counts.values().all(|&n| n == 1 || n == 2)
}

/// `_physical_length`: the total length of the distinct edges.
fn physical_length(part: &Part, occurrences: &[Shared]) -> f64 {
    let mut seen = BTreeSet::new();
    py::fsum(
        occurrences
            .iter()
            .filter(|s| seen.insert(s.edge))
            .map(|s| edge_length(part, s.edge)),
    )
}

impl BlendGraph<'_> {
    /// `_cylinder_components`: native cylinder faces joined by native continuations.
    fn cylinder_components(&self) -> Vec<Vec<usize>> {
        let mut pending: BTreeSet<usize> = (0..self.part.faces.len())
            .filter(|&f| self.cylinder(f).is_some())
            .collect();
        let mut components = Vec::new();
        while let Some(first) = pending.pop_first() {
            let mut found = vec![first];
            let mut queue = std::collections::VecDeque::from([first]);
            while let Some(current) = queue.pop_front() {
                for n in self.neighbours(current) {
                    if pending.contains(&n) && self.native_neutral(current, n) {
                        pending.remove(&n);
                        found.push(n);
                        queue.push_back(n);
                    }
                }
            }
            found.sort_unstable();
            components.push(found);
        }
        components
    }

    /// The blend view's `_support_region`: native continuations from *first*, outside
    /// *excluded*, across occurrences all owned by *solid*.
    fn support_region(&self, first: usize, excluded: &[usize], solid: usize) -> Vec<usize> {
        let mut found = BTreeSet::from([first]);
        let mut queue = std::collections::VecDeque::from([first]);
        while let Some(current) = queue.pop_front() {
            for n in self.neighbours(current) {
                if found.contains(&n) || excluded.contains(&n) || !self.native_neutral(current, n) {
                    continue;
                }
                let occurrences = shared_occurrences(self.part, current, n);
                if occurrences.is_empty()
                    || occurrences
                        .iter()
                        .any(|o| ownership(self.part, o) != Some(solid))
                {
                    continue;
                }
                found.insert(n);
                queue.push_back(n);
            }
        }
        found.into_iter().collect()
    }

    /// `BlendCollapseIndex._classify`: the component as a complete chain, or `None` (refused).
    fn classify(&self, component: &[usize]) -> Option<BlendChain> {
        let part = self.part;
        let inside = |f: usize| component.contains(&f);
        let mut spring_neighbours: BTreeMap<usize, BTreeMap<usize, Vec<Shared>>> =
            component.iter().map(|&n| (n, BTreeMap::new())).collect();
        let mut terminal_entries: Vec<(usize, Shared)> = Vec::new();
        let mut internal: Vec<Shared> = Vec::new();
        let mut sides = BTreeSet::new();
        let mut solid: Option<usize> = None;
        let mut degree: BTreeMap<usize, usize> = component.iter().map(|&n| (n, 0)).collect();
        let mut accounted: BTreeSet<Half> = BTreeSet::new();
        for &node in component {
            for n in self.neighbours(node) {
                if inside(n) && node > n {
                    continue;
                }
                let refs = self.arc_refs(node, n);
                if refs.is_empty() {
                    return None;
                }
                for r in &refs {
                    accounted.insert(r.halves.0);
                    accounted.insert(r.halves.1);
                    let owner = ownership(part, r)?;
                    if *solid.get_or_insert(owner) != owner {
                        return None;
                    }
                }
                if inside(n) {
                    if !self.native_neutral(node, n) || !one_nonbranching_edge_group(part, &refs) {
                        return None;
                    }
                    internal.extend(refs);
                    *degree.get_mut(&node).unwrap() += 1;
                    *degree.get_mut(&n).unwrap() += 1;
                    continue;
                }
                let side = self.smooth_side(node, n);
                if self.smooth(node, n)
                    && matches!(side, Some(SmoothSide::Convex | SmoothSide::Concave))
                {
                    spring_neighbours
                        .get_mut(&node)
                        .unwrap()
                        .entry(n)
                        .or_default()
                        .extend(refs);
                    sides.insert(side.unwrap());
                    continue;
                }
                if self.smooth(node, n) {
                    return None;
                }
                terminal_entries.extend(refs.into_iter().map(|r| (node, r)));
            }
        }
        for &node in component {
            for half in edge_occurrences(part, node) {
                if !accounted.contains(&half) && !closed_in_face(part, half.edge, node) {
                    return None;
                }
            }
        }
        let degrees: Vec<usize> = degree.values().copied().collect();
        if component.len() == 1 {
            if degrees[0] != 0 {
                return None;
            }
        } else if degrees.iter().filter(|&&d| d == 1).count() != 2
            || degrees.iter().any(|&d| d != 1 && d != 2)
        {
            return None;
        }
        if sides.len() != 1 {
            return None;
        }
        let solid = solid?;
        let mut cache: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        let mut by_patch: BTreeMap<usize, BTreeMap<Vec<usize>, Vec<Shared>>> =
            component.iter().map(|&n| (n, BTreeMap::new())).collect();
        for (&node, neighbours) in &spring_neighbours {
            for (&n, refs) in neighbours {
                let region = match cache.get(&n) {
                    Some(r) => r.clone(),
                    None => {
                        let r = self.support_region(n, component, solid);
                        for &m in &r {
                            cache.insert(m, r.clone());
                        }
                        r
                    }
                };
                by_patch
                    .get_mut(&node)
                    .unwrap()
                    .entry(region)
                    .or_default()
                    .extend(refs.iter().copied());
            }
        }
        let support_sets: BTreeSet<&Vec<usize>> =
            by_patch.values().flat_map(|g| g.keys()).collect();
        if support_sets.len() != 2
            || by_patch
                .values()
                .any(|g| g.keys().collect::<BTreeSet<_>>() != support_sets)
        {
            return None;
        }
        if by_patch
            .values()
            .flat_map(|g| g.values())
            .any(|arcs| !one_nonbranching_edge_group(part, arcs))
        {
            return None;
        }
        let terminal: Vec<Shared> = terminal_entries.iter().map(|(_, r)| *r).collect();
        let terminal_groups = edge_groups(part, &terminal);
        if terminal_groups.len() != 2 {
            return None;
        }
        let path_ends: BTreeSet<usize> = if component.len() == 1 {
            component.iter().copied().collect()
        } else {
            degree
                .iter()
                .filter(|(_, d)| **d == 1)
                .map(|(n, _)| *n)
                .collect()
        };
        let mut attached_ends = BTreeSet::new();
        for group in &terminal_groups {
            if !one_nonbranching_edge_group(part, group) {
                return None;
            }
            let attached: BTreeSet<usize> = terminal_entries
                .iter()
                .filter(|(_, r)| group.contains(r))
                .map(|(b, _)| *b)
                .collect();
            if attached.len() != 1 || !attached.is_subset(&path_ends) {
                return None;
            }
            attached_ends.extend(attached);
        }
        if attached_ends != path_ends {
            return None;
        }
        let mut supports: Vec<Vec<usize>> = support_sets.into_iter().cloned().collect();
        supports.sort_by_key(|s| s[0]);
        let spring_groups: Vec<Vec<Shared>> = supports
            .iter()
            .map(|support| {
                let mut arcs: Vec<Shared> = component
                    .iter()
                    .flat_map(|n| by_patch[n][support].iter().copied())
                    .collect();
                arcs.sort_by_key(Shared::key);
                arcs
            })
            .collect();
        let first = self.cylinder(component[0])?;
        let radius = first.parameters[6];
        let area_scale =
            |faces: &[usize]| py::fsum(faces.iter().map(|&f| face_area(part, f))).sqrt();
        let mut local_values = vec![radius];
        local_values.extend(
            spring_groups
                .iter()
                .chain(&terminal_groups)
                .map(|g| physical_length(part, g)),
        );
        local_values.push(area_scale(component));
        local_values.extend(supports.iter().map(|s| area_scale(s)));
        if local_values.iter().any(|v| !(v.is_finite() && *v > 0.0)) {
            return None;
        }
        let local = py_min(&local_values);
        if component.iter().any(|&n| {
            self.cylinder(n).is_none_or(|f| {
                equivalent_parameters(
                    SurfaceKind::Cylinder,
                    &first.parameters,
                    &f.parameters,
                    local,
                ) != Some(true)
            })
        }) {
            return None;
        }
        let mut spring_arcs: Vec<Shared> = spring_groups.into_iter().flatten().collect();
        spring_arcs.sort_by_key(Shared::key);
        internal.sort_by_key(Shared::key);
        let mut terminal_arcs: Vec<Shared> = terminal_groups.into_iter().flatten().collect();
        terminal_arcs.sort_by_key(Shared::key);
        let [a, b]: [Vec<usize>; 2] = supports.try_into().ok()?;
        Some(BlendChain {
            blend_nodes: component.to_vec(),
            supports: [a, b],
            spring_arcs,
            internal_arcs: internal,
            terminal_arcs,
            side: sides.pop_first().unwrap(),
            radius,
            solid,
        })
    }

    /// `BlendCollapseIndex.chains`: every complete chain, chains whose supports overlap without
    /// being the same refused.
    pub fn chains(&self) -> Vec<BlendChain> {
        let chains: Vec<BlendChain> = self
            .cylinder_components()
            .iter()
            .filter_map(|c| self.classify(c))
            .collect();
        let overlap = |a: &[usize], b: &[usize]| a.iter().any(|f| b.contains(f));
        let conflicted: Vec<bool> = (0..chains.len())
            .map(|i| {
                (0..chains.len()).any(|j| {
                    j != i
                        && (overlap(&chains[i].blend_nodes, &chains[j].blend_nodes)
                            || chains[i].supports.iter().any(|l| {
                                chains[j].supports.iter().any(|r| l != r && overlap(l, r))
                            }))
                })
            })
            .collect();
        chains
            .into_iter()
            .zip(conflicted)
            .filter(|(_, c)| !c)
            .map(|(chain, _)| chain)
            .collect()
    }
}
