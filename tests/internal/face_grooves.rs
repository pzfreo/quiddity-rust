//! For #123: independent revolved profiles, with no dependency on the unlicensed 608ZZ.
use quiddity::features::{Context, face_grooves};
use quiddity::kernel::brep::{Edge, Face, Loop, Part, Solid};
use quiddity::kernel::geom::{self, Curve, Frame, Surface};
use quiddity::kernel::step::IDENTITY;
use std::f64::consts::{FRAC_PI_2, TAU};

fn frame(z: f64) -> Frame {
    Frame {
        origin: [0., 0., z],
        x: [1., 0., 0.],
        y: [0., 1., 0.],
        z: [0., 0., 1.],
    }
}

/// Revolve a clockwise meridian polygon. Optional toroidal segments specify the centre
/// and radius of a circular meridian arc. This is constructed from numbers, not a STEP.
fn revolve(points: &[(f64, f64)], rounds: &[(usize, f64, f64, f64)]) -> Part {
    let mut edges: Vec<_> = points
        .iter()
        .enumerate()
        .map(|(i, &(r, z))| {
            let curve = Curve::Circle {
                frame: frame(z),
                radius: r,
            };
            Edge {
                start: curve.value(0.),
                end: curve.value(0.),
                samples: (0..=64)
                    .map(|j| curve.value(TAU * j as f64 / 64.))
                    .collect(),
                curve,
                vertices: (i, i),
                same_sense: true,
            }
        })
        .collect();
    let mut faces = Vec::new();
    for (i, &(r, z)) in points.iter().enumerate() {
        let j = (i + 1) % points.len();
        let (s, t) = points[j];
        let (dr, dz) = (s - r, t - z);
        let (surface, reversed, loops) = if dz.abs() < 1e-12 {
            (
                Surface::Plane { frame: frame(z) },
                dr > 0.,
                vec![
                    Loop {
                        edges: vec![(i, true)],
                        vertex: None,
                    },
                    Loop {
                        edges: vec![(j, false)],
                        vertex: None,
                    },
                ],
            )
        } else {
            let start = edges[i].start;
            let end = edges[j].start;
            let (surface, reversed, curve, samples) = if let Some(&(_, major, centre, minor)) =
                rounds.iter().find(|x| x.0 == i)
            {
                let a = (z - centre).atan2(r - major);
                let b = (t - centre).atan2(s - major);
                let span = (b - a + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI;
                assert!((span.abs() - FRAC_PI_2).abs() < 1e-9);
                let curve = Curve::Circle {
                    frame: Frame {
                        origin: [major, 0., centre],
                        x: [1., 0., 0.],
                        y: [0., 0., 1.],
                        z: [0., -1., 0.],
                    },
                    radius: minor,
                };
                let samples = (0..=32)
                    .map(|k| curve.value(a + span * k as f64 / 32.))
                    .collect();
                (
                    Surface::Torus {
                        frame: frame(centre),
                        major,
                        minor,
                    },
                    dz * (a + span / 2.).cos() - dr * (a + span / 2.).sin() < 0.,
                    curve,
                    samples,
                )
            } else {
                let surface = if dr.abs() < 1e-12 {
                    Surface::Cylinder {
                        frame: frame(z),
                        radius: r,
                    }
                } else {
                    let mut f = frame(z);
                    if dr / dz < 0. {
                        f.y = [0., -1., 0.];
                        f.z = [0., 0., -1.];
                    }
                    Surface::Cone {
                        frame: f,
                        radius: r,
                        semi_angle: (dr / dz).abs().atan(),
                    }
                };
                (
                    surface,
                    dz < 0.,
                    Curve::Line {
                        origin: start,
                        dir: geom::sub(end, start),
                    },
                    vec![start, end],
                )
            };
            let seam = edges.len();
            let same_sense = if let Curve::Circle { frame, .. } = &curve {
                let a = frame.to_local(start);
                let b = frame.to_local(end);
                (b[1].atan2(b[0]) - a[1].atan2(a[0]) + std::f64::consts::PI).rem_euclid(TAU)
                    - std::f64::consts::PI
                    > 0.
            } else {
                true
            };
            edges.push(Edge {
                curve,
                start,
                end,
                samples,
                vertices: (i, j),
                same_sense,
            });
            (
                surface,
                reversed,
                vec![Loop {
                    edges: vec![(i, true), (seam, true), (j, false), (seam, false)],
                    vertex: None,
                }],
            )
        };
        faces.push(Face {
            surface,
            reversed,
            solid: Some(0),
            pcurves: vec![],
            loops,
        });
    }
    Part::new(
        faces,
        edges,
        vec![Solid {
            faces: (0..points.len()).collect(),
        }],
    )
}

pub(super) fn two_ends() -> Part {
    revolve(
        &[
            (11., -3.5),
            (11., 3.5),
            (9.25, 3.5),
            (9., 3.25),
            (6.75, 3.25),
            (6.5, 3.5),
            (4., 3.5),
            (4., -3.5),
            (6.5, -3.5),
            (6.75, -3.25),
            (9., -3.25),
            (9.25, -3.5),
        ],
        &[],
    )
}

/// Replace only the exterior cylinder with four planar sides. The recesses have no
/// dependency on that cylinder or on a turned-body classification.
fn prismatic_host() -> Part {
    let base = two_ends();
    let mut faces = base.faces.clone();
    let mut edges = base.edges.clone();
    let corners = [[12., 12.], [-12., 12.], [-12., -12.], [12., -12.]];
    let mut rings = [Vec::new(), Vec::new()];
    let mut verticals = Vec::new();
    for (k, z) in [-3.5, 3.5].into_iter().enumerate() {
        for i in 0..4 {
            let j = (i + 1) % 4;
            let start = [corners[i][0], corners[i][1], z];
            let end = [corners[j][0], corners[j][1], z];
            rings[k].push(edges.len());
            edges.push(Edge {
                curve: Curve::Line {
                    origin: start,
                    dir: geom::sub(end, start),
                },
                start,
                end,
                samples: vec![start, end],
                vertices: (100 + k * 4 + i, 100 + k * 4 + j),
                same_sense: true,
            });
        }
    }
    for (i, p) in corners.into_iter().enumerate() {
        let start = [p[0], p[1], -3.5];
        let end = [p[0], p[1], 3.5];
        verticals.push(edges.len());
        edges.push(Edge {
            curve: Curve::Line {
                origin: start,
                dir: geom::sub(end, start),
            },
            start,
            end,
            samples: vec![start, end],
            vertices: (100 + i, 104 + i),
            same_sense: true,
        });
    }
    faces[1].loops[0].edges = rings[1].iter().map(|&e| (e, true)).collect();
    faces[11].loops[1].edges = rings[0].iter().rev().map(|&e| (e, false)).collect();
    for i in 0..4 {
        let edge = &edges[rings[0][i]];
        let x = geom::unit(geom::sub(edge.end, edge.start)).unwrap();
        let y = [0., 0., 1.];
        let f = Face {
            surface: Surface::Plane {
                frame: Frame {
                    origin: edge.start,
                    x,
                    y,
                    z: geom::cross(x, y),
                },
            },
            reversed: false,
            solid: Some(0),
            pcurves: vec![],
            loops: vec![Loop {
                edges: vec![
                    (rings[0][i], true),
                    (verticals[(i + 1) % 4], true),
                    (rings[1][i], false),
                    (verticals[i], false),
                ],
                vertex: None,
            }],
        };
        if i == 0 {
            faces[0] = f;
        } else {
            faces.push(f);
        }
    }
    let count = faces.len();
    Part::new(
        faces,
        edges,
        vec![Solid {
            faces: (0..count).collect(),
        }],
    )
}

#[test]
fn coaxial_walls_on_a_prismatic_host_still_define_face_grooves() {
    let part = prismatic_host();
    assert!(part.solid_is_geometrically_valid(0));
    assert_eq!(face_grooves::discover(&Context::new(&part)).len(), 2);
    let reading = quiddity::correspondence::recognise(&part);
    let doc = quiddity::recognition::document(&reading, part.faces.len());
    assert_eq!(
        doc.records
            .iter()
            .filter(|r| r.family == "face_grooves")
            .count(),
        2
    );
}

#[test]
fn coplanar_floor_splits_and_split_circular_boundaries_preserve_the_groove() {
    let base = two_ends();
    let mut faces = base.faces.clone();
    let mut edges = base.edges.clone();
    let Curve::Circle { frame, radius } = edges[3].curve else {
        panic!()
    };
    let angle = (-0.9_f64).acos();
    let arc = |a: f64, b: f64, vertices| Edge {
        curve: Curve::Circle { frame, radius },
        start: frame.to_world([radius * a.cos(), radius * a.sin(), 0.]),
        end: frame.to_world([radius * b.cos(), radius * b.sin(), 0.]),
        samples: (0..=32)
            .map(|j| {
                let t = a + (b - a) * j as f64 / 32.;
                frame.to_world([radius * t.cos(), radius * t.sin(), 0.])
            })
            .collect(),
        vertices,
        same_sense: true,
    };
    edges[3] = arc(0., angle, (3, 100));
    let b = edges.len();
    edges.push(arc(angle, TAU - angle, (100, 101)));
    let c = edges.len();
    edges.push(arc(TAU - angle, TAU, (101, 3)));
    let (start, end) = (edges[3].end, edges[b].end);
    let chord = edges.len();
    edges.push(Edge {
        curve: Curve::Line {
            origin: start,
            dir: geom::sub(end, start),
        },
        start,
        end,
        samples: vec![start, end],
        vertices: (100, 101),
        same_sense: true,
    });
    let wall = &mut faces[2].loops[0].edges;
    let at = wall.iter().position(|&(e, _)| e == 3).unwrap();
    wall.splice(at..=at, [(c, false), (b, false), (3, false)]);
    faces[3].loops[0].edges = vec![(3, true), (chord, true), (c, true)];
    let mut patch = faces[3].clone();
    patch.loops = vec![Loop {
        edges: vec![(b, true), (chord, false)],
        vertex: None,
    }];
    faces.push(patch);
    let count = faces.len();
    let part = Part::new(
        faces,
        edges,
        vec![Solid {
            faces: (0..count).collect(),
        }],
    );
    assert!(part.solid_is_geometrically_valid(0));
    let raw = face_grooves::discover(&Context::new(&part));
    assert_eq!(raw.len(), 2);
    assert_eq!(raw[0].record.floor, [3, 12]);
    assert_eq!(raw[0].record.faces, [2, 3, 4, 12]);
}

#[test]
fn two_conical_end_grooves_have_rim_dimensions_and_intrinsic_publication() {
    let base = two_ends();
    assert!(base.solid_is_geometrically_valid(0));
    let (s, c) = 0.713_f64.sin_cos();
    for motion in [
        IDENTITY,
        [[c, 0., s, 17.], [0., 1., 0., -31.], [-s, 0., c, 6.]],
    ] {
        let part = base.transformed(&motion).unwrap();
        let raw = face_grooves::discover(&Context::new(&part));
        assert_eq!(raw.len(), 2);
        for o in &raw {
            assert!((o.record.outer_diameter - 18.5).abs() < 1e-9);
            assert!((o.record.inner_diameter - 13.).abs() < 1e-9);
            assert!((o.record.depth - 0.25).abs() < 1e-9);
            assert!((o.record.outer_wall_angle - 45.).abs() < 1e-9);
            assert!((o.record.inner_wall_angle - 45.).abs() < 1e-9);
            assert_eq!(o.record.floor.len(), 1);
            assert_eq!(o.record.faces, o.defining);
        }
        let reading = quiddity::correspondence::recognise(&part);
        let doc = quiddity::recognition::document(&reading, part.faces.len());
        let records: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "face_grooves")
            .collect();
        assert_eq!(records.len(), 2, "{:#?}", doc.records);
        for r in records {
            assert_eq!(r.record_type, "FaceGroove");
            assert_eq!(
                r.evidence_status,
                quiddity::recognition::EvidenceStatus::Verified
            );
            assert_eq!(r.parameters["faces"], serde_json::json!(r.faces));
            let fp = reading
                .fingerprints
                .features
                .iter()
                .find(|f| f.id == r.id)
                .unwrap();
            assert_eq!(fp.sizes.len(), 5);
            assert!((fp.sizes["outer_diameter"] - 18.5).abs() < 1e-9);
            assert!(fp.axis.is_some());
            let axis: [f64; 3] = serde_json::from_value(r.parameters["axis"].clone()).unwrap();
            assert!(
                geom::norm(geom::cross(
                    axis,
                    [motion[0][2], motion[1][2], motion[2][2]]
                )) < 1e-9
            );
            let point: [f64; 3] =
                serde_json::from_value(r.parameters["axis_point"].clone()).unwrap();
            let local = geom::sub(point, [motion[0][3], motion[1][3], motion[2][3]]);
            assert!((geom::dot(local, axis) - 3.5).abs() < 1e-9);
        }
    }
}

