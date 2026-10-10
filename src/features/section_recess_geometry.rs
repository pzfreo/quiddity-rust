//! Section-recess discovery and projection (`quiddity._section_recess_geometry`): every proved
//! constant-section recess on a part as a candidate (its faces, mouth, body, classification and
//! public geometry), and the projections that turn each prover's private proof into the
//! published [`SectionRecessGeometry`].
//!
//! Five sources propose candidates: open cylindrical seats, intact native floors read three ways
//! (an obround of two semicircular caps and two flats, a straight-edged polygon, a mixed
//! line/arc wire), polygonal pockets and passages ending on a native cylinder, and polygonal
//! passages under a two-plane roof. A floor reading is proved by volume probes: the swept
//! section holds no material, the slab past its mouth is air and the slab under its floor is
//! material. Each projection rounds the proof to the publication grid and refuses (with
//! Python's message) when that moves the whole occurrence by more than 0.002.
//!
//! Python catches its own `RuntimeError`, `TypeError`, `ValueError` and `ZeroDivisionError`
//! around a probe or a projection and drops that one candidate; here a refused projection is
//! an `Err` the caller drops the same way, and a probe the kernel cannot build or answer proves
//! nothing (Python's kernel raises there). The values Python computes outside those guards
//! (a frame, a body reference, an occurrence) refuse the whole discovery, as Python's raise
//! does.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use super::Context;
use super::cylindrical_channels::CylindricalChannelProof;
use super::cylindrical_end_surface::CylindricalEndSurface;
use super::cylindrical_passages::{CylindricalPassageProof, cylindrical_passage_proofs};
use super::cylindrical_pockets::{CylindricalPocketProof, cylindrical_pocket_proofs};
use super::cylindrical_seats::{CylindricalSeatProof, cylindrical_seat_proofs};
use super::effective_surfaces::EffectiveFaces;
use super::entry_treatments::material_fraction;
use super::evidence::common_valid_solid;
use super::graph::{face_vertices, is_planar, normal, shared_occurrences, span};
use super::passages::{PassageFrame, PassageSectionVertex};
use super::plane_envelope_passages::{PlaneEnvelopePassageProof, plane_envelope_passage_proofs};
use super::policy::length_tol;
use super::recess_obround::END_RADIUS_FRAC;
use super::section_passages::{end_slab, line_section, probe_prism};
use super::section_recess::{
    ClosedSectionProfile, EndSurface, OpenSectionProfile, PlanarEndSurface, PlanarEndTerm,
    PlanarEnvelopeEndSurface, SectionEnd, SectionProfile, SectionRecessEnds, SectionRecessError,
    SectionRecessGeometry, open_profile_material_side,
};
use super::sections::{
    BodyRefIssuer, LocalFrame, PlanarSection, SectionEnds, SectionOccurrence, SectionVertex, V2,
    arc as section_arc, occurrence_geometry,
};
use super::support_patches::covered_patch;
use crate::kernel::brep::{Arc, Edge, Face, Loop, Part};
use crate::kernel::geom::{Curve, Frame, Surface, V3};
use crate::kernel::py;
use crate::kernel::sampling::{edge_interval, sample_edge};
use crate::kernel::sweep::extrude_face;

const DIRECTION_TOL: f64 = 1e-6;
const SEMICIRCLE_TOL: f64 = 1e-4;
/// The kernel-coordinate floor the floor readers' probes are drawn in by.
const INSET: f64 = 1e-6;
const EMPTY: f64 = 1e-9;

type Checked<T> = Result<T, SectionRecessError>;

fn refuse<T>(message: &'static str) -> Checked<T> {
    Err(SectionRecessError(message))
}

/// One proved section recess before it is numbered (`_Candidate`): its defining and constituent
/// faces (sorted, but a seat's walls in their proof order), the mouth face, the solid, the
/// public geometry and its classification.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub defining_faces: Vec<usize>,
    pub constituent_faces: Vec<usize>,
    pub mouth: usize,
    pub body: usize,
    pub geometry: SectionRecessGeometry,
    pub section_shape: &'static str,
    pub feature_kind: &'static str,
}

/// The cylinder a cylindrical end lies on: the three cylindrical proofs' common fields.
#[derive(Clone, Copy, Debug)]
pub struct CylinderEnd {
    pub axis_point: V3,
    pub axis_direction: V3,
    pub radius: f64,
}

impl From<&CylindricalPocketProof> for CylinderEnd {
    fn from(p: &CylindricalPocketProof) -> Self {
        CylinderEnd {
            axis_point: p.axis_point,
            axis_direction: p.axis_direction,
            radius: p.radius,
        }
    }
}

impl From<&CylindricalChannelProof> for CylinderEnd {
    fn from(p: &CylindricalChannelProof) -> Self {
        CylinderEnd {
            axis_point: p.axis_point,
            axis_direction: p.axis_direction,
            radius: p.radius,
        }
    }
}

impl From<&CylindricalPassageProof> for CylinderEnd {
    fn from(p: &CylindricalPassageProof) -> Self {
        CylinderEnd {
            axis_point: p.axis_point,
            axis_direction: p.axis_direction,
            radius: p.radius,
        }
    }
}

/// Whether an observed plane caps the original wall support (`has_physical_planar_floor`): a
/// planar constituent face facing out along *axis* on the open side, of no thickness along it,
/// level with every wall's capped end and within 0.002 of the published floor.
///
/// A ring's cap witnesses can be curved blends: they establish blindness, but their tangent
/// level does not establish a physical plane.
pub fn has_physical_planar_floor(
    part: &Part,
    walls: &BTreeSet<usize>,
    constituent: &BTreeSet<usize>,
    axis: &str,
    open_sign: i32,
    published_floor: f64,
) -> bool {
    let Some(coordinate) = ["x", "y", "z"].iter().position(|&a| a == axis) else {
        return false;
    };
    let end = if open_sign == 1 { 0 } else { 1 };
    let pick = |(low, high): (f64, f64)| if end == 0 { low } else { high };
    for &node in constituent.difference(walls) {
        if !is_planar(part, node) {
            continue;
        }
        let Some(n) = normal(part, node) else {
            continue;
        };
        if n[coordinate] * f64::from(open_sign) < 1.0 - DIRECTION_TOL {
            continue;
        }
        let (low, high) = span(part, node, coordinate);
        if high - low > 1e-6 {
            continue;
        }
        let floor = (low + high) / 2.0;
        if walls
            .iter()
            .any(|&wall| (pick(span(part, wall, coordinate)) - floor).abs() > 1e-6)
        {
            continue;
        }
        // This comparison bounds publication displacement, not source recognition.
        if (published_floor - floor).abs() <= 0.002 {
            return true;
        }
    }
    false
}

fn dot(a: V3, b: V3) -> f64 {
    py::dot(&a, &b)
}

fn subtract(left: V3, right: V3) -> V3 {
    [0, 1, 2].map(|i| left[i] - right[i])
}

