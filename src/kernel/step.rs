//! STEP import through `step-io`, mapped into [`Part`].
//!
//! Mirrors `quiddity.import_step_geometry`: geometry only, assemblies flattened, lengths in
//! millimetres (OpenCascade's default import unit).

use std::collections::HashMap;

use step_io::generated::model as m;
use step_io::scene::geometry::{Edge as StepEdge, Face as StepFace, SurfaceKind};

use super::brep::{Edge, Face, Loop, Part, Solid};
use super::geom::{self, Curve, Frame, Surface, V3};
use super::nurbs::{NurbsCurve, NurbsSurface};
use super::sampling::sample_edge;

#[derive(Debug)]
pub enum StepError {
    Parse(String),
    Unsupported(String),
}

impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StepError::Parse(s) => write!(f, "STEP parse error: {s}"),
            StepError::Unsupported(s) => write!(f, "unsupported STEP content: {s}"),
        }
    }
}

impl std::error::Error for StepError {}

/// A rigid placement, rows of `[R | t]` with `t` in millimetres.
type Placement = [[f64; 4]; 3];

const IDENTITY: Placement = [
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

    fn surface(&self, face: &StepFace<'_>) -> Surface {
        let mm = self.to_mm;
        let resolved = match face.surface().kind() {
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
            SurfaceKind::BSpline(_) | SurfaceKind::BSplineWithKnots(_) => {
                self.freeform(face, "BSPLINE")
            }
            SurfaceKind::Bezier(_) => self.freeform(face, "BEZIER"),
            SurfaceKind::LinearExtrusion(_) => self.freeform(face, "EXTRUSION"),
            SurfaceKind::Revolution(_) => self.freeform(face, "REVOLUTION"),
            _ => self.freeform(face, "OTHER"),
        };
        resolved.unwrap_or(Surface::Other { kind: "UNRESOLVED" })
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

    /// The control polygons of the B-spline pcurves the file gives for this face's edges, in
    /// the surface's parameter units (lengths scaled to millimetres, angles as they are).
    fn pcurve_poles(&self, face: &StepFace<'_>, surface: &Surface) -> Vec<Vec<(f64, f64)>> {
        let Ok(basis) = m::SurfaceRef::from_any(face.surface().key()) else {
            return Vec::new();
        };
        let (su, sv) = match surface {
            Surface::Plane { .. } => (self.to_mm, self.to_mm),
            Surface::Cylinder { .. } | Surface::Cone { .. } => (self.to_rad, self.to_mm),
            Surface::Sphere { .. } | Surface::Torus { .. } => (self.to_rad, self.to_rad),
            Surface::Freeform { .. } | Surface::Other { .. } => return Vec::new(),
        };
        let mut out = Vec::new();
        for bound in face.bounds() {
            for edge in bound.edges() {
                let vertex = |v: Option<step_io::scene::geometry::Vertex<'_>>| {
                    v.and_then(|v| v.point())
                        .map(|p| self.place_point(geom::scale(p.xyz(), self.to_mm)))
                };
                let (Some(a), Some(b)) = (vertex(edge.start()), vertex(edge.end())) else {
                    continue;
                };
                let ends = (a, b);
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
                        let Some(points) = self.bspline_poles_2d(item) else {
                            continue;
                        };
                        let points: Vec<(f64, f64)> =
                            points.into_iter().map(|(u, v)| (u * su, v * sv)).collect();
                        // OpenCascade boxes the pcurve trimmed to the edge. A clamped curve's
                        // end poles are its end points, so the polygon stands for the edge only
                        // when they land on the edge's vertices; a longer curve (a line's pcurve
                        // running past the edge) is left to the sampled loops.
                        if spans_edge(surface, &points, ends) {
                            out.push(points);
                        }
                    }
                }
            }
        }
        out
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
            m::CurveRef::SurfaceCurve(i) => {
                self.curve(&self.model.surface_curve_arena.get(i.0).curve_3d)
            }
            m::CurveRef::SeamCurve(i) => self.curve(&self.model.seam_curve_arena.get(i.0).curve_3d),
            _ => None,
        }
    }

    /// *vertex_id* numbers the vertices of the instance being read.
    fn edge(
        &self,
        edge: &StepEdge<'_>,
        vertex_id: &mut impl FnMut(m::EntityKey) -> usize,
    ) -> Result<Edge, StepError> {
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
        let curve = self
            .curve(&raw_edge_curve(self.model, edge))
            .or_else(|| edge.curve().to_nurbs().and_then(to_curve))
            .or_else(|| edge.to_nurbs().and_then(to_curve));
        // A curve no form resolves is replaced by its chord, which keeps the loop closed where
        // dropping the edge would open it.
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
        Ok(Edge {
            curve,
            start,
            end,
            vertices,
            same_sense: edge.same_sense(),
            samples,
        })
    }
}

