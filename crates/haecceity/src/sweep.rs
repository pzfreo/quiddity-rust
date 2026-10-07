//! A planar face swept along a straight vector into a solid of its own — what the Python
//! implementation builds with `Solid.extrude` / `BRepPrimAPI_MakePrism` to ask a volume probe
//! about a region shaped like a face (`kernel::volume::Probe::Solid`).
//!
//! The prism is built exactly: the face itself at both ends and one side face per edge, a plane
//! along each straight edge and a cylinder along each circular arc whose axis runs with the
//! sweep. Other edges (closed circles, which would need a seam, and free-form curves) are not
//! swept; the caller is told so and must not treat the region as proved either way.

use super::brep::{Edge, Face, Loop, Part, Solid};
use super::geom::{self, Curve, Frame, Surface, V3};
use super::sampling::sample_edge;

/// *face* of *part* moved by *offset* and swept along *sweep*, as a one-solid part, or `None`
/// when the face is not planar, the sweep runs in its plane, or an edge is not a straight line
/// or an open circular arc about an axis along the sweep.
pub fn extrude_face(part: &Part, face: usize, offset: V3, sweep: V3) -> Option<Part> {
    let f = &part.faces[face];
    let Surface::Plane { frame } = f.surface else {
        return None;
    };
    let up = geom::unit(sweep)?;
    if geom::dot(frame.z, up).abs() <= 1e-9 {
        return None;
    }
    let moved = |p: V3, by: V3| geom::add(p, by);
    let top_by = geom::add(offset, sweep);
    // Each source edge once: its copies at both ends.
    let source: Vec<usize> = part.face_edges(face);
    let mut vertex_ids: Vec<usize> = Vec::new();
    for &e in &source {
        let edge = &part.edges[e];
        if edge.is_closed() {
            return None;
        }
        match &edge.curve {
            Curve::Line { .. } => {}
            Curve::Circle { frame: c, .. } if 1.0 - geom::dot(c.z, up).abs() <= 1e-9 => {}
            _ => return None,
        }
        for v in [edge.vertices.0, edge.vertices.1] {
            if !vertex_ids.contains(&v) {
                vertex_ids.push(v);
            }
        }
    }
    let n_vertices = vertex_ids.len();
    let vid = |v: usize| vertex_ids.iter().position(|&x| x == v).expect("collected");
    let copy = |edge: &Edge, by: V3, level: usize| Edge {
        curve: match &edge.curve {
            Curve::Line { origin, dir } => Curve::Line {
                origin: moved(*origin, by),
                dir: *dir,
            },
            Curve::Circle { frame, radius } => Curve::Circle {
                frame: Frame {
                    origin: moved(frame.origin, by),
                    ..*frame
                },
                radius: *radius,
            },
            _ => unreachable!("refused above"),
        },
        start: moved(edge.start, by),
        end: moved(edge.end, by),
        vertices: (
            vid(edge.vertices.0) + level * n_vertices,
            vid(edge.vertices.1) + level * n_vertices,
        ),
        same_sense: edge.same_sense,
        samples: edge.samples.iter().map(|&p| moved(p, by)).collect(),
    };
    // Edges: bottom copies, top copies, then one rising edge per vertex.
    let mut edges: Vec<Edge> = source
        .iter()
        .map(|&e| copy(&part.edges[e], offset, 0))
        .collect();
    edges.extend(source.iter().map(|&e| copy(&part.edges[e], top_by, 1)));
    let mut corner: Vec<Option<V3>> = vec![None; n_vertices];
    for &e in &source {
        let edge = &part.edges[e];
        corner[vid(edge.vertices.0)] = Some(edge.start);
        corner[vid(edge.vertices.1)] = Some(edge.end);
    }
    let rising0 = edges.len();
    for (i, p) in corner.iter().enumerate() {
        let start = moved(p.expect("every vertex is an edge end"), offset);
        let end = moved(start, sweep);
        let curve = Curve::Line {
            origin: start,
            dir: sweep,
        };
        edges.push(Edge {
            samples: sample_edge(&curve, start, end, true, false),
            curve,
            start,
            end,
            vertices: (i, i + n_vertices),
            same_sense: true,
        });
    }
    let bottom = |e: usize| source.iter().position(|&x| x == e).expect("face edge");
    let top = |e: usize| bottom(e) + source.len();

    // The face's outward normal decides which end keeps its orientation: the end the sweep
    // leaves from faces against the sweep.
    let outward = if f.reversed {
        geom::scale(frame.z, -1.0)
    } else {
        frame.z
    };
    let along = geom::dot(outward, up) > 0.0;
    let cap = |by: V3, edge_of: &dyn Fn(usize) -> usize, flip: bool| Face {
        surface: Surface::Plane {
            frame: Frame {
                origin: moved(frame.origin, by),
                ..frame
            },
        },
        reversed: f.reversed != flip,
        loops: f
            .loops
            .iter()
            .map(|lp| {
                let mut edges: Vec<(usize, bool)> = lp
                    .edges
                    .iter()
                    .map(|&(e, fw)| (edge_of(e), fw != flip))
                    .collect();
                if flip {
                    edges.reverse();
                }
                Loop {
                    edges,
                    vertex: None,
                }
            })
            .collect(),
        solid: Some(0),
        pcurves: Vec::new(),
    };
    let mut faces = vec![cap(offset, &bottom, along), cap(top_by, &top, !along)];
    // Sides: each edge of the far end (which faces along the sweep, its interior on the left of
    // its walk looking down the sweep) walked back, down, along the near end and up again.
    let n = source.len();
    let far_loops: Vec<Vec<(usize, bool)>> =
        faces[1].loops.iter().map(|lp| lp.edges.clone()).collect();
    for lp in &far_loops {
        for &(t, forward) in lp {
            let edge = &edges[t];
            let (p, q) = if forward {
                (edge.vertices.0, edge.vertices.1)
            } else {
                (edge.vertices.1, edge.vertices.0)
            };
            let (p, q) = (p - n_vertices, q - n_vertices);
            let side_loop = vec![
                (t, !forward),
                (rising0 + p, false),
                (t - n, forward),
                (rising0 + q, true),
            ];
            // The side's outward normal: the walk's tangent across the sweep.
            let samples = &edge.samples;
            let k = samples.len() / 2;
            let (a, c) = (
                samples[k.saturating_sub(1).min(samples.len() - 2)],
                samples[k.max(1)],
            );
            let walk = if forward {
                geom::sub(c, a)
            } else {
                geom::sub(a, c)
            };
            let mid = geom::scale(geom::add(a, c), 0.5);
            let out_normal = geom::unit(geom::cross(walk, up))?;
            let surface = match &edge.curve {
                Curve::Line { .. } => {
                    let x = geom::unit(geom::sub(edge.end, edge.start))?;
                    let z = out_normal;
                    Surface::Plane {
                        frame: Frame {
                            origin: edge.start,
                            x,
                            y: geom::cross(z, x),
                            z,
                        },
                    }
                }
                Curve::Circle { frame, radius } => Surface::Cylinder {
                    frame: *frame,
                    radius: *radius,
                },
                _ => unreachable!("refused above"),
            };
            let (u, v) = surface.parameters(mid, None)?;
            let natural = surface.normal(u, v)?;
            faces.push(Face {
                surface,
                reversed: geom::dot(natural, out_normal) < 0.0,
                loops: vec![Loop {
                    edges: side_loop,
                    vertex: None,
                }],
                solid: Some(0),
                pcurves: Vec::new(),
            });
        }
    }
    let solid = Solid {
        faces: (0..faces.len()).collect(),
    };
    Some(Part::new(faces, edges, vec![solid]))
}