#[test]
fn square_walls_keep_the_existing_annular_record_and_a_counterbore_stays_a_hole() {
    let square = revolve(
        &[
            (12., 0.),
            (12., 10.),
            (8., 10.),
            (8., 6.),
            (5., 6.),
            (5., 10.),
            (3., 10.),
            (3., 0.),
        ],
        &[],
    );
    assert!(square.solid_is_geometrically_valid(0));
    let raw = face_grooves::discover(&Context::new(&square));
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].record.outer_diameter, 16.);
    assert_eq!(raw[0].record.inner_diameter, 10.);
    assert_eq!(raw[0].record.depth, 4.);
    assert_eq!(raw[0].record.outer_wall_angle, 90.);
    assert_eq!(raw[0].record.inner_wall_angle, 90.);
    let existing = quiddity::features::recognise(&square);
    assert_eq!(existing.annular_grooves.len(), 1);
    assert!(existing.face_grooves.is_empty());
    let counterbore = revolve(
        &[
            (12., 0.),
            (12., 10.),
            (8., 10.),
            (8., 6.),
            (3., 6.),
            (3., 0.),
        ],
        &[],
    );
    assert!(counterbore.solid_is_geometrically_valid(0));
    assert!(face_grooves::discover(&Context::new(&counterbore)).is_empty());
    let holes = quiddity::features::recognise(&counterbore);
    assert!(!holes.holes.is_empty());
    assert!(holes.face_grooves.is_empty());
}

