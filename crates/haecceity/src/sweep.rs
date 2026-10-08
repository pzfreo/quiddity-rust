//! A planar face swept along a straight vector into a solid of its own — what the Python
//! implementation builds with `Solid.extrude` / `BRepPrimAPI_MakePrism` to ask a volume probe
//! about a region shaped like a face (`kernel::volume::Probe::Solid`).
//!
//! The prism is built exactly: the face itself at both ends and one side face per edge, a plane
//! along each straight edge and a cylinder along each circular arc whose axis runs with the
//! sweep. A closed circle (a hole's whole rim) sweeps to a seamless cylinder band bounded by its
//! two copies. Other edges (free-form curves, arcs about an axis across the sweep) are not swept;
//! the caller is told so and must not treat the region as proved either way.

use super::brep::{Edge, Face, Loop, Part, Solid};
use super::geom::{self, Curve, Frame, Surface, V3};
use super::sampling::sample_edge;

/// *face* of *part* moved by *offset* and swept along *sweep*, as a one-solid part, or `None`
/// when the face is not planar, the sweep runs in its plane, or an edge is not a straight line
/// or a circle or circular arc about an axis along the sweep.
pub fn extrude_face(part: &Part, face: usize, offset: V3, sweep: V3) -> Option<Part> {
    let f = &part.faces[face];
    let Surface::Plane { frame } = f.surface else {
        return None;
    };
    let up = geom::unit(sweep)?;
    if geom::dot(frame.z, up).abs() <= 1e-9 {
        return None;
    }
    let top_by = geom::add(offset, sweep);
    // Each source edge once: its copies at both ends.
    let source: Vec<usize> = part.face_edges(face);
    let vertex_ids = sweepable_vertices(part, &source, up)?;
    let n_vertices = vertex_ids.len();
    // Edges: bottom copies, top copies, then one rising edge per vertex.
    let mut edges: Vec<Edge> = source
        .iter()
        .map(|&e| copy_edge(&part.edges[e], offset, 0, &vertex_ids))
        .collect();
    edges.extend(
        source
            .iter()
            .map(|&e| copy_edge(&part.edges[e], top_by, 1, &vertex_ids)),
    );
    let riser = push_risers(part, &source, &vertex_ids, offset, sweep, &mut edges);
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
    let mut faces = vec![
        cap(f, frame, offset, &bottom, along),
        cap(f, frame, top_by, &top, !along),
    ];
    // Sides: each edge of the far end (which faces along the sweep, its interior on the left of
    // its walk looking down the sweep) walked back, down, along the near end and up again.
    let n = source.len();
    let far_loops: Vec<Vec<(usize, bool)>> =
        faces[1].loops.iter().map(|lp| lp.edges.clone()).collect();
    for lp in &far_loops {
        for &(t, forward) in lp {
            faces.push(side_face(&edges, t, forward, n, n_vertices, &riser, up)?);
        }
    }
    let solid = Solid {
        faces: (0..faces.len()).collect(),
    };
    Some(Part::new(faces, edges, vec![solid]))
}

/// The vertices of the *source* edges, each once in order of first use, or `None` when an edge
/// is neither a straight line nor a circle or circular arc about an axis along *up*.
fn sweepable_vertices(part: &Part, source: &[usize], up: V3) -> Option<Vec<usize>> {
    let mut vertex_ids: Vec<usize> = Vec::new();
    for &e in source {
        let edge = &part.edges[e];
        match &edge.curve {
            Curve::Line { .. } if !edge.is_closed() => {}
            Curve::Circle { frame: c, .. } if 1.0 - geom::dot(c.z, up).abs() <= 1e-9 => {}
            _ => return None,
        }
        for v in [edge.vertices.0, edge.vertices.1] {
            if !vertex_ids.contains(&v) {
                vertex_ids.push(v);
            }
        }
    }
    Some(vertex_ids)
}

/// *v*'s index among the swept vertices.
fn vertex_index(vertex_ids: &[usize], v: usize) -> usize {
    vertex_ids.iter().position(|&x| x == v).expect("collected")
}

