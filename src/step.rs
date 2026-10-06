//! STEP import through `step-io`, mapped into [`Part`].
//!
//! Mirrors `quiddity.import_step_geometry`: geometry only, assemblies flattened, lengths in
//! millimetres (OpenCascade's default import unit).

use std::collections::HashMap;

use step_io::generated::model as m;
use step_io::scene::geometry::{Edge as StepEdge, Face as StepFace, SurfaceKind};

use crate::brep::{Edge, Face, Loop, Part, Solid, sample_edge};
use crate::geom::{self, Curve, Frame, Surface, V3};
use crate::nurbs::{NurbsCurve, NurbsSurface};

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

    fn frame3(&self, id: m::Axis2Placement3dId) -> Option<Frame> {
        let a = self.model.axis2_placement3d_arena.get(id.0);
        let origin = self.point(&a.location)?;
        let z = match &a.axis {
            Some(d) => self.direction(d)?,
            None => [0.0, 0.0, 1.0],
        };
        let reference = match &a.ref_direction {
            Some(d) => self.direction(d)?,
            None => {
                if z[0].abs() < 0.9 {
                    [1.0, 0.0, 0.0]
                } else {
                    [0.0, 0.0, 1.0]
                }
            }
        };
        let x = geom::unit(geom::sub(
            reference,
            geom::scale(z, geom::dot(reference, z)),
        ))?;
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

    fn freeform(&self, face: &StepFace<'_>, kind: &'static str) -> Option<Surface> {
        let n = face.to_nurbs()?;
        let place = |row: &Vec<[f64; 3]>| -> Vec<V3> {
            row.iter()
                .map(|p| self.place_point(geom::scale(*p, self.to_mm)))
                .collect()
        };
        Some(Surface::Freeform {
            kind,
            surface: Box::new(NurbsSurface::new(
                n.degree_u,
                n.degree_v,
                n.control_points.iter().map(place).collect(),
                n.weights,
                n.knots_u,
                n.knots_v,
            )),
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

    fn edge(&self, edge: &StepEdge<'_>) -> Result<Edge, StepError> {
        let vertex = |v: Option<step_io::scene::geometry::Vertex<'_>>| -> Option<V3> {
            let p = v?.point()?.xyz();
            Some(self.place_point(geom::scale(p, self.to_mm)))
        };
        let (Some(start), Some(end)) = (vertex(edge.start()), vertex(edge.end())) else {
            return Err(StepError::Unsupported(
                "edge without cartesian vertices".into(),
            ));
        };
        let curve = self.curve(&raw_edge_curve(self.model, edge)).or_else(|| {
            edge.to_nurbs().map(|n| {
                Curve::Nurbs(NurbsCurve {
                    degree: n.degree,
                    control_points: n
                        .control_points
                        .iter()
                        .map(|p| self.place_point(geom::scale(*p, self.to_mm)))
                        .collect(),
                    weights: n.weights,
                    knots: n.knots,
                })
            })
        });
        let curve = curve.unwrap_or(Curve::Other { kind: "UNRESOLVED" });
        let samples = sample_edge(&curve, start, end, edge.same_sense());
        Ok(Edge {
            curve,
            start,
            end,
            same_sense: edge.same_sense(),
            samples,
        })
    }
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
    let mut shells: Vec<(bool, Placement, Vec<StepFace<'_>>)> = Vec::new();
    let mut instances = Vec::new();
    for root in scene.root_definitions() {
        collect_instances(root, IDENTITY, reader.to_mm, &mut instances, 0);
    }
    if instances.is_empty() {
        instances = scene.all_solids().map(|s| (s, IDENTITY)).collect();
    }
    for (solid, placement) in instances {
        shells.push((true, placement, solid.faces().collect()));
    }
    for sbsm in &model.shell_based_surface_model_arena.items {
        for shell in &sbsm.sbsm_boundary {
            let faces = match shell {
                m::ShellRef::OpenShell(i) => &model.open_shell_arena.get(i.0).cfs_faces,
                m::ShellRef::ClosedShell(i) => &model.closed_shell_arena.get(i.0).cfs_faces,
                _ => continue,
            };
            shells.push((false, IDENTITY, faces.iter().filter_map(&face_of).collect()));
        }
    }

    let (mut out_faces, mut edges_out, mut solids) = (Vec::new(), Vec::new(), Vec::new());
    for (is_solid, placement, faces) in shells {
        reader.placement = placement;
        // Edges are shared within one placed shell only; another instance gets its own copies.
        let mut edge_index: HashMap<m::EntityKey, usize> = HashMap::new();
        let mut members = Vec::new();
        for face in faces {
            let mut loops = Vec::new();
            for bound in face.bounds() {
                let mut edges = Vec::new();
                for (edge, forward) in bound.oriented_edges() {
                    let index = match edge_index.get(&edge.key()) {
                        Some(&i) => i,
                        None => {
                            edges_out.push(reader.edge(&edge)?);
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
            out_faces.push(Face {
                surface: reader.surface(&face),
                reversed: !face.same_sense(),
                loops,
                solid: is_solid.then_some(solids.len()),
            });
        }
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