fn scale(vector: V3, factor: f64) -> V3 {
    vector.map(|c| c * factor)
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// The unit direction turned to point along its largest component, ties to the later axis, and
/// round-off components cleared (`_canonical`). `None` for a degenerate direction (Python's
/// "direction must be nonzero"); every caller passes a unit normal or axis.
fn canonical(vector: V3) -> Option<V3> {
    let mut value = py::unit(vector)?;
    let pivot = (0..3)
        .max_by(|&a, &b| {
            value[a]
                .abs()
                .partial_cmp(&value[b].abs())
                .unwrap_or(Ordering::Equal)
                .then(a.cmp(&b))
        })
        .expect("three axes");
    if value[pivot] < 0.0 {
        value = scale(value, -1.0);
    }
    Some(value.map(|c| if c.abs() < 5e-13 { 0.0 } else { c }))
}

fn parallel(left: V3, right: V3) -> bool {
    (dot(left, right).abs() - 1.0).abs() <= DIRECTION_TOL
}

/// A native cylinder's radius, canonical axis direction and axis location (`_cylinder`).
fn cylinder(part: &Part, node: usize) -> Option<(f64, V3, V3)> {
    let Surface::Cylinder { frame, radius } = part.faces[node].surface else {
        return None;
    };
    Some((radius, canonical(frame.z)?, frame.origin))
}

fn is_cylinder(part: &Part, node: usize) -> bool {
    matches!(part.faces[node].surface, Surface::Cylinder { .. })
}

/// A circular edge's length (`edge.length`).
fn circle_length(edge: &Edge) -> Option<f64> {
    let Curve::Circle { radius, .. } = edge.curve else {
        return None;
    };
    let (a, b) = edge_interval(
        &edge.curve,
        edge.start,
        edge.end,
        edge.same_sense,
        edge.is_closed(),
    );
    Some(radius * (b - a).abs())
}

/// The angle the floor's shared circular edges sweep on a cylinder of *radius* (`_edge_sweep`).
fn edge_sweep(part: &Part, floor: usize, cylinder: usize, radius: f64) -> Option<f64> {
    let occurrences = shared_occurrences(part, floor, cylinder);
    if occurrences.is_empty() {
        return None;
    }
    let lengths: Option<Vec<f64>> = occurrences
        .iter()
        .map(|&e| circle_length(&part.edges[e]))
        .collect();
    Some(py::sum(lengths?) / radius)
}

/// The face's vertices' extent along *direction* (`_node_interval`).
fn node_interval(part: &Part, node: usize, direction: V3) -> Option<(f64, f64)> {
    let values: Vec<f64> = face_vertices(part, node)
        .into_iter()
        .map(|p| dot(p, direction))
        .collect();
    if values.is_empty() {
        return None;
    }
    let low = values.iter().copied().fold(f64::INFINITY, f64::min);
    let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Some((low, high))
}

fn project(point: V3, frame: &LocalFrame) -> V2 {
    let relative = subtract(point, frame.origin);
    [dot(relative, frame.u), dot(relative, frame.v)]
}

/// The face's area centroid (`face.center()` of a planar face).
fn face_center(part: &Part, face: usize) -> Option<V3> {
    Some(part.face_moments(face)?.centroid)
}

/// The edge's vertex point for vertex identity *vertex*.
fn vertex_point(edge: &Edge, vertex: usize) -> V3 {
    if edge.vertices.0 == vertex {
        edge.start
    } else {
        edge.end
    }
}

/// The face's one wire's corners in walk order, each the single vertex an edge shares with the
/// one before (`edges[index - 1].vertices()` against `edge.vertices()`), with the edges.
fn wire_corners(part: &Part, face: usize) -> Option<(Vec<usize>, Vec<V3>)> {
    let loops = &part.faces[face].loops;
    if loops.len() != 1 {
        return None;
    }
    let edges: Vec<usize> = loops[0].edges.iter().map(|&(e, _)| e).collect();
    let ends = |e: usize| {
        let v = part.edges[e].vertices;
        if v.0 == v.1 {
            vec![v.0]
        } else {
            vec![v.0, v.1]
        }
    };
    let mut corners = Vec::new();
    for (index, &e) in edges.iter().enumerate() {
        let previous = edges[(index + edges.len() - 1) % edges.len()];
        let mine = ends(e);
        let shared: Vec<usize> = ends(previous)
            .into_iter()
            .filter(|v| mine.contains(v))
            .collect();
        let &[vertex] = shared.as_slice() else {
            return None;
        };
        corners.push(vertex_point(&part.edges[e], vertex));
    }
    Some((edges, corners))
}

/// One physical straight-edged floor wire in *frame*, centred on its centroid
/// (`_polygonal_section`).
fn polygonal_section(part: &Part, floor: usize, frame: &LocalFrame) -> Option<PlanarSection> {
    let (edges, corners) = wire_corners(part, floor)?;
    if edges.len() < 3
        || edges
            .iter()
            .any(|&e| !matches!(part.edges[e].curve, Curve::Line { .. }))
    {
        return None;
    }
    let points: Vec<V2> = corners.iter().map(|&p| project(p, frame)).collect();
    let raw = PlanarSection::polygon(&points).ok()?;
    let centre = raw.centroid();
    PlanarSection::new(
        raw.boundary()
            .iter()
            .map(|v| SectionVertex::new([v.point[0] - centre[0], v.point[1] - centre[1]], 0.0))
            .collect::<Result<_, _>>()
            .ok()?,
    )
    .ok()
}

/// A flat end plane (Python's default `SectionEnd` surface).
fn flat() -> Checked<EndSurface> {
    Ok(EndSurface::Planar(PlanarEndSurface::new(
        "plane",
        [0.0, 0.0],
    )?))
}

fn end(condition: &str) -> Checked<SectionEnd> {
    SectionEnd::new(condition, flat()?)
}

/// The obround through two cap centres as a published geometry (`_public_geometry`).
fn public_geometry(
    depth: V3,
    first_center: V3,
    second_center: V3,
    radius: f64,
    run_interval: (f64, f64),
    floor_at: f64,
) -> Checked<SectionRecessGeometry> {
    let centroid = [0, 1, 2].map(|i| (first_center[i] + second_center[i]) / 2.0);
    let frame = LocalFrame::canonical(depth, centroid)?;
    let first = project(first_center, &frame);
    let second = project(second_center, &frame);
    let long = [second[0] - first[0], second[1] - first[1]];
    let length = py::hypot(&long);
    if length <= 1e-12 {
        return refuse("obround cap centres must be distinct");
    }
    let direction = [long[0] / length, long[1] / length];
    let width = [-direction[1], direction[0]];
    let points = [
        [first[0] - radius * width[0], first[1] - radius * width[1]],
        [second[0] - radius * width[0], second[1] - radius * width[1]],
        [second[0] + radius * width[0], second[1] + radius * width[1]],
        [first[0] + radius * width[0], first[1] + radius * width[1]],
    ];
    let section = PlanarSection::new(
        points
            .iter()
            .enumerate()
            .map(|(i, &p)| SectionVertex::new(p, if i == 1 || i == 3 { 1.0 } else { 0.0 }))
            .collect::<Result<_, _>>()?,
    )?;
    let (low, high) = run_interval;
    let mut issuer = BodyRefIssuer::default();
    let occurrence = SectionOccurrence::new(
        issuer.issue(None)?,
        frame,
        run_interval,
        section,
        SectionEnds::new(
            (floor_at - low).abs() <= (floor_at - high).abs(),
            (floor_at - high).abs() < (floor_at - low).abs(),
        )?,
    )?;
    project_section_recess_geometry(&occurrence, &issuer)
}

/// A closed private section occurrence as the public geometry
/// (`project_section_recess_geometry`): its published frame, interval and boundary, each end
/// capped or open under a flat plane.
pub fn project_section_recess_geometry(
    occurrence: &SectionOccurrence,
    body_refs: &BodyRefIssuer,
) -> Checked<SectionRecessGeometry> {
    let projected = occurrence_geometry(occurrence, body_refs)?;
    let f = projected.frame;
    let boundary = projected
        .section
        .boundary
        .iter()
        .map(|v| PassageSectionVertex::new(v.point, v.bulge))
        .collect::<Result<_, _>>()?;
    let condition = |capped: bool| if capped { "capped" } else { "open" };
    SectionRecessGeometry::new(
        "section_recess",
        PassageFrame::new(f.origin, f.run, f.u, f.v)?,
        projected.run_interval,
        SectionProfile::Closed(ClosedSectionProfile::new("closed", boundary)?),
        SectionRecessEnds::new(
            end(condition(projected.ends.low_capped))?,
            end(condition(projected.ends.high_capped))?,
        )?,
    )
}

/// The exact line/semicircle prism between *low* and *high* along *depth* (`_obround_prism`):
/// the probe before publication rounding, not a chord polygon.
fn obround_prism(
    depth: V3,
    first: V3,
    second: V3,
    radius: f64,
    low: f64,
    high: f64,
) -> Option<Part> {
    let along = subtract(second, first);
    let along = subtract(along, scale(depth, dot(along, depth)));
    let direction = py::unit(along)?;
    if high <= low {
        return None;
    }
    let width = cross(depth, direction);
    let at_low = |center: V3| {
        let shift = low - dot(center, depth);
        [0, 1, 2].map(|i| center[i] + depth[i] * shift)
    };
    let point = |center: V3, offset: V3, sign: f64| {
        let base = at_low(center);
        [0, 1, 2].map(|i| base[i] + sign * radius * offset[i])
    };
    let corners = [
        point(first, width, -1.0),
        point(second, width, -1.0),
        point(second, width, 1.0),
        point(first, width, 1.0),
    ];
    // The walk is counter-clockwise about `depth` (`direction` × `width` = `depth`), each arc
    // about its centre at `low` from −width through ±direction to +width.
    let circle = |center: V3| Curve::Circle {
        frame: Frame {
            origin: at_low(center),
            x: direction,
            y: width,
            z: depth,
        },
        radius,
    };
    let curves = [
        Curve::Line {
            origin: corners[0],
            dir: subtract(corners[1], corners[0]),
        },
        circle(second),
        Curve::Line {
            origin: corners[2],
            dir: subtract(corners[3], corners[2]),
        },
        circle(first),
    ];
    let edges: Vec<Edge> = curves
        .into_iter()
        .enumerate()
        .map(|(i, curve)| {
            let (start, end) = (corners[i], corners[(i + 1) % 4]);
            Edge {
                samples: sample_edge(&curve, start, end, true, false),
                curve,
                start,
                end,
                vertices: (i, (i + 1) % 4),
                same_sense: true,
            }
        })
        .collect();
    let face = Face {
        surface: Surface::Plane {
            frame: Frame {
                origin: corners[0],
                x: direction,
                y: width,
                z: depth,
            },
        },
        reversed: false,
        loops: vec![Loop {
            edges: (0..4).map(|e| (e, true)).collect(),
            vertex: None,
        }],
        solid: None,
        pcurves: Vec::new(),
    };
    let section = Part::new(vec![face], edges, Vec::new());
    extrude_face(&section, 0, [0.0; 3], scale(depth, high - low))
}

/// The share of *probe* *owner*'s material fills; an unbuilt or unanswered probe proves
/// nothing.
fn fraction(ctx: &Context<'_>, owner: usize, probe: Option<Part>) -> Option<f64> {
    material_fraction(ctx, owner, &probe?)
}

/// An intact native obround pocket on *floor*: two semicircular caps and two flats, all
/// concave to it, one mouth plane, proved by probes (`_one_obround_candidate`).
fn one_obround_candidate(ctx: &Context<'_>, floor: usize) -> Option<Candidate> {
    let part = ctx.part;
    let depth = canonical(normal(part, floor)?)?;
    let concave: Vec<usize> = part
        .neighbours(floor)
        .into_iter()
        .filter(|&n| part.arc(floor, n) == Some(Arc::Concave))
        .collect();
    let cylinders: Vec<usize> = concave
        .iter()
        .copied()
        .filter(|&n| is_cylinder(part, n))
        .collect();
    let sides: Vec<usize> = concave
        .iter()
        .copied()
        .filter(|&n| is_planar(part, n))
        .collect();
    if cylinders.len() != 2 || sides.len() != 2 || concave.len() != 4 {
        return None;
    }
    if cylinders
        .iter()
        .any(|&n| part.frame_points_outward(n) != Some(false))
    {
        return None;
    }
    let first = cylinder(part, cylinders[0])?;
    let second = cylinder(part, cylinders[1])?;
    let radius = first.0;
    if (first.0 - second.0).abs() > length_tol(radius, END_RADIUS_FRAC)
        || !parallel(first.1, second.1)
        || !parallel(first.1, depth)
    {
        return None;
    }
    let long = subtract(second.2, first.2);
    let long = subtract(long, scale(depth, dot(long, depth)));
    let long_direction = py::unit(long)?;
    let width_direction = canonical(cross(depth, long_direction))?;
    let normals = sides
        .iter()
        .map(|&n| canonical(normal(part, n)?))
        .collect::<Option<Vec<V3>>>()?;
    if !parallel(normals[0], normals[1])
        || normals.iter().any(|&n| !parallel(n, width_direction))
        || normals.iter().any(|&n| dot(n, depth).abs() > DIRECTION_TOL)
    {
        return None;
    }
    if !cylinders
        .iter()
        .all(|&c| sides.iter().all(|&s| part.arc(c, s) == Some(Arc::Smooth)))
    {
        return None;
    }
    for &c in &cylinders {
        let sweep = edge_sweep(part, floor, c, radius)?;
        if (sweep - std::f64::consts::PI).abs() > SEMICIRCLE_TOL {
            return None;
        }
    }
    let supports: Vec<usize> = cylinders.iter().chain(&sides).copied().collect();
    let spans = supports
        .iter()
        .map(|&n| node_interval(part, n, depth))
        .collect::<Option<Vec<_>>>()?;
    let low = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let high = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    let tolerance = length_tol(high - low, END_RADIUS_FRAC);
    if high - low <= tolerance
        || spans
            .iter()
            .any(|s| (s.0 - low).abs() > tolerance || (s.1 - high).abs() > tolerance)
    {
        return None;
    }
    let floor_interval = node_interval(part, floor, depth)?;
    let floor_at = (floor_interval.0 + floor_interval.1) / 2.0;
    if (floor_at - low).abs().min((floor_at - high).abs()) > tolerance {
        return None;
    }
    let mouth_at = if (floor_at - low).abs() <= tolerance {
        high
    } else {
        low
    };
    let mut context: BTreeSet<usize> = part.neighbours(cylinders[0]).into_iter().collect();
    for &node in &supports[1..] {
        let theirs: BTreeSet<usize> = part.neighbours(node).into_iter().collect();
        context = context.intersection(&theirs).copied().collect();
    }
    let mouths: Vec<usize> = context
        .into_iter()
        .filter(|&node| node != floor)
        .filter(|&node| {
            let n = if is_planar(part, node) {
                normal(part, node)
            } else {
                None
            };
            let interval = node_interval(part, node, depth);
            n.and_then(canonical).is_some_and(|n| parallel(n, depth))
                && interval.is_some_and(|i| ((i.0 + i.1) / 2.0 - mouth_at).abs() <= tolerance)
                && supports
                    .iter()
                    .all(|&s| matches!(part.arc(node, s), Some(Arc::Convex) | Some(Arc::Smooth)))
        })
        .collect();
    let mut members = supports.clone();
    members.push(floor);
    let owner = common_valid_solid(part, &members);
    let (&[mouth], Some(owner)) = (mouths.as_slice(), owner) else {
        return None;
    };
    if high - low <= 2.0 * INSET {
        return None;
    }
    let thickness = 2e-5f64.max(1.0f64.max(high - low).max(radius).max(py::hypot(&long)) * 1e-4);
    let floor_sign = if (floor_at - low).abs() <= tolerance {
        -1.0
    } else {
        1.0
    };
    let probe = |start: f64, end: f64| {
        let (lo, hi) = if start <= end {
            (start, end)
        } else {
            (end, start)
        };
        obround_prism(depth, first.2, second.2, radius, lo, hi)
    };
    if fraction(ctx, owner, probe(low + INSET, high - INSET))? > EMPTY
        || fraction(
            ctx,
            owner,
            probe(
                mouth_at - floor_sign * INSET,
                mouth_at - floor_sign * thickness,
            ),
        )? > EMPTY
        || fraction(
            ctx,
            owner,
            probe(
                floor_at + floor_sign * INSET,
                floor_at + floor_sign * thickness,
            ),
        )? < 1.0 - EMPTY
    {
        return None;
    }
    let geometry = public_geometry(depth, first.2, second.2, radius, (low, high), floor_at).ok()?;
    let mut defining = supports;
    defining.sort_unstable();
    let mut constituent = defining.clone();
    constituent.push(floor);
    constituent.sort_unstable();
    Some(Candidate {
        defining_faces: defining,
        constituent_faces: constituent,
        mouth,
        body: owner,
        geometry,
        section_shape: "obround",
        feature_kind: "pocket",
    })
}

/// The section shape a polygon's corners name (`_polygonal_shape`).
pub fn polygonal_shape(points: &[V2]) -> &'static str {
    match points.len() {
        4 => {
            let edges: Vec<V2> = (0..4)
                .map(|i| {
                    let (p, q) = (points[i], points[(i + 1) % 4]);
                    [q[0] - p[0], q[1] - p[1]]
                })
                .collect();
            let square = (0..4).all(|i| {
                let (edge, following) = (edges[i], edges[(i + 1) % 4]);
                py::hypot(&edge) > 0.0
                    && (edge[0] * following[0] + edge[1] * following[1]).abs()
                        <= DIRECTION_TOL * py::hypot(&edge) * py::hypot(&following)
            });
            if square { "rectangular" } else { "polygonal" }
        }
        3 => "triangular",
        6 => "hexagonal",
        _ => "polygonal",
    }
}

