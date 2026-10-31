//! Whole-solid classification, intrinsic any-axis proof and publication, for #123.
use super::common;
use quiddity::features::{self, Context, turned_bodies};
use quiddity::kernel::brep::Part;
use quiddity::kernel::geom::{self, Surface};

fn part(name: &str) -> Part {
    quiddity::read_step_file(&common::fixtures().join(name)).unwrap()
}
fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-8, "{a} != {b}");
}

#[test]
fn original_single_diameter_disc_has_bore_chamfers_and_complete_claims() {
    let p = part("turned_body_disc.step");
    let f = features::recognise(&p);
    assert_eq!(f.turned_bodies.len(), 1);
    let b = &f.turned_bodies[0];
    near(b.od, 22.);
    near(b.length, 7.);
    assert_eq!(serde_json::to_value(b.axis).unwrap(), "z");
    assert_eq!(b.bores.len(), 1);
    near(b.bores[0].diameter, 8.);
    near(b.bores[0].span[0], -3.5);
    near(b.bores[0].span[1], 3.5);
    assert_eq!(b.profile.len(), 6);
    assert_eq!(b.profile.iter().filter(|s| s.kind == "cone").count(), 2);
    assert_eq!(f.defining["turned_bodies"][0], (0..6).collect::<Vec<_>>());
    assert!(f.bosses.is_empty());
    assert_eq!(f.chamfers.len(), 2);
    assert_eq!(f.holes.len(), 1);
    assert!(f.turned_steps.is_empty());
    let raw_boss = features::bosses::discover(&Context::new(&p));
    assert_eq!(raw_boss.len(), 1);
    // For #134: include both 0.25 mm end chamfers, not only the cylinder.
    near(raw_boss[0].record.height, 7.);
    let key = b.profile_key.as_ref().unwrap();
    assert_eq!(key.axis, 'z');
    let fp = quiddity::correspondence::fingerprint::fingerprint(&p, &f);
    assert!(
        fp.features
            .iter()
            .any(|f| f.family == "turned_bodies" && !f.sizes.is_empty())
    );
    let recognised = quiddity::correspondence::recognise(&p);
    let doc = quiddity::recognition::document(&recognised, p.faces.len());
    for (face, account) in doc.face_accounting.unwrap().iter().enumerate() {
        let quiddity::recognition::FaceAccounting::Feature { record_ids } = account else {
            panic!("face {face}: {account:?}")
        };
        assert!(record_ids.contains(&"turned_bodies/0".into()));
    }
}

#[test]
fn stepped_shafts_keep_features_and_short_smaller_bosses() {
    for name in [
        "turned.step",
        "filleted_stepped_shaft.step",
        "turned_with_sphere.step",
        "golden_toroidal_blends_turned.step",
    ] {
        let p = part(name);
        let f = features::recognise(&p);
        assert_eq!(f.turned_bodies.len(), 1, "{name}");
        let accepted = features::inventory(&p).unwrap().accepted();
        for boss in accepted
            .bosses
            .iter()
            .filter(|b| b.diameter < f.turned_bodies[0].od - 1e-6)
        {
            assert!(f.bosses.contains(boss), "{name}: smaller end boss");
        }
        if name != "golden_toroidal_blends_turned.step" {
            assert!(!f.turned_steps.is_empty(), "{name}");
        }
    }
}

#[test]
fn complete_nurbs_plane_certificates_cover_captured_turned_shoulders() {
    for (name, count) in [
        ("9af1f99c3f98ddff.step.gz", 1),
        ("1590cfd2fcde58fd.step.gz", 2),
    ] {
        let p = part(&format!("captured/{name}"));
        let bodies = turned_bodies::discover(&Context::new(&p));
        assert_eq!(bodies.len(), count, "{name}");
        assert_eq!(
            bodies.iter().map(|b| b.defining.len()).sum::<usize>(),
            p.faces.len()
        );
    }
}