/// A copy of *edge* moved by *by*, its vertices those of end *level* (0 near, 1 far).
fn copy_edge(edge: &Edge, by: V3, level: usize, vertex_ids: &[usize]) -> Edge {
    let n_vertices = vertex_ids.len();
    Edge {
        curve: match &edge.curve {
            Curve::Line { origin, dir } => Curve::Line {
                origin: geom::add(*origin, by),
                dir: *dir,
            },
            Curve::Circle { frame, radius } => Curve::Circle {
                frame: Frame {
                    origin: geom::add(frame.origin, by),
                    ..*frame
                },
                radius: *radius,
            },
            _ => unreachable!("refused above"),
        },
        start: geom::add(edge.start, by),
        end: geom::add(edge.end, by),
        vertices: (
            vertex_index(vertex_ids, edge.vertices.0) + level * n_vertices,
            vertex_index(vertex_ids, edge.vertices.1) + level * n_vertices,
        ),
        same_sense: edge.same_sense,
        samples: edge.samples.iter().map(|&p| geom::add(p, by)).collect(),
    }
}

/// Pushes onto *edges* a rising edge for each vertex an open edge ends at (a closed circle's
/// vertex has none), and returns each vertex's rising edge.
fn push_risers(
    part: &Part,
    source: &[usize],
    vertex_ids: &[usize],
    offset: V3,
    sweep: V3,
    edges: &mut Vec<Edge>,
) -> Vec<Option<usize>> {
    let n_vertices = vertex_ids.len();
    let vid = |v: usize| vertex_index(vertex_ids, v);
    let mut corner: Vec<Option<V3>> = vec![None; n_vertices];
    for &e in source {
        let edge = &part.edges[e];
        corner[vid(edge.vertices.0)] = Some(edge.start);
        corner[vid(edge.vertices.1)] = Some(edge.end);
    }
    let mut riser: Vec<Option<usize>> = vec![None; n_vertices];
    for &e in source.iter().filter(|&&e| !part.edges[e].is_closed()) {
        let edge = &part.edges[e];
        riser[vid(edge.vertices.0)] = Some(0);
        riser[vid(edge.vertices.1)] = Some(0);
    }
    for (i, p) in corner.iter().enumerate() {
        if riser[i].is_none() {
            continue;
        }
        riser[i] = Some(edges.len());
        let start = geom::add(p.expect("every vertex is an edge end"), offset);
        let end = geom::add(start, sweep);
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
    riser
}

/// An end of the prism: face *f* (in the plane of *frame*) moved by *by*, its edges renumbered
/// by *edge_of*, and turned over when *flip*.
fn cap(f: &Face, frame: Frame, by: V3, edge_of: &dyn Fn(usize) -> usize, flip: bool) -> Face {
    Face {
        surface: Surface::Plane {
            frame: Frame {
                origin: geom::add(frame.origin, by),
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
    }
}

/// The side swept by far-end edge *t* (used *forward* or not by the far end), whose near-end
/// copy is `t - n`; `None` when its surface's orientation cannot be found.
fn side_face(
    edges: &[Edge],
    t: usize,
    forward: bool,
    n: usize,
    n_vertices: usize,
    riser: &[Option<usize>],
    up: V3,
) -> Option<Face> {
    let edge = &edges[t];
    let (p, q) = if forward {
        (edge.vertices.0, edge.vertices.1)
    } else {
        (edge.vertices.1, edge.vertices.0)
    };
    let (p, q) = (p - n_vertices, q - n_vertices);
    // A closed circle's band is bounded by its two copies as separate loops.
    let side_loops = match (riser[p], riser[q]) {
        (Some(down), Some(up)) if !edge.is_closed() => {
            vec![vec![
                (t, !forward),
                (down, false),
                (t - n, forward),
                (up, true),
            ]]
        }
        _ => vec![vec![(t, !forward)], vec![(t - n, forward)]],
    };
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
    Some(Face {
        surface,
        reversed: geom::dot(natural, out_normal) < 0.0,
        loops: side_loops
            .into_iter()
            .map(|edges| Loop {
                edges,
                vertex: None,
            })
            .collect(),
        solid: Some(0),
        pcurves: Vec::new(),
    })
}