/// The probes' common answer: the section between the ends holds no material, past the mouth
/// is air and under the floor is material.
fn probes_prove(
    ctx: &Context<'_>,
    owner: usize,
    void: Option<Part>,
    mouth: Option<Part>,
    under: Option<Part>,
) -> Option<bool> {
    Some(
        fraction(ctx, owner, void)? <= EMPTY
            && fraction(ctx, owner, mouth)? <= EMPTY
            && fraction(ctx, owner, under)? >= 1.0 - EMPTY,
    )
}

/// An intact native straight-edged pocket on *floor*: planar walls concave to it along its
/// normal, mouth planes every wall meets, proved by probes (`_one_polygonal_candidate`).
fn one_polygonal_candidate(ctx: &Context<'_>, floor: usize) -> Checked<Option<Candidate>> {
    let part = ctx.part;
    let Some(depth) = normal(part, floor).and_then(canonical) else {
        return Ok(None);
    };
    let walls: Vec<usize> = part
        .neighbours(floor)
        .into_iter()
        .filter(|&n| part.arc(floor, n) == Some(Arc::Concave) && is_planar(part, n))
        .collect();
    if walls.len() < 3
        || walls
            .iter()
            .any(|&w| normal(part, w).is_none_or(|n| dot(n, depth).abs() > DIRECTION_TOL))
    {
        return Ok(None);
    }
    let Some(spans) = walls
        .iter()
        .map(|&w| node_interval(part, w, depth))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let low = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let high = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    let tolerance = length_tol(high - low, END_RADIUS_FRAC);
    if high - low <= tolerance
        || spans
            .iter()
            .any(|s| (s.0 - low).abs() > tolerance || (s.1 - high).abs() > tolerance)
    {
        return Ok(None);
    }
    let Some(floor_interval) = node_interval(part, floor, depth) else {
        return Ok(None);
    };
    let floor_at = (floor_interval.0 + floor_interval.1) / 2.0;
    if (floor_at - low).abs().min((floor_at - high).abs()) > tolerance {
        return Ok(None);
    }
    let mouth_at = if (floor_at - low).abs() <= tolerance {
        high
    } else {
        low
    };
    let context: BTreeSet<usize> = walls.iter().flat_map(|&w| part.neighbours(w)).collect();
    let meets = |node: usize, wall: usize| {
        matches!(part.arc(node, wall), Some(Arc::Convex) | Some(Arc::Smooth))
    };
    let mouths: Vec<usize> = context
        .into_iter()
        .filter(|&node| node != floor)
        .filter(|&node| {
            let n = if is_planar(part, node) {
                normal(part, node)
            } else {
                None
            };
            let interval = node_interval(part, node, depth);
            n.and_then(canonical).is_some_and(|n| parallel(n, depth))
                && interval.is_some_and(|i| ((i.0 + i.1) / 2.0 - mouth_at).abs() <= tolerance)
                && walls.iter().any(|&w| meets(node, w))
        })
        .collect();
    // A physical mouth may be partitioned into several coplanar stock faces. Every wall still
    // needs observed termination context on that same plane; none of these consulted patches
    // becomes defining or constituent pocket evidence.
    let mut members = walls.clone();
    members.push(floor);
    members.extend(&mouths);
    let owner = common_valid_solid(part, &members);
    let Some(owner) = owner else {
        return Ok(None);
    };
    if mouths.is_empty() || walls.iter().any(|&w| !mouths.iter().any(|&m| meets(m, w))) {
        return Ok(None);
    }
    let Some(centre) = face_center(part, floor) else {
        // Python's `center()` measures any face; the kernel could not integrate this one.
        return Ok(None);
    };
    let frame = LocalFrame::canonical(depth, centre)?;
    let Some(section) = polygonal_section(part, floor, &frame) else {
        return Ok(None);
    };
    if section.boundary().len() != walls.len() {
        return Ok(None);
    }
    let scale_ = 1.0f64.max(high - low);
    let radius = section
        .boundary()
        .iter()
        .map(|v| py::hypot(&v.point))
        .fold(f64::NEG_INFINITY, f64::max);
    let thickness = 2e-5f64.max(scale_ * 1e-4).max(radius * 1e-4);
    let floor_sign = if (floor_at - low).abs() <= tolerance {
        -1.0
    } else {
        1.0
    };
    let mouth_sign = -floor_sign;
    if probes_prove(
        ctx,
        owner,
        probe_prism(&frame, (low, high), &section),
        end_slab(&frame, mouth_at, mouth_sign, thickness, &section),
        end_slab(&frame, floor_at, floor_sign, thickness, &section),
    ) != Some(true)
    {
        return Ok(None);
    }
    let mut issuer = BodyRefIssuer::default();
    let points: Vec<V2> = section.boundary().iter().map(|v| v.point).collect();
    let occurrence = SectionOccurrence::new(
        issuer.issue(None)?,
        frame,
        (low, high),
        section,
        SectionEnds::new(
            (floor_at - low).abs() <= (floor_at - high).abs(),
            (floor_at - high).abs() < (floor_at - low).abs(),
        )?,
    )?;
    let Ok(geometry) = project_section_recess_geometry(&occurrence, &issuer) else {
        return Ok(None);
    };
    let mut defining = walls;
    defining.sort_unstable();
    let mut constituent = defining.clone();
    constituent.push(floor);
    constituent.sort_unstable();
    Ok(Some(Candidate {
        defining_faces: defining,
        constituent_faces: constituent,
        mouth: *mouths.iter().min().expect("nonempty"),
        body: owner,
        geometry,
        section_shape: polygonal_shape(&points),
        feature_kind: "pocket",
    }))
}