#[test]
fn meridian_fingerprints_preserve_arc_geometry_under_axis_reversal() {
    use quiddity::correspondence::fingerprint::{fingerprint, published_fingerprint};
    let motion = quiddity::framed_records::Rigid {
        rotation: [[1., 0., 0.], [0., -1., 0.], [0., 0., -1.]],
        translation: [17., -23., 9.],
    };
    for name in [
        "turned_body_disc.step",
        "turned_with_sphere.step",
        "golden_toroidal_blends_turned.step",
        "full_torus.step",
    ] {
        let p = part(name);
        let raw = features::recognise(&p);
        for segment in &raw.turned_bodies[0].profile {
            if let (Some(c), Some(radius), Some(sweep)) =
                (segment.arc_center, segment.arc_radius, segment.arc_sweep)
            {
                let theta = (segment.start[1] - c[1]).atan2(segment.start[0] - c[0]);
                for i in 0..=8 {
                    let s = c[0] + radius * (theta + sweep * i as f64 / 8.).cos();
                    assert!(
                        s >= segment.start[0] - 1e-8 && s <= segment.end[0] + 1e-8,
                        "{name}: nonmonotone meridian"
                    );
                }
            }
        }
        let before = fingerprint(&p, &raw)
            .features
            .into_iter()
            .find(|f| f.family == "turned_bodies")
            .unwrap();
        let moved = p
            .transformed(&[0, 1, 2].map(|i| {
                [
                    motion.rotation[i][0],
                    motion.rotation[i][1],
                    motion.rotation[i][2],
                    motion.translation[i],
                ]
            }))
            .unwrap();
        let mapped = quiddity::framed_records::map_features(raw, &motion);
        let after = published_fingerprint(&moved, &mapped.fingerprinted)
            .features
            .into_iter()
            .find(|f| f.family == "turned_bodies")
            .unwrap();
        assert_eq!(before.traits, after.traits, "{name}");
        assert_eq!(
            before.sizes.keys().collect::<Vec<_>>(),
            after.sizes.keys().collect::<Vec<_>>()
        );
        for (key, v) in &before.sizes {
            near(*v, after.sizes[key]);
        }
    }
}

/// Original D-section shaft: circular arc at R5, flat x=3, extruded 12 mm.
fn flat_shaft() -> Part {
    use quiddity::kernel::brep::{Edge, Face, Loop, Solid};
    use quiddity::kernel::geom::{Curve, Frame};
    let frame = |origin| Frame {
        origin,
        x: [1., 0., 0.],
        y: [0., 1., 0.],
        z: [0., 0., 1.],
    };
    let points = [[3., 4., 0.], [3., -4., 0.], [3., 4., 12.], [3., -4., 12.]];
    let theta = (3_f64 / 5.).acos();
    let mut edges = Vec::new();
    for (a, b) in [(0, 1), (2, 3), (1, 0), (3, 2), (0, 2), (1, 3)] {
        let (curve, samples) = if edges.len() < 2 {
            let c = Curve::Circle {
                frame: frame([0., 0., points[a][2]]),
                radius: 5.,
            };
            let samples = (0..=64)
                .map(|i| c.value(theta + (std::f64::consts::TAU - 2. * theta) * i as f64 / 64.))
                .collect();
            (c, samples)
        } else {
            (
                Curve::Line {
                    origin: points[a],
                    dir: geom::unit(geom::sub(points[b], points[a])).unwrap(),
                },
                vec![points[a], points[b]],
            )
        };
        edges.push(Edge {
            curve,
            start: points[a],
            end: points[b],
            vertices: (a, b),
            same_sense: true,
            samples,
        });
    }
    let mut faces = Vec::new();
    let mut add = |surface, reversed, uses| {
        faces.push(Face {
            surface,
            reversed,
            loops: vec![Loop {
                edges: uses,
                vertex: None,
            }],
            solid: Some(0),
            pcurves: vec![],
        })
    };
    add(
        Surface::Plane {
            frame: frame([0., 0., 0.]),
        },
        true,
        vec![(2, false), (0, false)],
    );
    add(
        Surface::Plane {
            frame: frame([0., 0., 12.]),
        },
        false,
        vec![(1, true), (3, true)],
    );
    add(
        Surface::Cylinder {
            frame: frame([0.; 3]),
            radius: 5.,
        },
        false,
        vec![(0, true), (5, true), (1, false), (4, false)],
    );
    add(
        Surface::Plane {
            frame: Frame {
                origin: [3., 0., 0.],
                x: [0., 1., 0.],
                y: [0., 0., 1.],
                z: [1., 0., 0.],
            },
        },
        false,
        vec![(2, true), (4, true), (3, false), (5, false)],
    );
    Part::new(
        faces,
        edges,
        vec![Solid {
            faces: (0..4).collect(),
        }],
    )
}

#[test]
fn a_flat_or_a_second_revolution_axis_refuses_the_whole_solid() {
    let p = flat_shaft();
    assert!(p.solid_is_geometrically_valid(0));
    assert!(turned_bodies::discover(&Context::new(&p)).is_empty());
    // Move only the bore axis in a fresh boundary. All supports remain revolved,
    // but their lines differ; cached geometry from the original must not leak.
    let p = part("turned_body_disc.step");
    let mut faces = p.faces.clone();
    let Surface::Cylinder {
        ref mut frame,
        radius,
    } = faces[4].surface
    else {
        panic!()
    };
    near(radius, 4.);
    frame.origin[0] = 0.01;
    let p = Part::new(faces, p.edges.clone(), p.solids.clone());
    assert!(turned_bodies::discover(&Context::new(&p)).is_empty());
}