#[test]
fn tangent_floor_rounds_and_small_chamfers_are_included_in_the_proved_faces() {
    let rounded = revolve(
        &[
            (12., 0.),
            (12., 10.),
            (8., 10.),
            (8., 6.5),
            (7.5, 6.),
            (5.5, 6.),
            (5., 6.5),
            (5., 10.),
            (3., 10.),
            (3., 0.),
        ],
        &[(3, 7.5, 6.5, 0.5), (5, 5.5, 6.5, 0.5)],
    );
    assert!(rounded.solid_is_geometrically_valid(0));
    let raw = face_grooves::discover(&Context::new(&rounded));
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].defining, [2, 3, 4, 5, 6]);
    assert_eq!(raw[0].record.outer_wall_angle, 90.);
    assert_eq!(raw[0].record.inner_wall_angle, 90.);
    let rim_rounds = revolve(
        &[
            (12., 0.),
            (12., 10.),
            (8.5, 10.),
            (8., 9.5),
            (8., 6.),
            (5., 6.),
            (5., 9.5),
            (4.5, 10.),
            (3., 10.),
            (3., 0.),
        ],
        &[(2, 8.5, 9.5, 0.5), (6, 4.5, 9.5, 0.5)],
    );
    assert!(rim_rounds.solid_is_geometrically_valid(0));
    let raw = face_grooves::discover(&Context::new(&rim_rounds));
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].defining, [2, 3, 4, 5, 6]);
    assert_eq!(raw[0].record.outer_diameter, 17.);
    assert_eq!(raw[0].record.inner_diameter, 9.);
    assert_eq!(raw[0].record.depth, 4.);
    let chamfered = revolve(
        &[
            (12., 0.),
            (12., 10.),
            (8.25, 10.),
            (8., 9.75),
            (8., 6.),
            (5., 6.),
            (5., 9.75),
            (4.75, 10.),
            (3., 10.),
            (3., 0.),
        ],
        &[],
    );
    assert!(chamfered.solid_is_geometrically_valid(0));
    let raw = face_grooves::discover(&Context::new(&chamfered));
    assert_eq!(raw.len(), 1);
    assert_eq!(raw[0].defining, [2, 3, 4, 5, 6]);
    assert_eq!(raw[0].record.outer_diameter, 16.5);
    assert_eq!(raw[0].record.inner_diameter, 9.5);
    assert_eq!(raw[0].record.outer_wall_angle, 90.);
    assert_eq!(raw[0].record.inner_wall_angle, 90.);
}