/// The floor's observed line/arc wire in a canonical frame through its centroid, short edges
/// and offset axes kept as they are (`_mixed_floor_section`).
fn mixed_floor_section(
    part: &Part,
    floor: usize,
    depth: V3,
) -> Option<(LocalFrame, PlanarSection)> {
    let loops = &part.faces[floor].loops;
    if loops.len() != 1 {
        return None;
    }
    let kinds: BTreeSet<&str> = loops[0]
        .edges
        .iter()
        .map(|&(e, _)| match part.edges[e].curve {
            Curve::Line { .. } => "LINE",
            Curve::Circle { .. } => "CIRCLE",
            _ => "OTHER",
        })
        .collect();
    if kinds != BTreeSet::from(["CIRCLE", "LINE"]) {
        return None;
    }
    let frame = LocalFrame::canonical(depth, face_center(part, floor)?).ok()?;
    let (edges, corners) = wire_corners(part, floor)?;
    let corners: Vec<V2> = corners.iter().map(|&p| project(p, &frame)).collect();
    let mut vertices = Vec::new();
    for (index, &e) in edges.iter().enumerate() {
        let (start, end) = (corners[index], corners[(index + 1) % corners.len()]);
        let edge = &part.edges[e];
        let mut bulge = 0.0;
        if let Curve::Circle { radius, .. } = edge.curve {
            let mid = project(edge.midpoint(), &frame);
            let turn =
                (mid[0] - start[0]) * (end[1] - mid[1]) - (mid[1] - start[1]) * (end[0] - mid[0]);
            if turn.abs() <= 1e-12 {
                return None;
            }
            bulge = (circle_length(edge)? / radius / 4.0).tan().copysign(turn);
        }
        vertices.push(SectionVertex::new(start, bulge).ok()?);
    }
    let raw = PlanarSection::new(vertices).ok()?;
    let offset = raw.centroid();
    let frame = LocalFrame::canonical(
        depth,
        [0, 1, 2].map(|i| frame.origin[i] + frame.u[i] * offset[0] + frame.v[i] * offset[1]),
    )
    .ok()?;
    let section = PlanarSection::new(
        raw.boundary()
            .iter()
            .map(|v| SectionVertex::new([v.point[0] - offset[0], v.point[1] - offset[1]], v.bulge))
            .collect::<Result<_, _>>()
            .ok()?,
    )
    .ok()?;
    Some((frame, section))
}

