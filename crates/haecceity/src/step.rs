//! STEP import through `step-io`, mapped into [`Part`].
//!
//! Mirrors `quiddity.import_step_geometry`: geometry only, assemblies flattened, lengths in
//! millimetres (OpenCascade's default import unit).

use std::collections::HashMap;

use step_io::generated::model as m;
use step_io::parser::{Attribute, RawEntity};
use step_io::scene::geometry::{Edge as StepEdge, Face as StepFace, SurfaceKind};

use super::brep::{Edge, Face, Loop, Part, Pcurve, Solid, Source};
use super::geom::{self, Curve, Frame, Surface, V3};
use super::nurbs::{NurbsCurve, NurbsSurface};
use super::sampling::sample_edge;

#[derive(Debug)]
pub enum StepError {
    Parse(String),
    Unsupported(String),
    /// The file parses but its shapes are not all there: no solid or shell at all, or
    /// geometry or topology step-io dropped (missing, malformed or of an unknown type).
    Incomplete(String),
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepError::Parse(s) => write!(f, "STEP parse error: {s}"),
            StepError::Unsupported(s) => write!(f, "unsupported STEP content: {s}"),
            StepError::Incomplete(s) => write!(f, "incomplete STEP geometry: {s}"),
        }
    }
}

impl std::error::Error for StepError {}

/// A rigid placement, rows of `[R | t]` with `t` in millimetres.
pub type Placement = [[f64; 4]; 3];

pub const IDENTITY: Placement = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
];

fn compose(outer: &Placement, inner: &Placement) -> Placement {
    let mut out = [[0.0; 4]; 3];
    for i in 0..3 {
        for j in 0..4 {
            out[i][j] = (0..3).map(|k| outer[i][k] * inner[k][j]).sum::<f64>()
                + if j == 3 { outer[i][3] } else { 0.0 };
        }
    }
    out
}

struct Reader<'a> {
    model: &'a m::StepModel,
    /// Length unit → millimetres.
    to_mm: f64,
    /// Plane-angle unit → radians.
    to_rad: f64,
    /// The placement of the solid instance being read.
    placement: Placement,
}