#[test]
fn valid_undercuts_unequal_mouth_heights_and_radial_grooves_are_not_face_grooves() {
    for points in [
        // The outer wall leans over the floor: the mouth is narrower than the recess.
        vec![
            (12., 0.),
            (12., 10.),
            (8., 10.),
            (8.5, 6.),
            (5., 6.),
            (5., 10.),
            (3., 10.),
            (3., 0.),
        ],
        // Both walls rise, but their lands are on different planes.
        vec![
            (12., 0.),
            (12., 10.),
            (8., 10.),
            (8., 6.),
            (5., 6.),
            (5., 9.5),
            (3., 9.5),
            (3., 0.),
        ],
        // A radial shaft groove has a cylindrical floor, with transverse side walls.
        vec![
            (12., 0.),
            (12., 4.),
            (10., 4.),
            (10., 6.),
            (12., 6.),
            (12., 10.),
            (3., 10.),
            (3., 0.),
        ],
    ] {
        let part = revolve(&points, &[]);
        assert!(part.solid_is_geometrically_valid(0));
        assert!(face_grooves::discover(&Context::new(&part)).is_empty());
    }
}

#[test]
fn unequal_mouth_planes_undercuts_missing_ownership_and_noncoaxial_walls_are_refused() {
    for fault in 0..4 {
        let base = two_ends();
        let mut faces = base.faces.clone();
        let edges = base.edges.clone();
        match fault {
            0 => {
                if let Surface::Plane { frame } = &mut faces[1].surface {
                    frame.origin[2] += 0.1;
                }
            }
            1 => faces[2].reversed = false,
            2 => faces[3].solid = None,
            _ => {
                if let Surface::Cone { frame, .. } = &mut faces[2].surface {
                    frame.origin[0] += 0.1;
                }
            }
        }
        // Remove the opposite-end candidate so each fault must leave no groove.
        faces[9].solid = None;
        let part = Part::new(faces, edges, base.solids.clone());
        assert!(
            face_grooves::discover(&Context::new(&part)).is_empty(),
            "fault {fault}"
        );
    }
}