/// An intact native mixed line/arc pocket proved from its exact swept floor
/// (`_one_mixed_candidate`): the swept floor's every face covered by the walls, the floor and
/// its copy at the mouth, and the probes.
fn one_mixed_candidate(ctx: &Context<'_>, floor: usize) -> Option<Candidate> {
    let part = ctx.part;
    let depth = canonical(normal(part, floor)?)?;
    let (frame, section) = mixed_floor_section(part, floor, depth)?;
    let walls: Vec<usize> = part
        .neighbours(floor)
        .into_iter()
        .filter(|&n| part.arc(floor, n) == Some(Arc::Concave))
        .collect();
    if walls.is_empty() {
        return None;
    }
    let spans = walls
        .iter()
        .map(|&w| node_interval(part, w, depth))
        .collect::<Option<Vec<_>>>()?;
    let low = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let high = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    let moments = part.face_moments(floor)?;
    let floor_at = dot(moments.centroid, depth);
    let tolerance = 1e-6;
    if high - low <= 2.0 * tolerance {
        return None;
    }
    let (mouth_at, sign) = if (floor_at - low).abs() <= tolerance {
        (high, 1.0)
    } else if (floor_at - high).abs() <= tolerance {
        (low, -1.0)
    } else {
        return None;
    };
    let context: BTreeSet<usize> = walls
        .iter()
        .flat_map(|&w| part.neighbours(w))
        .filter(|&n| n != floor)
        .collect();
    let mouths: Vec<usize> = context
        .into_iter()
        .filter(|&node| {
            is_planar(part, node)
                && normal(part, node).is_some_and(|n| parallel(n, depth))
                && node_interval(part, node, depth).is_some_and(|(a, b)| {
                    (a - mouth_at).abs().max((b - mouth_at).abs()) <= tolerance
                })
        })
        .collect();
    if mouths.is_empty()
        || walls.iter().any(|&w| {
            !mouths
                .iter()
                .any(|&m| matches!(part.arc(w, m), Some(Arc::Convex) | Some(Arc::Smooth)))
        })
    {
        return None;
    }
    let mut members = vec![floor];
    members.extend(&walls);
    members.extend(&mouths);
    let owner = common_valid_solid(part, &members)?;
    let swept = extrude_face(part, floor, [0.0; 3], scale(depth, mouth_at - floor_at))?;
    // The swept floor's caps are the source floor and its copy at the mouth; the mouth cap is
    // face 1 because haecceity's `extrude_face` pushes the far cap there.
    let mut supports: Vec<(&Part, usize)> = walls.iter().map(|&w| (part, w)).collect();
    supports.push((part, floor));
    supports.push((&swept, 1));
    if (0..swept.faces.len()).any(|patch| !covered_patch((&swept, patch), &supports)) {
        return None;
    }
    let thickness = 2e-5f64.max(1.0f64.max(high - low).max(moments.area.sqrt()) * 1e-4);
    let probe = |start: f64, end: f64| {
        ctx.swept_face_fraction(
            owner,
            floor,
            scale(depth, start - floor_at),
            scale(depth, end - start),
        )
    };
    if probe(low + tolerance, high - tolerance)? > EMPTY
        || probe(mouth_at + sign * tolerance, mouth_at + sign * thickness)? > EMPTY
        || probe(floor_at - sign * tolerance, floor_at - sign * thickness)? < 1.0 - EMPTY
    {
        return None;
    }
    let mut issuer = BodyRefIssuer::default();
    let occurrence = SectionOccurrence::new(
        issuer.issue(None).ok()?,
        frame,
        (low, high),
        section,
        SectionEnds::new(sign > 0.0, sign < 0.0).ok()?,
    )
    .ok()?;
    let geometry = project_section_recess_geometry(&occurrence, &issuer).ok()?;
    let mut defining = walls;
    defining.sort_unstable();
    let mut constituent = defining.clone();
    constituent.push(floor);
    constituent.sort_unstable();
    Some(Candidate {
        defining_faces: defining,
        constituent_faces: constituent,
        mouth: *mouths.iter().min().expect("nonempty"),
        body: owner,
        geometry,
        section_shape: "general",
        feature_kind: "pocket",
    })
}

/// A proved cylindrical-ended pocket as a candidate (`_cylindrical_candidate`): its floor
/// polygon in the run's canonical frame, capped at the floor, the cylinder at the mouth.
pub fn cylindrical_candidate(part: &Part, proof: &CylindricalPocketProof) -> Checked<Candidate> {
    let base = LocalFrame::canonical(proof.run, [0.0; 3])?;
    let reading = part.faces[proof.floor]
        .loops
        .first()
        .and_then(|lp| line_section(part, &lp.edges, &base));
    let Some((section, centre)) = reading else {
        return refuse("cylindrical pocket floor must preserve one polygon");
    };
    let frame = LocalFrame::canonical(base.run, centre)?;
    let Some(floor_centre) = face_center(part, proof.floor) else {
        return refuse("cylindrical pocket floor has no centroid");
    };
    let floor_at = dot(floor_centre, frame.run);
    let sign = if dot(proof.run, frame.run) > 0.0 {
        1
    } else {
        -1
    };
    let points: Vec<V2> = section.boundary().iter().map(|v| v.point).collect();
    let geometry = cylindrical_geometry(
        &proof.into(),
        &frame,
        &section,
        floor_at,
        sign,
        usize::from(sign > 0),
        None,
        false,
    )?;
    let defining = proof.walls.clone();
    let mut constituent = defining.clone();
    constituent.push(proof.floor);
    constituent.sort_unstable();
    let mut defining = defining;
    defining.sort_unstable();
    Ok(Candidate {
        defining_faces: defining,
        constituent_faces: constituent,
        mouth: proof.stock,
        body: proof.owner,
        geometry,
        section_shape: polygonal_shape(&points),
        feature_kind: "pocket",
    })
}

/// A proved U-channel ending on a cylinder as its public geometry, without its temporary probe
/// closure (`cylindrical_channel_geometry`): the rectangular probe domain from the original
/// bounds, its one edge on the mouth published as the opening.
pub fn cylindrical_channel_geometry(
    proof: &CylindricalChannelProof,
) -> Checked<SectionRecessGeometry> {
    let index = |axis: &str| ["x", "y", "z"].iter().position(|&a| a == axis);
    let (Some(r), Some(w)) = (index(&proof.run_axis), index(&proof.width_axis)) else {
        return refuse("axis must be 'x', 'y', or 'z'");
    };
    let d = (0..3)
        .find(|&i| i != r && i != w)
        .expect("three distinct axes");
    let centre = proof.bounds.map(|(lo, hi)| (lo + hi) / 2.0);
    let frame = LocalFrame::principal(&proof.run_axis, centre)?;
    // Construct only the private rectangular domain, using exact original bounds.
    let mut corners = Vec::new();
    for (di, wi) in [(0, 0), (0, 1), (1, 1), (1, 0)] {
        let mut point = centre;
        let pick = |(lo, hi): (f64, f64), k: usize| if k == 0 { lo } else { hi };
        point[d] = pick(proof.bounds[d], di);
        point[w] = pick(proof.bounds[w], wi);
        let relative = subtract(point, frame.origin);
        corners.push([dot(relative, frame.u), dot(relative, frame.v)]);
    }
    let section = PlanarSection::polygon(&corners)?;
    let mouth = if proof.open_sign == 1 {
        proof.bounds[d].1
    } else {
        proof.bounds[d].0
    };
    let points: Vec<V2> = section.boundary().iter().map(|v| v.point).collect();
    let on_mouth: Vec<bool> = points
        .iter()
        .map(|p| (frame.origin[d] + p[0] * frame.u[d] + p[1] * frame.v[d] - mouth).abs() <= 1e-6)
        .collect();
    let n = points.len();
    let openings: Vec<usize> = (0..n)
        .filter(|&i| on_mouth[i] && on_mouth[(i + 1) % n])
        .collect();
    let &[opening] = openings.as_slice() else {
        return refuse("cylindrical channel must preserve one absent lateral support");
    };
    let end_index = proof.cylindrical_end;
    let floor_at = if end_index == 1 {
        proof.run_interval.0
    } else {
        proof.run_interval.1
    };
    cylindrical_geometry(
        &proof.into(),
        &frame,
        &section,
        floor_at,
        if end_index == 1 { -1 } else { 1 },
        end_index,
        Some(opening),
        false,
    )
}