#[test]
fn arbitrary_rotation_and_translation_preserve_the_disc_and_profile() {
    let p = part("turned_body_disc.step");
    let d = geom::unit([1., 2., 3.]).unwrap();
    let (s, c) = 37_f64.to_radians().sin_cos();
    let rotation = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let mut e = [0.; 3];
            e[j] = 1.;
            geom::add(
                geom::add(geom::scale(e, c), geom::scale(geom::cross(d, e), s)),
                geom::scale(d, geom::dot(d, e) * (1. - c)),
            )[i]
        })
    });
    let translation = [17., -23., 9.];
    let motion = [0, 1, 2].map(|i| {
        [
            rotation[i][0],
            rotation[i][1],
            rotation[i][2],
            translation[i],
        ]
    });
    let moved = p.transformed(&motion).unwrap();
    let f = features::recognise(&moved);
    assert_eq!(f.turned_bodies.len(), 1);
    let b = &f.turned_bodies[0];
    near(b.od, 22.);
    near(b.length, 7.);
    assert!(matches!(
        b.axis,
        features::cylindrical_profiles::ProfileAxis::Direction(_)
    ));
    for (a, b) in b.axis_point.iter().zip(translation) {
        near(*a, b);
    }
    assert!(f.bosses.is_empty());
    assert_eq!(f.holes.len(), 1);
    let published = quiddity::correspondence::recognise(&moved);
    assert_eq!(published.features.chamfers.len(), 2);
    assert_eq!(b.profile.iter().filter(|s| s.kind == "cone").count(), 2);
    // Exercise the exhaustive typed mapper, including an axis reversal.
    let raw = features::recognise(&p);
    for rotation in [rotation, [[1., 0., 0.], [0., -1., 0.], [0., 0., -1.]]] {
        let mapped = quiddity::framed_records::map_features(
            raw.clone(),
            &quiddity::framed_records::Rigid {
                rotation,
                translation,
            },
        );
        let json = serde_json::to_value(mapped.features).unwrap();
        near(json["turned_bodies"][0]["length"].as_f64().unwrap(), 7.);
    }
}

#[test]
fn spanner_flats_preserve_all_three_steps_and_explain_shoulders() {
    for name in ["spindle_plain.step", "spindle_flats.step"] {
        let p = part(name);
        let f = features::recognise(&p);
        assert_eq!(f.turned_steps.len(), 3, "{name}");
        assert_eq!(f.turned_bodies.len(), 1, "{name}");
        let mut sizes: Vec<_> = f
            .turned_steps
            .iter()
            .map(|s| (s.diameter, s.length()))
            .collect();
        sizes.sort_by(|a, b| a.0.total_cmp(&b.0));
        assert_eq!(sizes, vec![(10., 18.), (15., 22.), (24., 12.)]);
        near(f.turned_bodies[0].od, 24.);
        assert_eq!(
            f.flats.len(),
            if name == "spindle_flats.step" { 2 } else { 0 }
        );
        // Legitimate overlapping slab and shoulder descriptions. The published
        // riser's height_axis is file Y, so -12 is the head's radial extent.
        assert_eq!(f.plates.len(), 1);
        let reading = quiddity::correspondence::recognise(&p);
        let doc = quiddity::recognition::document(&reading, p.faces.len());
        let risers: Vec<_> = doc
            .records
            .iter()
            .filter(|r| r.family == "risers")
            .collect();
        assert_eq!(risers.len(), 1);
        let r = &risers[0].parameters;
        let height = match r["height_axis"].as_str().unwrap() {
            "x" => 0,
            "y" => 1,
            _ => panic!("transverse height axis"),
        };
        assert!(
            (r["z_lo"].as_f64().unwrap()
                - (p.bounds().min[height] - r["origin"][height].as_f64().unwrap()))
            .abs()
                < 1e-6
        );
        if name == "spindle_flats.step" {
            for face in [5, 7] {
                assert!(f.defining["turned_bodies"][0].contains(&face));
                assert!(
                    f.turned_bodies[0]
                        .profile
                        .iter()
                        .any(|s| s.face == face && s.kind == "plane_shoulder")
                );
            }
        }
    }
    for name in [
        "prismatic.step",
        "rejected_bored_box.step",
        "golden_polygonal_boss.step",
    ] {
        assert!(
            turned_bodies::discover(&Context::new(&part(name))).is_empty(),
            "{name}"
        );
    }
}