/// Whether a pcurve's end poles map onto the edge's two vertices (in either order).
fn spans_edge(surface: &Surface, poles: &[(f64, f64)], (a, b): (V3, V3)) -> bool {
    let (Some(&first), Some(&last)) = (poles.first(), poles.last()) else {
        return false;
    };
    let (p, q) = (
        surface.value(first.0, first.1),
        surface.value(last.0, last.1),
    );
    // Files approximate their pcurves; a curve longer than its edge misses a vertex by a large
    // fraction of the edge, an approximation by far less.
    let tol = (0.01 * geom::dist(a, b)).max(1e-4);
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

/// Every solid reachable from an assembly definition, with its accumulated placement.
fn collect_instances<'m>(
    def: step_io::scene::product::ProductDef<'m>,
    placement: Placement,
    to_mm: f64,
    out: &mut Vec<(step_io::scene::geometry::Solid<'m>, Placement)>,
    depth: usize,
) {
    if depth > 64 {
        return;
    }
    for solid in def.solids() {
        out.push((solid, placement));
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
        collect_instances(child, compose(&placement, &local), to_mm, out, depth + 1);
    }
}

fn raw_edge_curve(model: &m::StepModel, edge: &StepEdge<'_>) -> m::CurveRef {
    let m::EntityKey::EdgeCurve(id) = edge.key() else {
        unreachable!("edges are EDGE_CURVEs")
    };
    model.edge_curve_arena.get(id.0).edge_geometry.clone()
}

/// Read a STEP file's bytes into a [`Part`].
pub fn read_step(bytes: &[u8]) -> Result<Part, StepError> {
    let (model, _report) = step_io::read(bytes).map_err(|e| StepError::Parse(e.to_string()))?;
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
    // shells.
    let mut shells: Vec<(bool, Placement, Vec<(StepFace<'_>, bool)>)> = Vec::new();
    let mut instances = Vec::new();
    for root in scene.root_definitions() {
        collect_instances(root, IDENTITY, reader.to_mm, &mut instances, 0);
    }
    if instances.is_empty() {
        instances = scene.all_solids().map(|s| (s, IDENTITY)).collect();
    }
    for (solid, placement) in instances {
        // The outer shell, then each void shell (whose faces face inward, hence the flip).
        let mut faces: Vec<(StepFace<'_>, bool)> = solid.faces().map(|f| (f, false)).collect();
        let flips = void_orientations(&model, &solid);
        for (void, flip) in solid.voids().into_iter().zip(flips) {
            faces.extend(void.into_iter().map(|f| (f, flip)));
        }
        shells.push((true, placement, faces));
    }
    let rg = model.ref_graph();
    for index in reachable_surface_models(&model, &rg) {
        let sbsm = model.shell_based_surface_model_arena.get(index);
        for shell in &sbsm.sbsm_boundary {
            let faces = match shell {
                m::ShellRef::OpenShell(i) => &model.open_shell_arena.get(i.0).cfs_faces,
                m::ShellRef::ClosedShell(i) => &model.closed_shell_arena.get(i.0).cfs_faces,
                _ => continue,
            };
            shells.push((
                false,
                IDENTITY,
                faces
                    .iter()
                    .filter_map(&face_of)
                    .map(|f| (f, false))
                    .collect(),
            ));
        }
    }

    let (mut out_faces, mut edges_out, mut solids) = (Vec::new(), Vec::new(), Vec::new());
    let mut vertex_count = 0;
    for (is_solid, placement, faces) in shells {
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
                            edges_out.push(reader.edge(&edge, &mut vertex_id)?);
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
            let surface = reader.surface(&face);
            let pcurve_poles = reader.pcurve_poles(&face, &surface);
            out_faces.push(Face {
                surface,
                reversed: face.same_sense() == flipped,
                loops,
                solid: is_solid.then_some(solids.len()),
                pcurve_poles,
            });
        }
        drop(vertex_id);
        vertex_count += vertex_index.len();
        if is_solid {
            solids.push(Solid { faces: members });
        }
    }
    Ok(Part::new(out_faces, edges_out, solids))
}

/// Read a STEP file from disk, transparently gunzipping `*.gz`.
pub fn read_step_file(path: &std::path::Path) -> Result<Part, Box<dyn std::error::Error>> {
    let bytes = std::fs::read(path)?;
    let bytes = if path.extension().is_some_and(|e| e == "gz") {
        use std::io::Read;
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..]).read_to_end(&mut out)?;
        out
    } else {
        bytes
    };
    Ok(read_step(&bytes)?)
}