/// A proved plane-envelope passage as its public geometry (`_plane_envelope_geometry`): both
/// observed planes published under one complete 0.002 displacement bound.
pub fn plane_envelope_geometry(
    proof: &PlaneEnvelopePassageProof,
) -> Checked<SectionRecessGeometry> {
    let frame = &proof.frame;
    let r = |v: V3, d: usize| v.map(|c| py::round_to(c, d));
    let public_frame = PassageFrame::new(
        r(frame.origin, 3),
        r(frame.run, 6),
        r(frame.u, 6),
        r(frame.v, 6),
    )?;
    let raw_points: Vec<V2> = proof.section.boundary().iter().map(|v| v.point).collect();
    let public_points: Vec<V2> = raw_points
        .iter()
        .map(|p| p.map(|c| py::round_to(c, 4)))
        .collect();
    let terms = proof
        .terms
        .iter()
        .map(|&(h, g)| PlanarEndTerm::new(py::round_to(h, 6), g.map(|c| py::round_to(c, 6))))
        .collect::<Result<Vec<_>, _>>()?;
    let mut sorted = terms.clone();
    sorted.sort_by(|a, b| {
        py::tuple_order(
            &[a.height(), a.gradient()[0], a.gradient()[1]],
            &[b.height(), b.gradient()[0], b.gradient()[1]],
        )
    });
    let [first, second] = <[PlanarEndTerm; 2]>::try_from(sorted).expect("two terms");
    let surface = PlanarEnvelopeEndSurface::new(
        "plane_envelope",
        if proof.envelope_end == 1 {
            "min"
        } else {
            "max"
        },
        [first, second],
    )?;
    let mut interval = [proof.run_interval.0, proof.run_interval.1];
    interval[proof.envelope_end] = surface.height([0.0, 0.0])?;
    let mut ends = [end("open")?, end("open")?];
    ends[proof.envelope_end] = SectionEnd::new("open", EndSurface::PlanarEnvelope(surface))?;
    let [low_end, high_end] = ends;
    let geometry = SectionRecessGeometry::new(
        "section_recess",
        public_frame.clone(),
        (py::round_to(interval[0], 3), py::round_to(interval[1], 3)),
        SectionProfile::Closed(ClosedSectionProfile::new(
            "closed",
            public_points
                .iter()
                .map(|&p| PassageSectionVertex::new(p, 0.0))
                .collect::<Result<_, _>>()?,
        )?),
        SectionRecessEnds::new(low_end, high_end)?,
    )?;
    // Corresponding vertices induce an affine map on a common triangulation of the convex
    // profiles. Each term's error is affine there; min/max is 1-Lipschitz in those values,
    // including at a moving crease.
    let mut height_error = f64::NEG_INFINITY;
    for (&(h, g), term) in proof.terms.iter().zip(&terms) {
        for (raw, public) in raw_points.iter().zip(&public_points) {
            height_error =
                height_error.max((h + g[0] * raw[0] + g[1] * raw[1] - term.at(*public)?).abs());
        }
    }
    let floor = if proof.envelope_end == 1 {
        proof.run_interval.0
    } else {
        proof.run_interval.1
    };
    height_error = height_error.max((floor - py::round_to(floor, 3)).abs());
    let mut height_bound = floor.abs();
    for &(h, g) in &proof.terms {
        for p in &raw_points {
            height_bound = height_bound.max((h + g[0] * p[0] + g[1] * p[1]).abs());
        }
    }
    let largest = |values: &mut dyn Iterator<Item = f64>| values.fold(f64::NEG_INFINITY, f64::max);
    let pairs = || raw_points.iter().zip(&public_points);
    let dx = largest(&mut pairs().map(|(a, b)| (a[0] - b[0]).abs()));
    let dy = largest(&mut pairs().map(|(a, b)| (a[1] - b[1]).abs()));
    let pf = &public_frame;
    let mut displacement = py::dist(&frame.origin, &pf.origin);
    displacement += largest(&mut raw_points.iter().map(|p| p[0].abs())) * py::dist(&frame.u, &pf.u);
    displacement += largest(&mut raw_points.iter().map(|p| p[1].abs())) * py::dist(&frame.v, &pf.v);
    displacement += height_bound * py::dist(&frame.run, &pf.run);
    displacement += dx * dot(pf.u, pf.u).sqrt();
    displacement += dy * dot(pf.v, pf.v).sqrt();
    displacement += height_error * dot(pf.run, pf.run).sqrt();
    if displacement > 0.002 {
        return refuse("plane envelope projection exceeds whole-occurrence displacement bound");
    }
    Ok(geometry)
}

/// Python's `math.sqrt`, which refuses a negative argument.
fn sqrt(value: f64) -> Checked<f64> {
    if value < 0.0 {
        return refuse("math domain error");
    }
    Ok(value.sqrt())
}

/// Python's tuple order of two vertex chains: vertex by vertex, each by point then bulge.
fn chain_order(a: &[PassageSectionVertex], b: &[PassageSectionVertex]) -> Ordering {
    let flat = |s: &[PassageSectionVertex]| -> Vec<f64> {
        s.iter()
            .flat_map(|v| [v.point[0], v.point[1], v.bulge])
            .collect()
    };
    py::tuple_order(&flat(a), &flat(b))
}

/// The open profile through *chain* (Python's `OpenSectionProfile("open", chain, (chain[-1]
/// .point, chain[0].point), _open_profile_material_side(chain))`).
fn open_profile(chain: Vec<PassageSectionVertex>) -> Checked<OpenSectionProfile> {
    let side = open_profile_material_side(&chain)?;
    let opening = [chain[chain.len() - 1].point, chain[0].point];
    OpenSectionProfile::new("open", chain, opening, Some(side))
}