impl Reader<'_> {
    fn place_point(&self, p: V3) -> V3 {
        let x = &self.placement;
        [0, 1, 2].map(|i| x[i][0] * p[0] + x[i][1] * p[1] + x[i][2] * p[2] + x[i][3])
    }

    fn place_direction(&self, d: V3) -> V3 {
        let x = &self.placement;
        [0, 1, 2].map(|i| x[i][0] * d[0] + x[i][1] * d[1] + x[i][2] * d[2])
    }

    fn point(&self, r: &m::CartesianPointRef) -> Option<V3> {
        let m::CartesianPointRef::CartesianPoint(i) = r else {
            return None;
        };
        let c = &self.model.cartesian_point_arena.get(i.0).coordinates;
        let get = |k: usize| c.get(k).copied().unwrap_or(0.0) * self.to_mm;
        Some(self.place_point([get(0), get(1), get(2)]))
    }

    fn direction(&self, r: &m::DirectionRef) -> Option<V3> {
        let m::DirectionRef::Direction(i) = r else {
            return None;
        };
        let c = &self.model.direction_arena.get(i.0).direction_ratios;
        let get = |k: usize| c.get(k).copied().unwrap_or(0.0);
        geom::unit(self.place_direction([get(0), get(1), get(2)]))
    }

    /// A file direction before the instance placement is applied.
    fn raw_direction(&self, r: &m::DirectionRef) -> Option<V3> {
        let m::DirectionRef::Direction(i) = r else {
            return None;
        };
        let c = &self.model.direction_arena.get(i.0).direction_ratios;
        let get = |k: usize| c.get(k).copied().unwrap_or(0.0);
        geom::unit([get(0), get(1), get(2)])
    }

    /// `AXIS2_PLACEMENT_3D` as OpenCascade builds it: a missing or degenerate reference
    /// direction falls back to `gp_Ax2`'s own choice of x axis for the given z.
    fn frame3(&self, id: m::Axis2Placement3dId) -> Option<Frame> {
        let a = self.model.axis2_placement3d_arena.get(id.0);
        let origin = self.point(&a.location)?;
        let z = match &a.axis {
            Some(d) => self.raw_direction(d)?,
            None => [0.0, 0.0, 1.0],
        };
        let x = a
            .ref_direction
            .as_ref()
            .and_then(|d| self.raw_direction(d))
            .and_then(|r| geom::unit(geom::sub(r, geom::scale(z, geom::dot(r, z)))))
            .unwrap_or_else(|| geom::default_x_axis(z));
        let (x, z) = (self.place_direction(x), self.place_direction(z));
        Some(Frame {
            origin,
            x,
            y: geom::cross(z, x),
            z,
        })
    }

    fn placement(&self, r: &m::Axis2Placement3dRef) -> Option<Frame> {
        let m::Axis2Placement3dRef::Axis2Placement3d(id) = r;
        self.frame3(*id)
    }

    /// The face's surface, or `None` when no form of it resolves.
    fn surface(&self, face: &StepFace<'_>) -> Option<Surface> {
        let mm = self.to_mm;
        match face.surface().kind() {
            SurfaceKind::Plane(p) => self
                .placement(&p.position)
                .map(|frame| Surface::Plane { frame }),
            SurfaceKind::Cylindrical(c) => {
                self.placement(&c.position).map(|frame| Surface::Cylinder {
                    frame,
                    radius: c.radius * mm,
                })
            }
            SurfaceKind::Conical(c) => self.placement(&c.position).map(|frame| Surface::Cone {
                frame,
                radius: c.radius * mm,
                semi_angle: c.semi_angle * self.to_rad,
            }),
            SurfaceKind::Spherical(s) => self.placement(&s.position).map(|frame| Surface::Sphere {
                frame,
                radius: s.radius * mm,
            }),
            SurfaceKind::Toroidal(t) => self.placement(&t.position).map(|frame| Surface::Torus {
                frame,
                major: t.major_radius * mm,
                minor: t.minor_radius * mm,
            }),
            // Every B-spline form, rational ones included (written as complex entities), is
            // one GeomAbs_BSplineSurface to OpenCascade.
            SurfaceKind::BSpline(_)
            | SurfaceKind::BSplineWithKnots(_)
            | SurfaceKind::QuasiUniform(_)
            | SurfaceKind::Uniform(_)
            | SurfaceKind::Other("RATIONAL_B_SPLINE_SURFACE" | "COMPLEX") => {
                self.freeform(face, "BSPLINE")
            }
            SurfaceKind::Bezier(_) => self.freeform(face, "BEZIER"),
            SurfaceKind::LinearExtrusion(_) => self.freeform(face, "EXTRUSION"),
            SurfaceKind::Revolution(_) => self.freeform(face, "REVOLUTION"),
            _ => self.freeform(face, "OTHER"),
        }
    }

    /// The point of a `VERTEX_LOOP` bound, if that is what this bound is.
    fn vertex_loop(&self, bound: &step_io::scene::geometry::Bound<'_>) -> Option<V3> {
        let r = match bound.key() {
            m::EntityKey::FaceBound(i) => &self.model.face_bound_arena.get(i.0).bound,
            m::EntityKey::FaceOuterBound(i) => &self.model.face_outer_bound_arena.get(i.0).bound,
            _ => return None,
        };
        let m::LoopRef::VertexLoop(id) = r else {
            return None;
        };
        let m::VertexRef::VertexPoint(v) = &self.model.vertex_loop_arena.get(id.0).loop_vertex
        else {
            return None;
        };
        let m::PointRef::CartesianPoint(c) =
            &self.model.vertex_point_arena.get(v.0).vertex_geometry
        else {
            return None;
        };
        self.point(&m::CartesianPointRef::CartesianPoint(*c))
    }

    /// The pcurves the file gives for this face's edges, by edge index, in the surface's
    /// parameter units (lengths scaled to millimetres, angles to radians).
    fn pcurves(
        &self,
        face: &StepFace<'_>,
        surface: &Surface,
        edge_index: &HashMap<m::EntityKey, usize>,
        edges: &[Edge],
    ) -> Vec<(usize, Pcurve)> {
        let Ok(basis) = m::SurfaceRef::from_any(face.surface().key()) else {
            return Vec::new();
        };
        let (su, sv) = match surface {
            Surface::Plane { .. } => (self.to_mm, self.to_mm),
            Surface::Cylinder { .. } => (self.to_rad, self.to_mm),
            // STEP's cone v is axial height; OpenCascade's runs along the slant.
            Surface::Cone { semi_angle, .. } => (self.to_rad, self.to_mm / semi_angle.cos()),
            Surface::Sphere { .. } | Surface::Torus { .. } => (self.to_rad, self.to_rad),
            Surface::Freeform { .. } | Surface::Other { .. } => return Vec::new(),
        };
        let scale = |(u, v): (f64, f64)| (u * su, v * sv);
        let mut out = Vec::new();
        for bound in face.bounds() {
            for edge in bound.edges() {
                let Some(&index) = edge_index.get(&edge.key()) else {
                    continue;
                };
                let vertex = |v: Option<step_io::scene::geometry::Vertex<'_>>| {
                    v.and_then(|v| v.point())
                        .map(|p| self.place_point(geom::scale(p.xyz(), self.to_mm)))
                };
                let (Some(a), Some(b)) = (vertex(edge.start()), vertex(edge.end())) else {
                    continue;
                };
                let associated = match &raw_edge_curve(self.model, &edge) {
                    m::CurveRef::SurfaceCurve(i) => {
                        &self.model.surface_curve_arena.get(i.0).associated_geometry
                    }
                    m::CurveRef::SeamCurve(i) => {
                        &self.model.seam_curve_arena.get(i.0).associated_geometry
                    }
                    _ => continue,
                };
                for g in associated {
                    let m::PcurveOrSurfaceRef::Pcurve(p) = g else {
                        continue;
                    };
                    let pcurve = self.model.pcurve_arena.get(p.0);
                    if pcurve.basis_surface != basis {
                        continue;
                    }
                    let m::DefinitionalRepresentationRef::DefinitionalRepresentation(d) =
                        &pcurve.reference_to_curve
                    else {
                        continue;
                    };
                    for item in &self.model.definitional_representation_arena.get(d.0).items {
                        if let Some((point, dir)) = self.line_2d(item) {
                            let dir = (dir.0 * su, dir.1 * sv);
                            out.push((
                                index,
                                Pcurve::Line {
                                    point: scale(point),
                                    dir,
                                },
                            ));
                        } else if let Some(points) = self.bspline_poles_2d(item) {
                            let points: Vec<(f64, f64)> = points.into_iter().map(scale).collect();
                            // OpenCascade boxes the pcurve trimmed to the edge. A clamped curve's
                            // end poles are its end points, so the polygon stands for the edge
                            // only when they land on the edge's vertices.
                            let length: f64 = edges[index]
                                .samples
                                .windows(2)
                                .map(|w| geom::dist(w[0], w[1]))
                                .sum();
                            if spans_edge(surface, &points, (a, b), length) {
                                out.push((index, Pcurve::Poles(points)));
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// A (rational) B-spline curve written as a STEP complex entity — `B_SPLINE_CURVE` +
    /// `B_SPLINE_CURVE_WITH_KNOTS` (+ `RATIONAL_B_SPLINE_CURVE`) — in NURBS form.
    fn complex_bspline(&self, parts: &[m::UnitPart]) -> Option<Curve> {
        let (degree, refs) = parts.iter().find_map(|p| match p {
            m::UnitPart::BSplineCurve {
                degree,
                control_points_list,
                ..
            } => Some((*degree, control_points_list)),
            _ => None,
        })?;
        let (mults, values) = parts.iter().find_map(|p| match p {
            m::UnitPart::BSplineCurveWithKnots {
                knot_multiplicities,
                knots,
                ..
            } => Some((knot_multiplicities, knots)),
            _ => None,
        })?;
        let points: Vec<V3> = refs.iter().map(|r| self.point(r)).collect::<Option<_>>()?;
        let weights = parts
            .iter()
            .find_map(|p| match p {
                m::UnitPart::RationalBSplineCurve { weights_data } => Some(weights_data.clone()),
                _ => None,
            })
            .unwrap_or_else(|| vec![1.0; points.len()]);
        let knots: Vec<f64> = values
            .iter()
            .zip(mults)
            .flat_map(|(k, &m)| std::iter::repeat_n(*k, usize::try_from(m).unwrap_or(0)))
            .collect();
        NurbsCurve::new(usize::try_from(degree).ok()?, points, weights, knots).map(Curve::Nurbs)
    }

    /// A 2D `LINE`'s point and direction, unscaled.
    fn line_2d(&self, item: &m::RepresentationItemRef) -> Option<((f64, f64), (f64, f64))> {
        let m::RepresentationItemRef::Line(i) = item else {
            return None;
        };
        let line = self.model.line_arena.get(i.0);
        let m::CartesianPointRef::CartesianPoint(p) = &line.pnt else {
            return None;
        };
        let c = &self.model.cartesian_point_arena.get(p.0).coordinates;
        let m::VectorRef::Vector(v) = &line.dir else {
            return None;
        };
        let m::DirectionRef::Direction(d) = &self.model.vector_arena.get(v.0).orientation else {
            return None;
        };
        let r = &self.model.direction_arena.get(d.0).direction_ratios;
        Some(((*c.first()?, *c.get(1)?), (*r.first()?, *r.get(1)?)))
    }

    fn bspline_poles_2d(&self, item: &m::RepresentationItemRef) -> Option<Vec<(f64, f64)>> {
        let refs = match item {
            m::RepresentationItemRef::BSplineCurveWithKnots(i) => {
                &self
                    .model
                    .b_spline_curve_with_knots_arena
                    .get(i.0)
                    .control_points_list
            }
            m::RepresentationItemRef::Complex(c) => self
                .model
                .complex_unit_arena
                .get(c.0)
                .parts
                .iter()
                .find_map(|part| match part {
                    m::UnitPart::BSplineCurve {
                        control_points_list,
                        ..
                    } => Some(control_points_list),
                    _ => None,
                })?,
            _ => return None,
        };
        refs.iter()
            .map(|r| {
                let m::CartesianPointRef::CartesianPoint(i) = r else {
                    return None;
                };
                let c = &self.model.cartesian_point_arena.get(i.0).coordinates;
                Some((*c.first()?, *c.get(1)?))
            })
            .collect()
    }

    fn freeform(&self, face: &StepFace<'_>, kind: &'static str) -> Option<Surface> {
        let n = face.to_nurbs()?;
        let place = |row: &Vec<[f64; 3]>| -> Vec<V3> {
            row.iter()
                .map(|p| self.place_point(geom::scale(*p, self.to_mm)))
                .collect()
        };
        let surface = NurbsSurface::new(
            n.degree_u,
            n.degree_v,
            n.control_points.iter().map(place).collect(),
            n.weights,
            n.knots_u,
            n.knots_v,
        )?;
        Some(Surface::Freeform {
            kind,
            surface: Box::new(surface),
        })
    }

    /// The exact 3D curve under a curve reference, looking through surface/seam curves.
    fn curve(&self, r: &m::CurveRef) -> Option<Curve> {
        let mm = self.to_mm;
        match r {
            m::CurveRef::Line(i) => {
                let l = self.model.line_arena.get(i.0);
                let origin = self.point(&l.pnt)?;
                let m::VectorRef::Vector(v) = &l.dir else {
                    return None;
                };
                let v = self.model.vector_arena.get(v.0);
                let dir = self.direction(&v.orientation)?;
                Some(Curve::Line { origin, dir })
            }
            m::CurveRef::Circle(i) => {
                let c = self.model.circle_arena.get(i.0);
                let m::Axis2PlacementRef::Axis2Placement3d(p) = &c.position else {
                    return None;
                };
                Some(Curve::Circle {
                    frame: self.frame3(*p)?,
                    radius: c.radius * mm,
                })
            }
            m::CurveRef::Ellipse(i) => {
                let e = self.model.ellipse_arena.get(i.0);
                let m::Axis2PlacementRef::Axis2Placement3d(p) = &e.position else {
                    return None;
                };
                Some(Curve::Ellipse {
                    frame: self.frame3(*p)?,
                    major: e.semi_axis_1 * mm,
                    minor: e.semi_axis_2 * mm,
                })
            }
            m::CurveRef::Complex(c) => {
                self.complex_bspline(&self.model.complex_unit_arena.get(c.0).parts)
            }
            m::CurveRef::SurfaceCurve(i) => {
                self.curve(&self.model.surface_curve_arena.get(i.0).curve_3d)
            }
            m::CurveRef::SeamCurve(i) => self.curve(&self.model.seam_curve_arena.get(i.0).curve_3d),
            _ => None,
        }
    }

    /// *vertex_id* numbers the vertices of the instance being read. The flag is `false` when
    /// the edge's curve did not resolve and its chord stands in for it.
    fn edge(
        &self,
        edge: &StepEdge<'_>,
        vertex_id: &mut impl FnMut(m::EntityKey) -> usize,
    ) -> Result<(Edge, bool), StepError> {
        let vertex = |v: Option<step_io::scene::geometry::Vertex<'_>>| -> Option<V3> {
            let p = v?.point()?.xyz();
            Some(self.place_point(geom::scale(p, self.to_mm)))
        };
        let (Some(start), Some(end)) = (vertex(edge.start()), vertex(edge.end())) else {
            return Err(StepError::Unsupported(
                "edge without cartesian vertices".into(),
            ));
        };
        let (Some(v0), Some(v1)) = (edge.start(), edge.end()) else {
            unreachable!("both vertices resolved above")
        };
        let vertices = (vertex_id(v0.key()), vertex_id(v1.key()));
        // Exact analytic curves first; otherwise the whole curve in NURBS form, which the edge
        // is cut from by inverting its vertices (wrapping round a closed curve if it must). The
        // edge-bounded NURBS step-io offers is the last resort: it cuts the short span out of a
        // nearly closed curve even when the edge runs the long way round.
        let to_curve = |n: step_io::scene::NurbsCurve| {
            let points = n
                .control_points
                .iter()
                .map(|p| self.place_point(geom::scale(*p, self.to_mm)))
                .collect();
            NurbsCurve::new(n.degree, points, n.weights, n.knots).map(Curve::Nurbs)
        };
        let raw = raw_edge_curve(self.model, edge);
        let curve = self
            .curve(&raw)
            .or_else(|| edge.curve().to_nurbs().and_then(to_curve))
            .or_else(|| edge.to_nurbs().and_then(to_curve));
        let resolved = curve.is_some();
        // A closed edge has no chord to stand in for its curve.
        if !resolved && vertices.0 == vertices.1 {
            let model = self.model;
            let kind = format!(
                "{:?}",
                match &raw {
                    m::CurveRef::SurfaceCurve(i) => &model.surface_curve_arena.get(i.0).curve_3d,
                    m::CurveRef::SeamCurve(i) => &model.seam_curve_arena.get(i.0).curve_3d,
                    other => other,
                }
            );
            let kind = kind.split('(').next().unwrap_or_default();
            return Err(StepError::Unsupported(format!(
                "a closed edge's curve ({kind}) does not resolve"
            )));
        }
        // An open edge's curve no form resolves is replaced by its chord, which keeps the loop
        // closed where dropping the edge would open it; the part records the substitution.
        let curve = curve.unwrap_or(Curve::Line {
            origin: start,
            dir: geom::sub(end, start),
        });
        let samples = sample_edge(
            &curve,
            start,
            end,
            edge.same_sense(),
            vertices.0 == vertices.1,
        );
        let edge = Edge {
            curve,
            start,
            end,
            vertices,
            same_sense: edge.same_sense(),
            samples,
        };
        Ok((edge, resolved))
    }
}

/// Whether a pcurve's end poles map onto the edge's two vertices (in either order).
fn spans_edge(surface: &Surface, poles: &[(f64, f64)], (a, b): (V3, V3), length: f64) -> bool {
    let (Some(&first), Some(&last)) = (poles.first(), poles.last()) else {
        return false;
    };
    let (p, q) = (
        surface.value(first.0, first.1),
        surface.value(last.0, last.1),
    );
    // Files approximate their pcurves; a curve longer than its edge misses a vertex by a large
    // fraction of the edge's length, an approximation by far less.
    let tol = (0.01 * length).max(1e-4);
    let near = |x: V3, y: V3| geom::dist(x, y) <= tol;
    (near(p, a) && near(q, b)) || (near(p, b) && near(q, a))
}

/// Whether each void shell of a solid is used reversed (`ORIENTED_CLOSED_SHELL` `.F.`).
fn void_orientations(
    model: &m::StepModel,
    solid: &step_io::scene::geometry::Solid<'_>,
) -> Vec<bool> {
    let m::EntityKey::BrepWithVoids(id) = solid.key() else {
        return Vec::new();
    };
    model
        .brep_with_voids_arena
        .get(id.0)
        .voids
        .iter()
        .map(|v| match v {
            m::OrientedClosedShellRef::OrientedClosedShell(o) => {
                !model.oriented_closed_shell_arena.get(o.0).orientation
            }
            m::OrientedClosedShellRef::Complex(_) => false,
        })
        .collect()
}

/// The key of a representation named on one side of a `SHAPE_REPRESENTATION_RELATIONSHIP`,
/// for the shape-carrying kinds.
fn rep_key(r: &m::RepresentationOrRepresentationReferenceRef) -> Option<m::EntityKey> {
    use m::RepresentationOrRepresentationReferenceRef as R;
    Some(match r {
        R::ShapeRepresentation(i) => m::EntityKey::ShapeRepresentation(*i),
        R::AdvancedBrepShapeRepresentation(i) => m::EntityKey::AdvancedBrepShapeRepresentation(*i),
        R::ManifoldSurfaceShapeRepresentation(i) => {
            m::EntityKey::ManifoldSurfaceShapeRepresentation(*i)
        }
        _ => return None,
    })
}

/// Surface models OpenCascade transfers: those listed by a representation that a product shape
/// uses, directly or through a plain `SHAPE_REPRESENTATION_RELATIONSHIP`. An unreferenced model
/// is construction geometry the importer never reaches.
fn reachable_surface_models(model: &m::StepModel, rg: &step_io::RefGraph) -> Vec<usize> {
    let used_directly = |rep: m::EntityKey| {
        rg.referrers(rep)
            .iter()
            .any(|r| matches!(r, m::EntityKey::ShapeDefinitionRepresentation(_)))
    };
    let used = |rep: m::EntityKey| {
        used_directly(rep)
            || rg.referrers(rep).iter().any(|r| {
                let m::EntityKey::ShapeRepresentationRelationship(i) = r else {
                    return false;
                };
                let srr = model.shape_representation_relationship_arena.get(i.0);
                [&srr.rep_1, &srr.rep_2]
                    .into_iter()
                    .filter_map(rep_key)
                    .any(|k| k != rep && used_directly(k))
            })
    };
    (0..model.shell_based_surface_model_arena.items.len())
        .filter(|&i| {
            let key = m::EntityKey::ShellBasedSurfaceModel(m::ShellBasedSurfaceModelId(i));
            rg.referrers(key).iter().any(|&rep| used(rep))
        })
        .collect()
}

/// The product definitions whose shape lists surface model *index*, directly or through a
/// plain `SHAPE_REPRESENTATION_RELATIONSHIP`.
fn surface_model_owners(
    model: &m::StepModel,
    rg: &step_io::RefGraph,
    index: usize,
) -> Vec<m::EntityKey> {
    let key = m::EntityKey::ShellBasedSurfaceModel(m::ShellBasedSurfaceModelId(index));
    let mut reps: Vec<m::EntityKey> = rg.referrers(key).to_vec();
    for rep in reps.clone() {
        for r in rg.referrers(rep) {
            if let m::EntityKey::ShapeRepresentationRelationship(i) = r {
                let srr = model.shape_representation_relationship_arena.get(i.0);
                reps.extend([&srr.rep_1, &srr.rep_2].into_iter().filter_map(rep_key));
            }
        }
    }
    let mut owners = Vec::new();
    for rep in reps {
        for r in rg.referrers(rep) {
            let m::EntityKey::ShapeDefinitionRepresentation(i) = r else {
                continue;
            };
            let sdr = model.shape_definition_representation_arena.get(i.0);
            let m::RepresentedDefinitionRef::ProductDefinitionShape(pds) = &sdr.definition else {
                continue;
            };
            match model.product_definition_shape_arena.get(pds.0).definition {
                m::CharacterizedDefinitionRef::ProductDefinition(d) => {
                    owners.push(m::EntityKey::ProductDefinition(d))
                }
                m::CharacterizedDefinitionRef::ProductDefinitionWithAssociatedDocuments(d) => {
                    owners.push(m::EntityKey::ProductDefinitionWithAssociatedDocuments(d))
                }
                _ => {}
            }
        }
    }
    owners
}

/// Every solid reachable from an assembly definition, with its accumulated placement and the
/// index in *placed* of the placed definition it belongs to.
fn collect_instances<'m>(
    def: step_io::scene::product::ProductDef<'m>,
    placement: Placement,
    to_mm: f64,
    out: &mut Vec<(
        step_io::scene::geometry::Solid<'m>,
        Placement,
        Option<usize>,
    )>,
    placed: &mut Vec<(step_io::scene::product::ProductDef<'m>, Placement)>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    placed.push((def, placement));
    for solid in def.solids() {
        out.push((solid, placement, Some(placed.len() - 1)));
    }
    for occurrence in def.occurrences() {
        let Some(child) = occurrence.definition() else {
            continue;
        };
        let local = occurrence
            .transform()
            .and_then(|t| t.matrix())
            .map_or(IDENTITY, |mtx| {
                [0, 1, 2].map(|i| [mtx[i][0], mtx[i][1], mtx[i][2], mtx[i][3] * to_mm])
            });
        collect_instances(
            child,
            compose(&placement, &local),
            to_mm,
            out,
            placed,
            depth + 1,
        );
    }
}

fn raw_edge_curve(model: &m::StepModel, edge: &StepEdge<'_>) -> m::CurveRef {
    let m::EntityKey::EdgeCurve(id) = edge.key() else {
        unreachable!("edges are EDGE_CURVEs")
    };
    model.edge_curve_arena.get(id.0).edge_geometry.clone()
}

/// The file instance (`#N`) behind each face, edge and product definition step-io read.
///
/// step-io keeps no instance ids: it fills each arena of a simple entity type in ascending id
/// order over the instances it keeps, so arena index *k* of a type is the *k*-th kept instance
/// of that type (kept: not in the report's dropped list; the only instances its normalization
/// adds are `COLOUR`s, numbered after every instance of the file). The map is rebuilt from the
/// raw graph on that basis and every face and edge is checked against its record before its id
/// is given out (see [`FileIds::face`], [`FileIds::edge`]); a disagreement is an error, never a
/// guess.
struct FileIds<'a> {
    graph: &'a std::collections::BTreeMap<u64, step_io::parser::RawEntity>,
    model: &'a m::StepModel,
    /// Kept instances of each simple type below, in ascending id order: arena index → `#N`.
    by_type: HashMap<&'static str, Vec<u64>>,
    /// Faces and edges already checked.
    checked: HashMap<m::EntityKey, u64>,
}

/// The entity types whose instance ids the reader gives out.
const ID_TYPES: [&str; 6] = [
    "ADVANCED_FACE",
    "FACE_SURFACE",
    "EDGE_CURVE",
    "PRODUCT_DEFINITION",
    "PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS",
    "PRODUCT_DEFINITION_SHAPE",
];

impl<'a> FileIds<'a> {
    fn new(
        graph: &'a step_io::parser::Graph,
        model: &'a m::StepModel,
        report: &step_io::Report,
    ) -> Self {
        let dropped: std::collections::HashSet<u64> =
            report.dropped.iter().map(|(id, _)| *id).collect();
        let mut by_type: HashMap<&'static str, Vec<u64>> = HashMap::new();
        for (&id, entity) in &graph.entities {
            if let step_io::parser::RawEntity::Simple { name, .. } = entity
                && let Some(&kind) = ID_TYPES.iter().find(|&&t| t == name)
                && !dropped.contains(&id)
            {
                by_type.entry(kind).or_default().push(id);
            }
        }
        FileIds {
            graph: &graph.entities,
            model,
            by_type,
            checked: HashMap::new(),
        }
    }

    /// The `#N` of a face, edge, product definition or product definition shape.
    fn id(&self, key: m::EntityKey) -> Result<u64, StepError> {
        let (kind, index) = match key {
            m::EntityKey::AdvancedFace(i) => ("ADVANCED_FACE", i.0),
            m::EntityKey::FaceSurface(i) => ("FACE_SURFACE", i.0),
            m::EntityKey::EdgeCurve(i) => ("EDGE_CURVE", i.0),
            m::EntityKey::ProductDefinition(i) => ("PRODUCT_DEFINITION", i.0),
            m::EntityKey::ProductDefinitionWithAssociatedDocuments(i) => {
                ("PRODUCT_DEFINITION_WITH_ASSOCIATED_DOCUMENTS", i.0)
            }
            m::EntityKey::ProductDefinitionShape(i) => ("PRODUCT_DEFINITION_SHAPE", i.0),
            other => {
                return Err(StepError::Unsupported(format!(
                    "no file instance is kept for {}",
                    step_name(other)
                )));
            }
        };
        self.by_type
            .get(kind)
            .and_then(|ids| ids.get(index))
            .copied()
            .ok_or_else(|| {
                StepError::Unsupported(format!("{kind} {index} read has no instance in the file"))
            })
    }

    /// The `#N` of a face, checked against its record: entity type, bound count, the surface's
    /// entity type, `same_sense`, the surface's location point (its placement's, an axis's, or a
    /// B-spline's first control point) and the `EDGE_CURVE`s of its loops. A surface with no
    /// location point of its own (`SURFACE_OF_LINEAR_EXTRUSION`, `OFFSET_SURFACE`,
    /// `RECTANGULAR_TRIMMED_SURFACE` and other kinds `raw_location` does not name) has no
    /// position check: there the loops' `EDGE_CURVE`s, whose vertices [`FileIds::edge`] checks
    /// by position, are what tells two such faces apart.
    fn face(&mut self, face: &StepFace<'_>) -> Result<u64, StepError> {
        let key = face.key();
        if let Some(&id) = self.checked.get(&key) {
            return Ok(id);
        }
        let id = self.id(key)?;
        self.check_face(face, id).map_err(|why| {
            StepError::Unsupported(format!(
                "face {} read as #{id} does not match that instance: {why}",
                step_name(key)
            ))
        })?;
        self.checked.insert(key, id);
        Ok(id)
    }

    /// The `#N` of an edge, checked against its record: the vertices' and the curve's entity
    /// types, `same_sense` and both vertices' coordinates.
    fn edge(&mut self, edge: &StepEdge<'_>) -> Result<u64, StepError> {
        let key = edge.key();
        if let Some(&id) = self.checked.get(&key) {
            return Ok(id);
        }
        let id = self.id(key)?;
        self.check_edge(edge, id).map_err(|why| {
            StepError::Unsupported(format!(
                "EDGE_CURVE read as #{id} does not match that instance: {why}"
            ))
        })?;
        self.checked.insert(key, id);
        Ok(id)
    }

    /// The `product_definition_shape` a part's shape is represented through: the one of its
    /// definition a `shape_definition_representation` uses. Its id is checked to name the
    /// definition it is read for.
    fn definition_shape(
        &self,
        def: &step_io::scene::product::ProductDef<'_>,
        rg: &step_io::RefGraph,
    ) -> Result<u64, String> {
        let me = def.key();
        let mut found = Vec::new();
        for r in rg.referrers(me) {
            let m::EntityKey::ProductDefinitionShape(i) = *r else {
                continue;
            };
            let pds = self.model.product_definition_shape_arena.get(i.0);
            let names_me = match pds.definition {
                m::CharacterizedDefinitionRef::ProductDefinition(d) => {
                    m::EntityKey::ProductDefinition(d) == me
                }
                m::CharacterizedDefinitionRef::ProductDefinitionWithAssociatedDocuments(d) => {
                    m::EntityKey::ProductDefinitionWithAssociatedDocuments(d) == me
                }
                _ => false,
            };
            let represented = rg
                .referrers(*r)
                .iter()
                .any(|s| matches!(s, m::EntityKey::ShapeDefinitionRepresentation(_)));
            if names_me && represented {
                found.push(*r);
            }
        }
        let [shape] = found[..] else {
            return Err(format!(
                "{} product_definition_shapes with a shape representation",
                found.len()
            ));
        };
        let (id, definition) = (
            self.id(shape).map_err(|e| e.to_string())?,
            self.id(me).map_err(|e| e.to_string())?,
        );
        match self.attribute(id, 2) {
            Some(Attribute::EntityRef(d)) if *d == definition => Ok(id),
            _ => Err(format!(
                "#{id} read as the shape of #{definition} does not name it"
            )),
        }
    }

    /// Attribute *index* of simple instance *id*.
    fn attribute(&self, id: u64, index: usize) -> Option<&'a Attribute> {
        match self.graph.get(&id)? {
            RawEntity::Simple { attributes, .. } => attributes.get(index),
            RawEntity::Complex { .. } => None,
        }
    }

    /// The entity type of instance *id*, `COMPLEX` for a complex instance.
    fn kind(&self, id: u64) -> Option<&'a str> {
        Some(match self.graph.get(&id)? {
            RawEntity::Simple { name, .. } => name,
            RawEntity::Complex { .. } => "COMPLEX",
        })
    }

    /// The instance attribute *index* of *id* refers to.
    fn reference(&self, id: u64, index: usize) -> Option<u64> {
        match self.attribute(id, index)? {
            Attribute::EntityRef(r) => Some(*r),
            _ => None,
        }
    }

    /// A `CARTESIAN_POINT`'s coordinates as written.
    fn raw_point(&self, id: u64) -> Option<Vec<f64>> {
        if self.kind(id)? != "CARTESIAN_POINT" {
            return None;
        }
        let Attribute::List(coordinates) = self.attribute(id, 1)? else {
            return None;
        };
        coordinates
            .iter()
            .map(|c| match c {
                Attribute::Real(x) => Some(*x),
                Attribute::Integer(n) => Some(*n as f64),
                _ => None,
            })
            .collect()
    }

    /// The location point of surface instance *id* as written (see [`FileIds::face`]).
    fn raw_location(&self, id: u64) -> Option<Vec<f64>> {
        let first_pole = |list: &Attribute| match list {
            Attribute::List(rows) => match rows.first()? {
                Attribute::List(row) => match row.first()? {
                    Attribute::EntityRef(p) => self.raw_point(*p),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        match self.graph.get(&id)? {
            RawEntity::Simple { name, .. } => match name.as_str() {
                "PLANE"
                | "CYLINDRICAL_SURFACE"
                | "CONICAL_SURFACE"
                | "SPHERICAL_SURFACE"
                | "TOROIDAL_SURFACE"
                | "DEGENERATE_TOROIDAL_SURFACE" => {
                    let axis = self.reference(id, 1)?;
                    self.raw_point(self.reference(axis, 1)?)
                }
                "SURFACE_OF_REVOLUTION" => {
                    let axis = self.reference(id, 2)?;
                    self.raw_point(self.reference(axis, 1)?)
                }
                "B_SPLINE_SURFACE"
                | "B_SPLINE_SURFACE_WITH_KNOTS"
                | "BEZIER_SURFACE"
                | "UNIFORM_SURFACE"
                | "QUASI_UNIFORM_SURFACE" => first_pole(self.attribute(id, 3)?),
                _ => None,
            },
            RawEntity::Complex { parts, .. } => {
                let part = parts.iter().find(|p| p.name == "B_SPLINE_SURFACE")?;
                first_pole(part.attributes.get(2)?)
            }
        }
    }

    fn typed_point(&self, r: &m::CartesianPointRef) -> Option<Vec<f64>> {
        let m::CartesianPointRef::CartesianPoint(i) = r else {
            return None;
        };
        Some(
            self.model
                .cartesian_point_arena
                .get(i.0)
                .coordinates
                .clone(),
        )
    }

    /// The location point of a face's surface as step-io read it.
    fn typed_location(&self, face: &StepFace<'_>) -> Option<Vec<f64>> {
        let model = self.model;
        let placed = |r: &m::Axis2Placement3dRef| {
            let m::Axis2Placement3dRef::Axis2Placement3d(i) = r;
            self.typed_point(&model.axis2_placement3d_arena.get(i.0).location)
        };
        let first =
            |rows: &Vec<Vec<m::CartesianPointRef>>| self.typed_point(rows.first()?.first()?);
        match face.surface().kind() {
            SurfaceKind::Plane(s) => placed(&s.position),
            SurfaceKind::Cylindrical(s) => placed(&s.position),
            SurfaceKind::Conical(s) => placed(&s.position),
            SurfaceKind::Spherical(s) => placed(&s.position),
            SurfaceKind::Toroidal(s) => placed(&s.position),
            SurfaceKind::Revolution(s) => {
                let m::Axis1PlacementRef::Axis1Placement(i) = &s.axis_position;
                self.typed_point(&model.axis1_placement_arena.get(i.0).location)
            }
            SurfaceKind::BSpline(s) => first(&s.control_points_list),
            SurfaceKind::BSplineWithKnots(s) => first(&s.control_points_list),
            SurfaceKind::QuasiUniform(s) => first(&s.control_points_list),
            SurfaceKind::Uniform(s) => first(&s.control_points_list),
            SurfaceKind::Bezier(s) => first(&s.control_points_list),
            SurfaceKind::LinearExtrusion(_) | SurfaceKind::Other(_) => match face.surface().key() {
                m::EntityKey::DegenerateToroidalSurface(i) => {
                    placed(&model.degenerate_toroidal_surface_arena.get(i.0).position)
                }
                m::EntityKey::ComplexUnit(i) => model
                    .complex_unit_arena
                    .get(i.0)
                    .parts
                    .iter()
                    .find_map(|p| match p {
                        m::UnitPart::BSplineSurface {
                            control_points_list,
                            ..
                        } => first(control_points_list),
                        _ => None,
                    }),
                _ => None,
            },
        }
    }

    fn check_face(&self, face: &StepFace<'_>, id: u64) -> Result<(), String> {
        let (bounds, surface, same_sense) = match face.key() {
            m::EntityKey::AdvancedFace(i) => {
                let f = self.model.advanced_face_arena.get(i.0);
                (f.bounds.len(), f.face_geometry.entity_key(), f.same_sense)
            }
            m::EntityKey::FaceSurface(i) => {
                let f = self.model.face_surface_arena.get(i.0);
                (f.bounds.len(), f.face_geometry.entity_key(), f.same_sense)
            }
            other => return Err(format!("a face of type {}", step_name(other))),
        };
        let kind = self.kind(id).unwrap_or("nothing");
        if kind != step_name(face.key()) {
            return Err(format!("it is {kind}"));
        }
        match self.attribute(id, 1) {
            Some(Attribute::List(l)) if l.len() == bounds => {}
            _ => return Err(format!("its bound count is not {bounds}")),
        }
        let surface_id = self.reference(id, 2).ok_or("no surface reference")?;
        let surface_kind = self.kind(surface_id).unwrap_or("nothing");
        if surface_kind != step_name(surface) {
            return Err(format!(
                "its surface #{surface_id} is {surface_kind}, not {}",
                step_name(surface)
            ));
        }
        if logical(self.attribute(id, 3)) != Some(same_sense) {
            return Err(format!("its same_sense is not {same_sense}"));
        }
        if !same_point(
            self.raw_location(surface_id).as_deref(),
            self.typed_location(face).as_deref(),
        ) {
            return Err(format!(
                "its surface #{surface_id} is not where it was read"
            ));
        }
        // The EDGE_CURVEs of its EDGE_LOOPs, as written and as read.
        let mut written = Vec::new();
        if let Some(Attribute::List(list)) = self.attribute(id, 1) {
            for bound in list {
                let Attribute::EntityRef(bound) = bound else {
                    continue;
                };
                if !matches!(self.kind(*bound), Some("FACE_BOUND" | "FACE_OUTER_BOUND")) {
                    continue;
                }
                let Some(lp) = self.reference(*bound, 1) else {
                    continue;
                };
                if self.kind(lp) != Some("EDGE_LOOP") {
                    continue;
                }
                let Some(Attribute::List(oriented)) = self.attribute(lp, 1) else {
                    continue;
                };
                for oe in oriented {
                    let Attribute::EntityRef(oe) = oe else {
                        continue;
                    };
                    if self.kind(*oe) != Some("ORIENTED_EDGE") {
                        continue;
                    }
                    if let Some(edge) = self.reference(*oe, 3)
                        && self.kind(edge) == Some("EDGE_CURVE")
                    {
                        written.push(edge);
                    }
                }
            }
        }
        let read = face
            .bounds()
            .flat_map(|b| b.edges().collect::<Vec<_>>())
            .map(|e| self.id(e.key()).map_err(|e| e.to_string()))
            .collect::<Result<Vec<u64>, String>>()?;
        if written != read {
            return Err("its loops' edges are not the edges read".into());
        }
        Ok(())
    }

    fn check_edge(&self, edge: &StepEdge<'_>, id: u64) -> Result<(), String> {
        let m::EntityKey::EdgeCurve(i) = edge.key() else {
            return Err(format!("an edge of type {}", step_name(edge.key())));
        };
        let e = self.model.edge_curve_arena.get(i.0);
        let kind = self.kind(id).unwrap_or("nothing");
        if kind != "EDGE_CURVE" {
            return Err(format!("it is {kind}"));
        }
        for (index, vertex) in [(1, &e.edge_start), (2, &e.edge_end)] {
            let written = self.reference(id, index).ok_or("no vertex reference")?;
            let typed = vertex.entity_key();
            if self.kind(written) != Some(&step_name(typed)) {
                return Err(format!("vertex #{written} is not {}", step_name(typed)));
            }
            let read = match vertex {
                m::VertexRef::VertexPoint(v) => {
                    match &self.model.vertex_point_arena.get(v.0).vertex_geometry {
                        m::PointRef::CartesianPoint(c) => Some(
                            self.model
                                .cartesian_point_arena
                                .get(c.0)
                                .coordinates
                                .clone(),
                        ),
                        _ => None,
                    }
                }
                _ => None,
            };
            let written_point = (self.kind(written) == Some("VERTEX_POINT"))
                .then(|| self.reference(written, 1).and_then(|p| self.raw_point(p)))
                .flatten();
            if !same_point(written_point.as_deref(), read.as_deref()) {
                return Err(format!("vertex #{written} is not where it was read"));
            }
        }
        let curve = self.reference(id, 3).ok_or("no curve reference")?;
        let typed = e.edge_geometry.entity_key();
        if self.kind(curve) != Some(&step_name(typed)) {
            return Err(format!("curve #{curve} is not {}", step_name(typed)));
        }
        if logical(self.attribute(id, 4)) != Some(e.same_sense) {
            return Err(format!("its same_sense is not {}", e.same_sense));
        }
        Ok(())
    }
}

/// A BOOLEAN attribute (`.T.` / `.F.`).
fn logical(a: Option<&Attribute>) -> Option<bool> {
    match a? {
        Attribute::Enum(e) if e == "T" => Some(true),
        Attribute::Enum(e) if e == "F" => Some(false),
        _ => None,
    }
}

/// Points exactly equal: step-io's typed coordinates are the same parsed REAL tokens (an
/// INTEGER written where a REAL belongs converts exactly), so any difference means the two are
/// not the same instance. Equal as numbers, not bit for bit: step-io reads a written `-0.0` as
/// `0.0` (NIST CTC-04, FTC-07 and FTC-10, edition 2).
fn same_point(written: Option<&[f64]>, read: Option<&[f64]>) -> bool {
    match (written, read) {
        (Some(a), Some(b)) => a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x == y),
        (None, None) => true,
        _ => false,
    }
}

/// The STEP entity type of a typed key (`BSplineSurfaceWithKnots` → `B_SPLINE_SURFACE_WITH_KNOTS`),
/// `COMPLEX` for a complex instance.
fn step_name(key: m::EntityKey) -> String {
    let debug = format!("{key:?}");
    let variant = debug.split('(').next().unwrap_or_default();
    if variant == "ComplexUnit" {
        return "COMPLEX".into();
    }
    let mut out = String::new();
    for (i, c) in variant.chars().enumerate() {
        if c.is_ascii_uppercase() && i > 0 {
            out.push('_');
        }
        out.push(c.to_ascii_uppercase());
    }
    out
}

/// Read a STEP file's bytes into a [`Part`].
pub fn read_step(bytes: &[u8]) -> Result<Part, StepError> {
    read_step_placed(bytes, &IDENTITY)
}

/// Read a STEP file's bytes into a [`Part`] moved by *outer*, a proper rigid motion applied
/// above every instance placement, as if the file's roots were placed by it.
pub fn read_step_placed(bytes: &[u8], outer: &Placement) -> Result<Part, StepError> {
    read_placed(bytes, outer).map(|read| read.part)
}

/// A product definition the reader placed (an assembly or a part), as file instances.
struct PlacedDefinition {
    /// The `#N` of its `product_definition` (or the `_with_associated_documents` subtype).
    definition: u64,
    /// The `#N` of the `product_definition_shape` its shape is represented through, or why
    /// there is not exactly one.
    shape: Result<u64, String>,
    /// Its product's name.
    name: String,
    placement: Placement,
}

/// A part read with what placed each of its instances.
struct Read {
    part: Part,
    /// Per instance ([`Source::instance`]), the index in `placed` of the definition placed
    /// there, if a product definition holds it.
    instances: Vec<Option<usize>>,
    placed: Vec<PlacedDefinition>,
    /// Per face, its `same_sense` as written.
    same_sense: Vec<bool>,
}

fn read_placed(bytes: &[u8], outer: &Placement) -> Result<Read, StepError> {
    let r = outer.map(|row| [row[0], row[1], row[2]]);
    let det = r[0][0] * (r[1][1] * r[2][2] - r[1][2] * r[2][1])
        - r[0][1] * (r[1][0] * r[2][2] - r[1][2] * r[2][0])
        + r[0][2] * (r[1][0] * r[2][1] - r[1][1] * r[2][0]);
    if (det - 1.0).abs() > 1e-12 {
        // Frames are rebuilt right-handed (y = z × x), so a reflection would not be honoured.
        return Err(StepError::Unsupported(
            "outer placement must be a proper rotation".into(),
        ));
    }
    let (model, report) = step_io::read(bytes).map_err(|e| StepError::Parse(e.to_string()))?;
    let graph = step_io::parser::parse_bytes(bytes).map_err(|e| StepError::Parse(e.to_string()))?;
    refuse_dropped_shapes(&graph, &report)?;
    let mut ids = FileIds::new(&graph, &model, &report);
    let scene = model.scene();
    let units = scene.units();
    let mut reader = Reader {
        model: &model,
        to_mm: units.length.as_ref().map_or(1.0, |u| u.to_si * 1000.0),
        to_rad: units.angle.as_ref().map_or(1.0, |u| u.to_si),
        placement: IDENTITY,
    };

    let faces_by_key: HashMap<m::EntityKey, StepFace<'_>> =
        scene.all_faces().map(|f| (f.key(), f)).collect();
    let face_of = |r: &m::FaceRef| -> Option<StepFace<'_>> {
        let key = match r {
            m::FaceRef::AdvancedFace(i) => m::EntityKey::AdvancedFace(*i),
            m::FaceRef::FaceSurface(i) => m::EntityKey::FaceSurface(*i),
            _ => return None,
        };
        faces_by_key.get(&key).copied()
    };

    // Shells in the order OpenCascade's explorer meets them: placed solid instances, then open
    // shells. Each is read as one instance (a placed solid, or a placement of a surface model),
    // with the placed definition it belongs to.
    let mut shells = Vec::new();
    let mut instance_of: Vec<Option<usize>> = Vec::new();
    let (mut instances, mut placed) = (Vec::new(), Vec::new());
    for root in scene.root_definitions() {
        collect_instances(root, *outer, reader.to_mm, &mut instances, &mut placed, 0);
    }
    if instances.is_empty() {
        instances = scene.all_solids().map(|s| (s, *outer, None)).collect();
    }
    for (solid, placement, def) in instances {
        // The outer shell, then each void shell (whose faces face inward, hence the flip).
        let mut faces: Vec<(StepFace<'_>, bool)> = solid.faces().map(|f| (f, false)).collect();
        let flips = void_orientations(&model, &solid);
        for (void, flip) in solid.voids().into_iter().zip(flips) {
            faces.extend(void.into_iter().map(|f| (f, flip)));
        }
        shells.push((true, placement, faces, instance_of.len()));
        instance_of.push(def);
    }
    let rg = model.ref_graph();
    for index in reachable_surface_models(&model, &rg) {
        let sbsm = model.shell_based_surface_model_arena.get(index);
        // Placed like the assembly instances of the product that owns it, once per instance.
        let owners = surface_model_owners(&model, &rg, index);
        let mut placements: Vec<(Placement, Option<usize>)> = placed
            .iter()
            .enumerate()
            .filter(|(_, (def, _))| owners.contains(&def.key()))
            .map(|(i, (_, p))| (*p, Some(i)))
            .collect();
        if placements.is_empty() {
            placements.push((*outer, None));
        }
        for (placement, def) in placements {
            let instance = instance_of.len();
            instance_of.push(def);
            for shell in &sbsm.sbsm_boundary {
                let faces = match shell {
                    m::ShellRef::OpenShell(i) => &model.open_shell_arena.get(i.0).cfs_faces,
                    m::ShellRef::ClosedShell(i) => &model.closed_shell_arena.get(i.0).cfs_faces,
                    _ => continue,
                };
                shells.push((
                    false,
                    placement,
                    faces
                        .iter()
                        .filter_map(&face_of)
                        .map(|f| (f, false))
                        .collect(),
                    instance,
                ));
            }
        }
    }

    let (mut out_faces, mut edges_out, mut solids) = (Vec::new(), Vec::new(), Vec::new());
    let (mut unresolved_faces, mut unresolved_edges) = (Vec::new(), Vec::new());
    let (mut face_sources, mut edge_sources, mut same_sense) = (Vec::new(), Vec::new(), Vec::new());
    let mut vertex_count = 0;
    for (is_solid, placement, faces, instance) in shells {
        reader.placement = placement;
        // Edges are shared within one placed shell only; another instance gets its own copies.
        let mut edge_index: HashMap<m::EntityKey, usize> = HashMap::new();
        let mut vertex_index: HashMap<m::EntityKey, usize> = HashMap::new();
        let mut vertex_id = |key: m::EntityKey| {
            let next = vertex_count + vertex_index.len();
            *vertex_index.entry(key).or_insert(next)
        };
        let mut members = Vec::new();
        for (face, flipped) in faces {
            let mut loops = Vec::new();
            for bound in face.bounds() {
                let mut edges = Vec::new();
                for (edge, forward) in bound.oriented_edges() {
                    let index = match edge_index.get(&edge.key()) {
                        Some(&i) => i,
                        None => {
                            let (read, resolved) = reader.edge(&edge, &mut vertex_id)?;
                            if !resolved {
                                unresolved_edges.push(edges_out.len());
                            }
                            edges_out.push(read);
                            edge_sources.push(Source {
                                entity: ids.edge(&edge)?,
                                instance,
                            });
                            edge_index.insert(edge.key(), edges_out.len() - 1);
                            edges_out.len() - 1
                        }
                    };
                    edges.push((index, forward));
                }
                if !bound.orientation() {
                    edges.reverse();
                    for e in &mut edges {
                        e.1 = !e.1;
                    }
                }
                let vertex = if edges.is_empty() {
                    reader.vertex_loop(&bound)
                } else {
                    None
                };
                loops.push(Loop { edges, vertex });
            }
            members.push(out_faces.len());
            // A surface no form resolves stays in the part, unevaluable, and is recorded.
            let surface = reader.surface(&face).unwrap_or_else(|| {
                unresolved_faces.push(out_faces.len());
                Surface::Other { kind: "UNRESOLVED" }
            });
            close_cone_at_apex(&surface, &mut loops, &edges_out);
            let pcurves = reader.pcurves(&face, &surface, &edge_index, &edges_out);
            out_faces.push(Face {
                surface,
                reversed: face.same_sense() == flipped,
                loops,
                solid: is_solid.then_some(solids.len()),
                pcurves,
            });
            face_sources.push(Source {
                entity: ids.face(&face)?,
                instance,
            });
            same_sense.push(face.same_sense());
        }
        drop(vertex_id);
        vertex_count += vertex_index.len();
        if is_solid {
            solids.push(Solid { faces: members });
        }
    }
    if out_faces.is_empty() {
        return Err(StepError::Incomplete("no solid or shell with faces".into()));
    }
    let placed = placed
        .into_iter()
        .map(|(def, placement)| {
            Ok(PlacedDefinition {
                definition: ids.id(def.key())?,
                shape: ids.definition_shape(&def, &rg),
                name: def.product().map_or("", |p| p.name()).to_string(),
                placement,
            })
        })
        .collect::<Result<_, StepError>>()?;
    let part = Part::new(out_faces, edges_out, solids)
        .with_unresolved(unresolved_faces, unresolved_edges)
        .with_sources(face_sources, edge_sources);
    Ok(Read {
        part,
        instances: instance_of,
        placed,
        same_sense,
    })
}

/// One distinct part of a file: a product definition whose shape holds faces, with the numbering
/// of its faces and edges that PMI anchors to. Faces are numbered as specify-core numbers them
/// (OpenCascade's `TopExp::MapShapes` over the part's own shape, not over a placed instance).
/// Edges are haecceity's own numbering, one per `EDGE_CURVE`, in the order the faces' loops give
/// them: it is not specify-core's, whose edges come after OpenCascade's healing (which adds seam
/// and split edges, and reverses some loops on geometric grounds), so an edge index from
/// specify-core does not carry over; anchor edges by their `#N`
/// (`tests/fixtures/known_face_sources.json` lists every file where the two differ).
#[derive(Clone, Debug)]
pub struct PartDefinition {
    /// The `#N` of its `product_definition` (or `product_definition_with_associated_documents`).
    pub product_definition: u64,
    /// The `#N` of the `product_definition_shape` its shape is represented through, which the
    /// part's shape aspects name as `of_shape`.
    pub shape: u64,
    /// Its product's name (`PRODUCT.name`).
    pub name: String,
    /// Face index → the `#N` of the `ADVANCED_FACE` (or `FACE_SURFACE`) it was read from.
    pub faces: Vec<u64>,
    /// Edge index → the `#N` of its `EDGE_CURVE`.
    pub edges: Vec<u64>,
    /// Where each instance of the part is placed, in the reader's order.
    pub placements: Vec<PartPlacement>,
    face_index: HashMap<u64, usize>,
    edge_index: HashMap<u64, usize>,
}

/// One placement of a part: rows of `[R | t]` with `t` in millimetres, and the instances of the
/// [`Part`] read with it ([`crate::brep::Source::instance`]): one per solid or surface model of
/// the part.
#[derive(Clone, Debug)]
pub struct PartPlacement {
    pub placement: Placement,
    pub instances: Vec<usize>,
}

impl PartDefinition {
    /// The index of the face read from `ADVANCED_FACE` *entity*, if it is one of this part's.
    pub fn face_index(&self, entity: u64) -> Option<usize> {
        self.face_index.get(&entity).copied()
    }

    /// The index of the edge read from `EDGE_CURVE` *entity*, if it is one of this part's.
    pub fn edge_index(&self, entity: u64) -> Option<usize> {
        self.edge_index.get(&entity).copied()
    }
}

/// The distinct parts of a STEP file, in specify-core's order (depth first through the assembly
/// from its roots, each part where it is first placed; a part placed twice is one part with two
/// placements), each with its faces and edges numbered as in its own shape. Refused when a shape
/// with faces belongs to no product definition (nothing to anchor its PMI to), or a part's
/// product definition has not exactly one represented `product_definition_shape`.
pub fn read_part_definitions(bytes: &[u8]) -> Result<Vec<PartDefinition>, StepError> {
    let Read {
        part,
        instances,
        placed,
        same_sense,
    } = read_placed(bytes, &IDENTITY)?;
    if let Some(orphan) = instances.iter().position(Option::is_none) {
        return Err(StepError::Unsupported(format!(
            "instance {orphan} of the file's shapes belongs to no product definition"
        )));
    }
    let mut parts: Vec<PartDefinition> = Vec::new();
    for (index, def) in placed.iter().enumerate() {
        let mine: Vec<usize> = (0..instances.len())
            .filter(|&i| instances[i] == Some(index))
            .collect();
        if mine.is_empty() {
            continue;
        }
        let placement = PartPlacement {
            placement: def.placement,
            instances: mine.clone(),
        };
        if let Some(known) = parts
            .iter_mut()
            .find(|p| p.product_definition == def.definition)
        {
            known.placements.push(placement);
            continue;
        }
        let shape = def.shape.clone().map_err(|why| {
            StepError::Unsupported(format!("part #{} ({}): {why}", def.definition, def.name))
        })?;
        let ours: Vec<usize> = (0..part.faces.len())
            .filter(|&f| {
                part.face_source(f)
                    .is_some_and(|s| mine.contains(&s.instance))
            })
            .collect();
        let faces: Vec<u64> = ours
            .iter()
            .filter_map(|&f| part.face_source(f).map(|s| s.entity))
            .collect();
        // Edges as OpenCascade's explorer meets them: face by face, loop by loop, a loop's edges
        // in the order the file lists them, backwards when the bound's orientation differs from
        // the face's `same_sense` (a loop's edges here are already backwards when its bound is
        // used reversed); an edge two faces or two of the part's solids share is one edge.
        let mut edges: Vec<u64> = Vec::new();
        let mut edge_index = HashMap::new();
        for &f in &ours {
            let in_order = |l: &Loop| -> Vec<usize> {
                let order = l.edges.iter().map(|&(e, _)| e);
                if same_sense[f] {
                    order.collect()
                } else {
                    order.rev().collect()
                }
            };
            for e in part.faces[f].loops.iter().flat_map(in_order) {
                if let Some(s) = part.edge_source(e)
                    && !edge_index.contains_key(&s.entity)
                {
                    edge_index.insert(s.entity, edges.len());
                    edges.push(s.entity);
                }
            }
        }
        let face_index = faces.iter().enumerate().map(|(i, &f)| (f, i)).collect();
        parts.push(PartDefinition {
            product_definition: def.definition,
            shape,
            name: def.name.clone(),
            faces,
            edges,
            placements: vec![placement],
            face_index,
            edge_index,
        });
    }
    Ok(parts)
}

/// Refuses a file whose shapes lost entities when step-io read it: an entity a shape
/// representation reaches through its items that is missing from the file or was dropped
/// (malformed, or of a type step-io does not model), or a shape representation dropped itself.
/// Entities no shape reaches (presentation, PMI) may be dropped without harm.
fn refuse_dropped_shapes(
    graph: &step_io::parser::Graph,
    report: &step_io::Report,
) -> Result<(), StepError> {
    use std::collections::{BTreeSet, HashSet};
    use step_io::parser::{Attribute, RawEntity};

    fn refs(a: &Attribute, out: &mut Vec<u64>) {
        match a {
            Attribute::EntityRef(n) => out.push(*n),
            Attribute::List(l) => l.iter().for_each(|x| refs(x, out)),
            Attribute::Typed { value, .. } => refs(value, out),
            _ => {}
        }
    }
    fn name(e: &RawEntity) -> String {
        match e {
            RawEntity::Simple { name, .. } => name.clone(),
            RawEntity::Complex { parts, .. } => {
                let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
                format!("({})", names.join(" "))
            }
        }
    }

    if report.dropped.is_empty() {
        return Ok(());
    }
    let dropped: HashMap<u64, &step_io::DropReason> =
        report.dropped.iter().map(|(id, r)| (*id, r)).collect();
    let (mut lost, mut missing) = (BTreeSet::new(), BTreeSet::new());
    let mut stack = Vec::new();
    for (&id, entity) in &graph.entities {
        // A representation's items, not its context: units and tolerances are not shapes.
        let items = match entity {
            RawEntity::Simple {
                name, attributes, ..
            } if name.ends_with("SHAPE_REPRESENTATION") => attributes.get(1),
            RawEntity::Complex { parts, .. }
                if parts
                    .iter()
                    .any(|p| p.name.ends_with("SHAPE_REPRESENTATION")) =>
            {
                parts
                    .iter()
                    .find(|p| p.name == "REPRESENTATION")
                    .and_then(|p| p.attributes.get(1))
            }
            _ => continue,
        };
        if dropped.contains_key(&id) {
            lost.insert(id);
        }
        if let Some(items) = items {
            refs(items, &mut stack);
        }
    }
    let mut seen = HashSet::new();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let Some(entity) = graph.entities.get(&id) else {
            missing.insert(id);
            continue;
        };
        if dropped.contains_key(&id) {
            lost.insert(id);
        }
        match entity {
            RawEntity::Simple { attributes, .. } => {
                attributes.iter().for_each(|a| refs(a, &mut stack))
            }
            RawEntity::Complex { parts, .. } => parts
                .iter()
                .flat_map(|p| &p.attributes)
                .for_each(|a| refs(a, &mut stack)),
        }
    }
    if lost.is_empty() && missing.is_empty() {
        return Ok(());
    }
    // Name the causes: the missing entities, and the dropped ones not dropped merely because
    // something they refer to was.
    let mut causes: Vec<String> = missing
        .iter()
        .map(|id| format!("#{id} is referenced but not in the file"))
        .collect();
    causes.extend(lost.iter().filter_map(|id| {
        let reason = dropped[id];
        (reason.kind != step_io::DropKind::Cascade).then(|| {
            format!(
                "#{id} {} was dropped ({:?}: {})",
                name(&graph.entities[id]),
                reason.kind,
                reason.key
            )
        })
    }));
    if causes.is_empty() {
        causes.extend(
            lost.iter()
                .map(|id| format!("#{id} {} was dropped", name(&graph.entities[id]))),
        );
    }
    let shown = causes.len().min(5);
    let more = if causes.len() > shown {
        format!(" and {} more", causes.len() - shown)
    } else {
        String::new()
    };
    Err(StepError::Incomplete(format!(
        "{}{more} ({} shape entities lost)",
        causes[..shown].join("; "),
        lost.len()
    )))
}

/// A cone face bounded by a single loop running round the axis (a drill point written as just
/// its rim circle, with no seam and no `VERTEX_LOOP`) can only be the part of the cone between
/// that loop and the apex — the other side is unbounded. OpenCascade's import completes it with
/// a seam and a degenerate edge at the apex; here it gets the apex's vertex loop, the form such
/// a face takes when the file writes it out, so its domain, area and bounds reach the apex.
fn close_cone_at_apex(surface: &Surface, loops: &mut Vec<Loop>, edges: &[Edge]) {
    let (Surface::Cone { frame, .. }, Some(apex)) = (surface, surface.cone_apex()) else {
        return;
    };
    if loops
        .iter()
        .any(|lp| lp.vertex.is_some() || lp.edges.is_empty())
    {
        return;
    }
    // A loop that already reaches the apex (along a seam, as OpenCascade writes a cone) bounds
    // the face there itself.
    let reach = frame.to_local(apex)[2].abs().max(1.0) * 1e-7;
    if loops.iter().flat_map(|lp| &lp.edges).any(|&(e, _)| {
        edges[e]
            .samples
            .iter()
            .any(|&p| geom::dist(p, apex) < reach)
    }) {
        return;
    }
    // Turns round the axis: the swept angle of the loop's samples, each step taken the short
    // way round.
    let turns = |lp: &Loop| {
        let angles: Vec<f64> = lp
            .edges
            .iter()
            .flat_map(|&(e, forward)| {
                let samples = &edges[e].samples;
                let ordered: Box<dyn Iterator<Item = &V3>> = if forward {
                    Box::new(samples.iter())
                } else {
                    Box::new(samples.iter().rev())
                };
                ordered.map(|&p| {
                    let l = frame.to_local(p);
                    l[1].atan2(l[0])
                })
            })
            .collect();
        let swept: f64 = angles
            .iter()
            .zip(angles.iter().cycle().skip(1))
            .map(|(a, b)| {
                let d = b - a;
                d - std::f64::consts::TAU * (d / std::f64::consts::TAU).round()
            })
            .sum();
        (swept / std::f64::consts::TAU).round() != 0.0
    };
    if loops.iter().filter(|lp| turns(lp)).count() == 1 {
        loops.push(Loop {
            edges: Vec::new(),
            vertex: Some(apex),
        });
    }
}

/// Read a STEP file from disk, transparently gunzipping `*.gz`.
pub fn read_step_file(path: &std::path::Path) -> Result<Part, Box<dyn std::error::Error>> {
    Ok(read_step(&step_file_bytes(path)?)?)
}

/// [`read_step_file`] moved by *outer* (see [`read_step_placed`]).
pub fn read_step_file_placed(
    path: &std::path::Path,
    outer: &Placement,
) -> Result<Part, Box<dyn std::error::Error>> {
    Ok(read_step_placed(&step_file_bytes(path)?, outer)?)
}

/// A STEP file's bytes, gunzipped when the name ends in `.gz`.
fn step_file_bytes(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let bytes = std::fs::read(path)?;
    let bytes = if path.extension().is_some_and(|e| e == "gz") {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out)?;
        out
    } else {
        bytes
    };
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hlr::{Plane, Unresolved, View, project, section};

    fn fixture(name: &str) -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name)
    }

    /// `rejected_cylinder.step` with its top plane written as an `OFFSET_SURFACE`, which the
    /// kernel does not model: the face is kept and recorded, its solid is not valid, and a
    /// drawing of the part is refused.
    #[test]
    fn an_unresolved_surface_is_recorded() {
        let part = read_step_file(&fixture("broken/unresolved_surface.step")).unwrap();
        assert_eq!(part.faces.len(), 3);
        assert_eq!(part.unresolved_faces(), [1]);
        assert!(part.unresolved_edges().is_empty());
        assert!(matches!(part.faces[1].surface, Surface::Other { .. }));
        assert!(!part.solid_is_valid(0));
        let refused = Unresolved {
            faces: vec![1],
            edges: Vec::new(),
        };
        let view = View::new([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]).unwrap();
        assert_eq!(project(&part, &view).unwrap_err(), refused);
        let plane = Plane::new([0.0; 3], [0.0, 1.0, 0.0]).unwrap();
        assert_eq!(section(&part, &plane).unwrap_err(), refused);
    }

    /// The same cylinder with its seam line written as a hyperbola: the open edge's chord stands
    /// in for it, recorded, and the solid is not valid.
    #[test]
    fn an_unresolved_open_edge_is_recorded() {
        let part = read_step_file(&fixture("broken/unresolved_open_curve.step")).unwrap();
        assert!(part.unresolved_faces().is_empty());
        assert_eq!(part.unresolved_edges(), [1]);
        assert!(!part.solid_is_valid(0));
        let view = View::new([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]).unwrap();
        assert_eq!(project(&part, &view).unwrap_err().edges, [1]);
    }

    /// The unbroken cylinder records nothing, stays valid and is drawn.
    #[test]
    fn a_resolved_part_records_nothing() {
        let part = read_step_file(&fixture("rejected_cylinder.step")).unwrap();
        assert!(part.unresolved_faces().is_empty() && part.unresolved_edges().is_empty());
        assert!(part.solid_is_valid(0));
        let view = View::new([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]).unwrap();
        assert!(!project(&part, &view).unwrap().is_empty());
    }

    /// The id map is checked against the records, not only against entity types: two planar
    /// faces of one loop each, with the same sense, swapped in the map, pass every type check
    /// and are refused on where their planes are; likewise two edges on one kind of curve on where
    /// their vertices are.
    #[test]
    fn a_permuted_instance_map_is_refused() {
        let bytes = std::fs::read(fixture("prismatic.step")).unwrap();
        let (model, report) = step_io::read(&bytes).unwrap();
        let graph = step_io::parser::parse_bytes(&bytes).unwrap();
        let scene = model.scene();
        let faces: Vec<StepFace<'_>> = scene.all_faces().collect();
        let edges: Vec<StepEdge<'_>> = faces
            .iter()
            .flat_map(|f| {
                f.bounds()
                    .flat_map(|b| b.edges().collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            })
            .collect();
        let mut ids = FileIds::new(&graph, &model, &report);
        for f in &faces {
            ids.face(f).unwrap();
        }
        for e in &edges {
            ids.edge(e).unwrap();
        }

        let index = |key: m::EntityKey| match key {
            m::EntityKey::AdvancedFace(i) => i.0,
            m::EntityKey::EdgeCurve(i) => i.0,
            _ => unreachable!(),
        };
        let planar = |f: &&StepFace<'_>| {
            matches!(f.surface().kind(), SurfaceKind::Plane(_)) && f.bounds().count() == 1
        };
        // Planes at different points (the edge check alone would catch planes at one point).
        let location = |f: &StepFace<'_>| {
            let id = ids.id(f.key()).unwrap();
            ids.raw_location(ids.reference(id, 2).unwrap())
        };
        let (a, b) = faces
            .iter()
            .filter(planar)
            .flat_map(|a| faces.iter().filter(planar).map(move |b| (a, b)))
            .find(|(a, b)| a.same_sense() == b.same_sense() && location(a) != location(b))
            .expect("two planar one-loop faces of one sense");
        let mut permuted = FileIds::new(&graph, &model, &report);
        permuted
            .by_type
            .get_mut("ADVANCED_FACE")
            .unwrap()
            .swap(index(a.key()), index(b.key()));
        let (ia, ib) = (permuted.id(a.key()).unwrap(), permuted.id(b.key()).unwrap());
        assert_eq!(permuted.kind(ia), permuted.kind(ib));
        let surface = |id| permuted.kind(permuted.reference(id, 2).unwrap());
        assert_eq!(surface(ia), Some("PLANE"));
        assert_eq!(surface(ia), surface(ib));
        let error = permuted.face(a).unwrap_err().to_string();
        assert!(error.contains("is not where it was read"), "{error}");

        let curve = |e: &StepEdge<'_>| step_name(raw_edge_curve(&model, e).entity_key());
        let (c, d) = edges
            .iter()
            .flat_map(|c| edges.iter().map(move |d| (c, d)))
            .find(|(c, d)| {
                c.key() != d.key() && c.same_sense() == d.same_sense() && curve(c) == curve(d)
            })
            .expect("two edges on one kind of curve, of one sense");
        let mut permuted = FileIds::new(&graph, &model, &report);
        permuted
            .by_type
            .get_mut("EDGE_CURVE")
            .unwrap()
            .swap(index(c.key()), index(d.key()));
        let error = permuted.edge(c).unwrap_err().to_string();
        assert!(error.contains("is not where it was read"), "{error}");
    }

    #[test]
    fn broken_files_are_refused() {
        let error = |name: &str| read_step_file(&fixture(name)).unwrap_err().to_string();
        assert!(error("broken/empty_data.step").contains("no solid or shell"));
        assert!(error("broken/truncated.step").contains("parse error"));
        assert!(error("broken/truncated_closed.step").contains("#105 is referenced"));
        assert!(error("broken/deleted_face.step").contains("#109 is referenced"));
        assert!(error("broken/unresolved_closed_curve.step").contains("closed edge's curve"));
    }
}