/// One whole-occurrence error bound for an observed cylindrical termination
/// (`_cylindrical_geometry`).
///
/// The closed section is the private probe domain. For a channel its explicitly identified
/// absent edge (*opening_edge*) is removed before publishing the physical profile. *sign* is
/// the branch (+1 positive), *end_index* the end the cylinder closes (0 low, 1 high).
#[allow(clippy::too_many_arguments)]
pub fn cylindrical_geometry(
    cylinder: &CylinderEnd,
    frame: &LocalFrame,
    section: &PlanarSection,
    floor_at: f64,
    sign: i32,
    end_index: usize,
    opening_edge: Option<usize>,
    planar_open: bool,
) -> Checked<SectionRecessGeometry> {
    let largest = |values: &mut dyn Iterator<Item = f64>| values.fold(f64::NEG_INFINITY, f64::max);
    let smallest = |values: &mut dyn Iterator<Item = f64>| values.fold(f64::INFINITY, f64::min);
    let radius = cylinder.radius;
    let relative = subtract(cylinder.axis_point, frame.origin);
    let mut axis = [
        dot(cylinder.axis_direction, frame.u),
        dot(cylinder.axis_direction, frame.v),
    ];
    let norm = py::hypot(&axis);
    axis = [axis[0] / norm, axis[1] / norm];
    // The larger component, ties to the second.
    let dominant = if axis[0].abs() > axis[1].abs() { 0 } else { 1 };
    if axis[dominant] < 0.0 {
        axis = [-axis[0], -axis[1]];
    }
    let (mut cx, mut cy, cz) = (
        dot(relative, frame.u),
        dot(relative, frame.v),
        dot(relative, frame.run),
    );
    let (native_cx, native_cy) = (cx, cy);
    let along = cx * axis[0] + cy * axis[1];
    cx -= along * axis[0];
    cy -= along * axis[1];
    let raw_points: Vec<V2> = section.boundary().iter().map(|v| v.point).collect();
    // Exact tilted-cylinder roots add an axial linear term and divide the radial root by the
    // in-plane axis norm. Bound that source-model change.
    let source_tilt_error = dot(cylinder.axis_direction, frame.run).abs() / norm
        * largest(
            &mut raw_points
                .iter()
                .map(|p| (axis[0] * (p[0] - native_cx) + axis[1] * (p[1] - native_cy)).abs()),
        )
        + (1.0 / norm - 1.0).abs() * radius;
    let raw_q: Vec<f64> = raw_points
        .iter()
        .map(|p| -axis[1] * (p[0] - cx) + axis[0] * (p[1] - cy))
        .collect();
    let qmax = largest(&mut raw_q.iter().map(|q| q.abs()));
    let (low_q, high_q) = (
        smallest(&mut raw_q.iter().copied()),
        largest(&mut raw_q.iter().copied()),
    );
    let qmin = if low_q <= 0.0 && 0.0 <= high_q {
        0.0
    } else {
        smallest(&mut raw_q.iter().map(|q| q.abs()))
    };
    let raw_roots = [
        sqrt((radius - qmax) * (radius + qmax))?,
        sqrt((radius - qmin) * (radius + qmin))?,
    ];
    let s = f64::from(sign);
    let roof_bounds = raw_roots.map(|root| cz + s * root);
    let envelope = (
        floor_at.min(roof_bounds[0]).min(roof_bounds[1]),
        floor_at.max(roof_bounds[0]).max(roof_bounds[1]),
    );
    let mut issuer = BodyRefIssuer::default();
    let occurrence = SectionOccurrence::new(
        issuer.issue(None)?,
        *frame,
        envelope,
        section.clone(),
        SectionEnds::new(sign > 0, sign < 0)?,
    )?;
    let projected = project_section_recess_geometry(&occurrence, &issuer)?;
    let serialized_axis = axis.map(|c| py::round_to(c, 6));
    let unit_norm = py::hypot(&serialized_axis);
    let unit = serialized_axis.map(|c| c / unit_norm);
    let along = cx * unit[0] + cy * unit[1];
    let axis_point = [
        py::round_to(cx - along * unit[0], 6),
        py::round_to(cy - along * unit[1], 6),
        py::round_to(cz, 6),
    ];
    let surface_radius = py::round_to(radius, 6);
    let surface = CylindricalEndSurface::new(
        "cylinder",
        axis_point,
        serialized_axis,
        surface_radius,
        if sign > 0 { "positive" } else { "negative" },
    )?;
    // Curvature can amplify profile rounding into end-height error. Use the existing public
    // profile's four-decimal allowance, retaining the same whole-occurrence displacement limit
    // rather than widening it.
    let public_profile = ClosedSectionProfile::new(
        "closed",
        raw_points
            .iter()
            .map(|p| PassageSectionVertex::new(p.map(|c| py::round_to(c, 4)), 0.0))
            .collect::<Result<_, _>>()?,
    )?;
    let projected = SectionRecessGeometry::new(
        "section_recess",
        projected.frame().clone(),
        projected.run_interval(),
        SectionProfile::Closed(public_profile),
        projected.ends().clone(),
    )?;
    let public_points: Vec<V2> = projected
        .profile()
        .boundary()
        .iter()
        .map(|v| v.point)
        .collect();
    surface.polygon_height_bounds(&public_points)?;
    // Bound the whole cylindrical patch, not just its vertices. The square-root difference is
    // bounded by the discriminant difference divided by the sum of the minimum roots on the
    // complete original/serialized domains.
    let pairs = || raw_points.iter().zip(&public_points);
    let dx = largest(&mut pairs().map(|(a, b)| (a[0] - b[0]).abs()));
    let dy = largest(&mut pairs().map(|(a, b)| (a[1] - b[1]).abs()));
    // The surface's `_offset`: its axis is already unit to six decimals.
    let public_q: Vec<f64> = public_points
        .iter()
        .map(|p| {
            let [x, y] = serialized_axis;
            (-y * (p[0] - axis_point[0]) + x * (p[1] - axis_point[1])) / py::hypot(&[x, y])
        })
        .collect();
    // Both transverse coordinates are affine on each corresponding polygon edge/triangle.
    // Preserve the paired error instead of separately summing axis, centre and profile
    // perturbations which can cancel one another.
    let convex = |points: &[V2]| {
        let n = points.len();
        (0..n).all(|i| {
            let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
            (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]) >= 0.0
        })
    };
    let qerror = if convex(&raw_points) && convex(&public_points) {
        largest(&mut raw_q.iter().zip(&public_q).map(|(a, b)| (a - b).abs()))
    } else {
        py::dist(&axis, &unit)
            * largest(
                &mut raw_points
                    .iter()
                    .map(|p| py::hypot(&[p[0] - cx, p[1] - cy])),
            )
            + py::hypot(&[dx, dy])
            + py::dist(&[cx, cy], &axis_point[..2])
    };
    let public_qmax = largest(&mut public_q.iter().map(|q| q.abs()));
    let public_root_min = sqrt((surface_radius - public_qmax) * (surface_radius + public_qmax))?;
    let divisor = raw_roots[0].min(raw_roots[1]) + public_root_min;
    if divisor == 0.0 {
        return refuse("float division by zero");
    }
    let mut height_error = (cz - axis_point[2]).abs()
        + ((radius - surface_radius).abs() * (radius + surface_radius)
            + qerror * (qmax + public_qmax))
            / divisor;
    height_error += source_tilt_error;
    let floor_error = (floor_at - py::round_to(floor_at, 3)).abs();
    let pf = projected.frame();
    let mut displacement = py::dist(&frame.origin, &pf.origin);
    let (uu, vv, uv) = (dot(pf.u, pf.u), dot(pf.v, pf.v), dot(pf.u, pf.v));
    // Largest singular value of the two serialized basis columns. This preserves their
    // near-orthogonality instead of adding perpendicular errors linearly.
    let in_plane_stretch = ((uu + vv + ((uu - vv).powi(2) + 4.0 * uv * uv).sqrt()) / 2.0).sqrt();
    displacement += in_plane_stretch * py::hypot(&[dx, dy]);
    displacement += py::dist(&frame.u, &pf.u) * largest(&mut raw_points.iter().map(|p| p[0].abs()));
    displacement += py::dist(&frame.v, &pf.v) * largest(&mut raw_points.iter().map(|p| p[1].abs()));
    displacement += py::dist(&frame.run, &pf.run)
        * (envelope.0.abs().max(envelope.1.abs()) + source_tilt_error);
    displacement += dot(pf.run, pf.run).sqrt() * height_error.max(floor_error);
    if displacement > 0.002 {
        return refuse("serialized cylindrical pocket exceeds whole-occurrence displacement limit");
    }
    let profile = match opening_edge {
        None => projected.profile().clone(),
        Some(opening) => {
            let boundary = projected.profile().boundary();
            let start = (opening + 1) % boundary.len();
            let chain: Vec<PassageSectionVertex> = boundary[start..]
                .iter()
                .chain(&boundary[..start])
                .cloned()
                .collect();
            let reversed: Vec<PassageSectionVertex> = chain.iter().rev().cloned().collect();
            let chain = if chain_order(&reversed, &chain) == Ordering::Less {
                reversed
            } else {
                chain
            };
            SectionProfile::Open(open_profile(chain)?)
        }
    };
    let curved_end = SectionEnd::new("open", EndSurface::Cylindrical(surface.clone()))?;
    let flat_end = end(if planar_open || opening_edge.is_some() {
        "open"
    } else {
        "capped"
    })?;
    let centroid_end = py::round_to(surface.height([0.0, 0.0])?, 3);
    let floor_rounded = py::round_to(floor_at, 3);
    let (interval, ends) = if end_index == 1 {
        ((floor_rounded, centroid_end), (flat_end, curved_end))
    } else {
        ((centroid_end, floor_rounded), (curved_end, flat_end))
    };
    SectionRecessGeometry::new(
        "section_recess",
        projected.frame().clone(),
        interval,
        profile,
        SectionRecessEnds::new(ends.0, ends.1)?,
    )
}

/// A proved open seat as its public geometry (`_seat_geometry`): the actual open arc under a
/// complete 0.002 displacement bound.
pub fn seat_geometry(seat: &CylindricalSeatProof) -> Checked<SectionRecessGeometry> {
    let raw = &seat.frame;
    let r = |v: V3, d: usize| v.map(|c| py::round_to(c, d));
    let frame = PassageFrame::new(r(raw.origin, 3), r(raw.run, 6), r(raw.u, 6), r(raw.v, 6))?;
    let [first, last] = seat.boundary.map(|v| {
        PassageSectionVertex::new(
            v.point.map(|c| py::round_to(c, 4)),
            py::round_to(v.bulge, 12),
        )
    });
    let (first, last) = (first?, last?);
    let original = section_arc(&seat.boundary[0], &seat.boundary[1])?;
    let projected = section_arc(
        &SectionVertex::new(first.point, first.bulge)?,
        &SectionVertex::new(last.point, 0.0)?,
    )?;
    let (Some(original), Some(projected)) = (original, projected) else {
        return refuse("seat publication must retain its physical arc");
    };
    // At equal normalized sweep, circular points differ by centre displacement plus a rotated
    // radial-vector displacement; the sweep-rounding term bounds the remaining rotation
    // everywhere, without sampling only the endpoints.
    let start = seat.boundary[0].point;
    let radial = [start[0] - original.centre[0], start[1] - original.centre[1]];
    let published_radial = [
        first.point[0] - projected.centre[0],
        first.point[1] - projected.centre[1],
    ];
    let mut arc_error = py::dist(&original.centre, &projected.centre)
        + py::hypot(&[
            radial[0] - published_radial[0],
            radial[1] - published_radial[1],
        ]);
    arc_error += projected.radius * (original.sweep - projected.sweep).abs();
    let (t0, t1) = seat.run_interval;
    let interval = (py::round_to(t0, 3), py::round_to(t1, 3));
    let extent = py::hypot(&original.centre) + original.radius;
    let mut displacement = py::dist(&raw.origin, &frame.origin);
    displacement += t0.abs().max(t1.abs()) * py::dist(&raw.run, &frame.run);
    displacement +=
        (t0 - interval.0).abs().max((t1 - interval.1).abs()) * dot(frame.run, frame.run).sqrt();
    displacement += extent * (py::dist(&raw.u, &frame.u) + py::dist(&raw.v, &frame.v));
    displacement += arc_error * (1.0 + 3e-6);
    if displacement > 0.002 {
        return refuse("serialized seat exceeds whole-occurrence displacement limit");
    }
    let forward = vec![first.clone(), last.clone()];
    let backward = vec![
        PassageSectionVertex {
            point: last.point,
            bulge: py::without_negative_zero(-first.bulge),
        },
        PassageSectionVertex {
            point: first.point,
            bulge: 0.0,
        },
    ];
    let chain = if chain_order(&backward, &forward) == Ordering::Less {
        backward
    } else {
        forward
    };
    SectionRecessGeometry::new(
        "section_recess",
        frame,
        interval,
        SectionProfile::Open(open_profile(chain)?),
        SectionRecessEnds::new(end("open")?, end("open")?)?,
    )
}

/// What one floor reader makes of one planar face: `None`, a candidate, or (only the polygonal
/// reader, from a value Python computes outside its guards) a refusal.
pub type FloorReading = Checked<Option<Candidate>>;

/// Each planar face with its three [`floor_readings`], in face order.
pub type PlanarFloorReadings = Vec<(usize, [FloorReading; 3])>;

/// The three floor readers on one face, each asked on its own: obround, polygonal, mixed.
pub fn floor_readings(ctx: &Context<'_>, floor: usize) -> [FloorReading; 3] {
    [
        Ok(one_obround_candidate(ctx, floor)),
        one_polygonal_candidate(ctx, floor),
        Ok(one_mixed_candidate(ctx, floor)),
    ]
}

/// The passage geometry `_candidates` gives a cylindrical passage proof.
pub fn cylindrical_passage_geometry(
    passage: &CylindricalPassageProof,
) -> Checked<SectionRecessGeometry> {
    let end_index = passage.cylindrical_end;
    let floor_at = if end_index == 1 {
        passage.run_interval.0
    } else {
        passage.run_interval.1
    };
    cylindrical_geometry(
        &passage.into(),
        &passage.frame,
        &passage.section,
        floor_at,
        if end_index == 1 { -1 } else { 1 },
        end_index,
        None,
        true,
    )
}

fn section_points(section: &PlanarSection) -> Vec<V2> {
    section.boundary().iter().map(|v| v.point).collect()
}

/// The reading `_candidates` takes of one planar face: the existing proved specific
/// classification first, the general reader only as a fallback for this floor (not a second
/// occurrence of the same pocket). The later readers are asked only when needed.
fn preferred(
    obround: Option<Candidate>,
    polygonal: impl FnOnce() -> FloorReading,
    mixed: impl FnOnce() -> Option<Candidate>,
) -> FloorReading {
    Ok(match obround {
        Some(c) => Some(c),
        None => match polygonal()? {
            Some(c) => Some(c),
            None => mixed(),
        },
    })
}

/// Every candidate on the part (`_candidates`), each once, ordered by constituent faces, shape
/// and mouth: seats, then each planar face's specific reading before the general one, then
/// cylindrical pockets and passages and plane-envelope passages. A proof whose projection
/// refuses is dropped; a refusal outside the projections refuses the whole discovery.
pub fn candidates(ctx: &Context<'_>, surfaces: &EffectiveFaces<'_, '_>) -> Checked<Vec<Candidate>> {
    candidates_reading(ctx, surfaces, |floor| {
        preferred(
            one_obround_candidate(ctx, floor),
            || one_polygonal_candidate(ctx, floor),
            || one_mixed_candidate(ctx, floor),
        )
    })
}

/// [`candidates`] together with every planar face's three [`floor_readings`], in face order,
/// each reader asked once: the candidates take their floor readings from these (the readers
/// answer the same whenever they are asked), so a caller that wants both does not read every
/// floor twice.
pub fn candidates_and_floor_readings(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
) -> (Checked<Vec<Candidate>>, PlanarFloorReadings) {
    let readings: PlanarFloorReadings = (0..ctx.part.faces.len())
        .filter(|&node| is_planar(ctx.part, node))
        .map(|node| (node, floor_readings(ctx, node)))
        .collect();
    let found = candidates_reading(ctx, surfaces, |floor| {
        let [obround, polygonal, mixed] = &readings
            .iter()
            .find(|(node, _)| *node == floor)
            .expect("every planar face was read")
            .1;
        preferred(
            obround.clone().expect("the obround reader never refuses"),
            || polygonal.clone(),
            || mixed.clone().expect("the mixed reader never refuses"),
        )
    });
    (found, readings)
}

/// The candidates, each planar face's reading taken from *floor*.
fn candidates_reading(
    ctx: &Context<'_>,
    surfaces: &EffectiveFaces<'_, '_>,
    mut floor: impl FnMut(usize) -> FloorReading,
) -> Checked<Vec<Candidate>> {
    let part = ctx.part;
    let mut found: Vec<Candidate> = Vec::new();
    let mut add = |candidate: Candidate| {
        if !found.contains(&candidate) {
            found.push(candidate);
        }
    };
    for seat in cylindrical_seat_proofs(ctx) {
        if let Ok(geometry) = seat_geometry(&seat) {
            add(Candidate {
                defining_faces: seat.walls.clone(),
                constituent_faces: seat.walls.clone(),
                mouth: seat.context[0],
                body: seat.owner,
                geometry,
                section_shape: "circular",
                feature_kind: "channel",
            });
        }
    }
    for node in 0..part.faces.len() {
        if !is_planar(part, node) {
            continue;
        }
        if let Some(c) = floor(node)? {
            add(c);
        }
    }
    for proof in cylindrical_pocket_proofs(ctx, surfaces) {
        if let Ok(c) = cylindrical_candidate(part, &proof) {
            add(c);
        }
    }
    for passage in cylindrical_passage_proofs(ctx, surfaces) {
        if let Ok(geometry) = cylindrical_passage_geometry(&passage) {
            let mut walls = passage.walls.clone();
            walls.sort_unstable();
            add(Candidate {
                defining_faces: walls.clone(),
                constituent_faces: walls,
                mouth: passage.planar_context,
                body: passage.owner,
                geometry,
                section_shape: polygonal_shape(&section_points(&passage.section)),
                feature_kind: "passage",
            });
        }
    }
    for passage in plane_envelope_passage_proofs(ctx) {
        if let Ok(geometry) = plane_envelope_geometry(&passage) {
            let mut walls = passage.walls.clone();
            walls.sort_unstable();
            add(Candidate {
                defining_faces: walls.clone(),
                constituent_faces: walls,
                mouth: passage.planar_context,
                body: passage.owner,
                geometry,
                section_shape: polygonal_shape(&section_points(&passage.section)),
                feature_kind: "passage",
            });
        }
    }
    // Python sorts a set; equal keys keep the order found here.
    found.sort_by(|a, b| {
        a.constituent_faces
            .cmp(&b.constituent_faces)
            .then(a.section_shape.cmp(b.section_shape))
            .then(a.mouth.cmp(&b.mouth))
    });
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_are_named_by_their_corners() {
        let square = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
        let kite = [[0.0, 0.0], [1.0, 0.0], [2.0, 1.0], [0.0, 1.0]];
        assert_eq!(polygonal_shape(&square), "rectangular");
        assert_eq!(polygonal_shape(&kite), "polygonal");
        assert_eq!(polygonal_shape(&square[..3]), "triangular");
        assert_eq!(polygonal_shape(&[[0.0, 0.0]; 6]), "hexagonal");
        assert_eq!(polygonal_shape(&[[0.0, 0.0]; 5]), "polygonal");
    }

    #[test]
    fn canonical_directions_point_along_their_largest_component() {
        assert_eq!(canonical([0.0, -2.0, 0.0]), Some([0.0, 1.0, 0.0]));
        // A tie goes to the later axis.
        let tied = canonical([1.0, 0.0, -1.0]).unwrap();
        assert!(tied[2] > 0.0 && tied[0] < 0.0);
        assert_eq!(canonical([0.0; 3]), None);
    }
}
